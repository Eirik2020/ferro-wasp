//! STM32H743 four-motor DShot600 transmitter.
//!
//! The command lease, telemetry, completion, and fault policy are the shared
//! `DshotBank` state machine from `ferrowasp_stm32::dshot_bank`; this module
//! is the H7 hardware under it. The bank owns its timers, its four DMA
//! streams, and all DMA buffers as one fault-containment unit.
//!
//! Which timers and channels carry the motors, and on which streams, is the
//! board's: it hands over its timers, enabled and reset, as a `TimerSet`,
//! and names each motor's lane with `lane::<Timer, Channel, _>(stream)`. A
//! lane's compare register and DMAMUX request come from the timer's type and
//! the channel's, so they cannot disagree. Any channel of TIM2-TIM5 with a
//! DMA request can be a lane.
//!
//! The board latches the motor pads low as GPIO before building the bank and
//! connects them to their timers after it, so no pad sees a timer until its
//! outputs are configured and parked low.
//!
//! The timers start back to back in software; the skew is a few bus cycles
//! against a 1.67 us bit. Every lane writes 32-bit compare words: TIM2 and
//! TIM5 have 32-bit compare registers, and TIM3 and TIM4 ignore the upper
//! half.

use core::any::Any;
use core::marker::PhantomData;

use crate::dma_endpoints::{CompareChannel, StreamErrors, TimerCompareEndpoint};
use ferrowasp_stm32::dshot_bank::channel::LaneChannel;
use ferrowasp_waveform::dshot::{DshotPacket, DshotTiming, encode_compare_sequence};
use stm32h7xx_hal::{
    dma::{
        DBTransfer, MemoryToPeripheral, Transfer,
        dma::DmaConfig,
        traits::{DoubleBufferedStream, Stream},
    },
    pac::{TIM2, TIM3, TIM4, TIM5},
    rcc::CoreClocks,
};

pub use ferrowasp_stm32::dshot_bank::channel;
pub use ferrowasp_stm32::dshot_bank::{
    COMPARE_DMA_SLOTS, DSHOT_COMMAND_MAX, DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS,
    DSHOT600_BITRATE_HZ, DshotBank, DshotCommandError, DshotCommandSequence, DshotInitError,
    DshotInterruptEvent, DshotLanes, DshotMotor, DshotMotors, DshotServiceEvent,
    DshotSpecialCommandError, DshotStats, DshotTelemetryRequestError,
};

pub type DshotDmaBuffer = [u32; COMPARE_DMA_SLOTS];
type DshotBuffer = &'static mut DshotDmaBuffer;

/// A bank over the timer set `TS` and lanes `A`-`D`, for motors 1-4.
pub type DshotMotorBank<TS, A, B, C, D> = DshotBank<H7DshotLanes<TS, A, B, C, D>>;

/// Static storage for two DMA buffers per motor lane. It must be placed in
/// memory DMA2 can reach.
pub struct DshotDmaStorage {
    buffers: [DshotDmaBuffer; 8],
}

impl DshotDmaStorage {
    pub const fn new() -> Self {
        Self {
            buffers: [[0; COMPARE_DMA_SLOTS]; 8],
        }
    }
}

impl Default for DshotDmaStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// The register sequencing DShot needs from one timer, for the channels in
/// `channels` (bit n is channel n + 1).
pub trait LaneTimer: Any {
    /// Count at the DShot bit rate with the channels in PWM mode 1, preload
    /// on, outputs enabled active high, and every compare at zero.
    fn configure(&self, timing: DshotTiming, channels: u8);
    /// Stop the counter with the requests closed, load each channel's first
    /// duty from `first[channel]`, stage the counter at ARR so its first
    /// update latches them, and open the requests.
    fn stage(&self, channels: u8, first: &[u16; 4], period_end: u16);
    fn start(&self);
    /// Stop with the outputs held low: every compare at zero in PWM mode 1.
    fn stop(&self, channels: u8);
    /// Stop, then hold the outputs inactive whatever the compare values,
    /// until the bank is rebuilt.
    fn force_inactive(&self, channels: u8);
}

macro_rules! lane_timers {
    ($($TIM:ident),+) => {
        $(
            impl LaneTimer for $TIM {
                fn configure(&self, timing: DshotTiming, channels: u8) {
                    self.cr1.reset();
                    self.cr2.reset();
                    self.smcr.reset();
                    self.dier.reset();
                    self.ccer.reset();
                    self.ccmr1_output().reset();
                    self.ccmr2_output().reset();
                    self.psc.write(|w| w.psc().bits(0));
                    self.arr.write(|w| w.arr().bits(timing.arr.into()));
                    park(self);
                    self.cnt.write(|w| w.cnt().bits(0));
                    self.cr1.modify(|_, w| w.arpe().set_bit());
                    if channels & 0b0001 != 0 {
                        self.ccmr1_output()
                            .modify(|_, w| w.cc1s().output().oc1pe().enabled().oc1m().pwm_mode1());
                        self.ccer.modify(|_, w| w.cc1p().clear_bit().cc1e().set_bit());
                    }
                    if channels & 0b0010 != 0 {
                        self.ccmr1_output()
                            .modify(|_, w| w.cc2s().output().oc2pe().enabled().oc2m().pwm_mode1());
                        self.ccer.modify(|_, w| w.cc2p().clear_bit().cc2e().set_bit());
                    }
                    if channels & 0b0100 != 0 {
                        self.ccmr2_output()
                            .modify(|_, w| w.cc3s().output().oc3pe().enabled().oc3m().pwm_mode1());
                        self.ccer.modify(|_, w| w.cc3p().clear_bit().cc3e().set_bit());
                    }
                    if channels & 0b1000 != 0 {
                        self.ccmr2_output()
                            .modify(|_, w| w.cc4s().output().oc4pe().enabled().oc4m().pwm_mode1());
                        self.ccer.modify(|_, w| w.cc4p().clear_bit().cc4e().set_bit());
                    }
                    self.cr2.modify(|_, w| w.ccds().on_compare());
                    self.egr.write(|w| w.ug().set_bit());
                    self.sr.reset();
                }

                fn stage(&self, channels: u8, first: &[u16; 4], period_end: u16) {
                    set_requests(self, channels, false);
                    self.cr1.modify(|_, w| w.cen().clear_bit());
                    for (index, duty) in first.iter().enumerate() {
                        if channels & (1 << index) != 0 {
                            self.ccr[index].write(|w| w.ccr().bits((*duty).into()));
                        }
                    }
                    self.cnt.write(|w| w.cnt().bits(period_end.into()));
                    self.sr.reset();
                    set_requests(self, channels, true);
                }

                fn start(&self) {
                    self.cr1.modify(|_, w| w.cen().set_bit());
                }

                fn stop(&self, channels: u8) {
                    set_requests(self, channels, false);
                    self.cr1.modify(|_, w| w.cen().clear_bit());
                    park(self);
                    self.egr.write(|w| w.ug().set_bit());
                    self.sr.reset();
                }

                fn force_inactive(&self, channels: u8) {
                    self.stop(channels);
                    if channels & 0b0001 != 0 {
                        self.ccmr1_output().modify(|_, w| w.oc1m().force_inactive());
                    }
                    if channels & 0b0010 != 0 {
                        self.ccmr1_output().modify(|_, w| w.oc2m().force_inactive());
                    }
                    if channels & 0b0100 != 0 {
                        self.ccmr2_output().modify(|_, w| w.oc3m().force_inactive());
                    }
                    if channels & 0b1000 != 0 {
                        self.ccmr2_output().modify(|_, w| w.oc4m().force_inactive());
                    }
                }
            }

            impl ParkAndRequest for $TIM {
                fn park(&self) {
                    for ccr in &self.ccr {
                        ccr.write(|w| w.ccr().bits(0));
                    }
                }

                fn set_requests(&self, channels: u8, open: bool) {
                    self.dier.modify(|r, w| {
                        let bit = |n: u8, current: bool| {
                            if channels & (1 << n) != 0 { open } else { current }
                        };
                        w.cc1de()
                            .bit(bit(0, r.cc1de().bit()))
                            .cc2de()
                            .bit(bit(1, r.cc2de().bit()))
                            .cc3de()
                            .bit(bit(2, r.cc3de().bit()))
                            .cc4de()
                            .bit(bit(3, r.cc4de().bit()))
                    });
                }
            }
        )+
    };
}

/// Helpers the `LaneTimer` impls share, per timer register block.
trait ParkAndRequest {
    fn park(&self);
    fn set_requests(&self, channels: u8, open: bool);
}

fn park(timer: &impl ParkAndRequest) {
    timer.park();
}

fn set_requests(timer: &impl ParkAndRequest, channels: u8, open: bool) {
    timer.set_requests(channels, open);
}

lane_timers!(TIM2, TIM3, TIM4, TIM5);

/// The timers a bank drives, in slots. A lane finds its timer by type.
pub trait TimerSet: 'static {
    const LEN: usize;
    fn timer(&self, slot: usize) -> &dyn LaneTimer;
    fn find<T: 'static>(&self) -> Option<(usize, &T)>;
}

macro_rules! timer_sets {
    ($(($($T:ident: $slot:tt),+)),+) => {
        $(
            impl<$($T: LaneTimer),+> TimerSet for ($($T,)+) {
                const LEN: usize = [$($slot),+].len();

                fn timer(&self, slot: usize) -> &dyn LaneTimer {
                    match slot {
                        $($slot => &self.$slot,)+
                        _ => unreachable!("slot below LEN"),
                    }
                }

                fn find<T: 'static>(&self) -> Option<(usize, &T)> {
                    $(
                        if let Some(timer) = (&self.$slot as &dyn Any).downcast_ref::<T>() {
                            return Some(($slot, timer));
                        }
                    )+
                    None
                }
            }
        )+
    };
}

timer_sets!((A: 0), (A: 0, B: 1), (A: 0, B: 1, C: 2), (A: 0, B: 1, C: 2, D: 3));

/// A motor lane as the board names it: channel `C` of timer `T`, on DMA
/// `stream`.
pub struct LaneStream<S, T, C> {
    stream: S,
    _lane: PhantomData<(T, C)>,
}

/// Name a motor lane: channel `C` of timer `T`, carried by `stream`.
pub fn lane<T, C, S>(stream: S) -> LaneStream<S, T, C> {
    LaneStream {
        stream,
        _lane: PhantomData,
    }
}

/// A built lane: its timer slot, its channel, and its DMA transfer.
pub struct Lane<S, T, C>
where
    S: Stream,
    T: CompareChannel<C>,
    C: LaneChannel,
{
    slot: usize,
    transfer: Transfer<S, TimerCompareEndpoint<T, C>, MemoryToPeripheral, DshotBuffer, DBTransfer>,
}

/// One built lane, whatever its stream, timer, and channel.
pub trait LaneDma {
    fn slot(&self) -> usize;
    fn channel(&self) -> usize;
    fn exchange(&mut self, next: DshotBuffer) -> Result<DshotBuffer, DshotBuffer>;
    fn start(&mut self);
    fn status(&self) -> (bool, bool);
    fn pause_and_clear(&mut self);
}

impl<S, T, C> LaneDma for Lane<S, T, C>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    T: CompareChannel<C>,
    C: LaneChannel,
{
    fn slot(&self) -> usize {
        self.slot
    }

    fn channel(&self) -> usize {
        C::INDEX
    }

    fn exchange(&mut self, next: DshotBuffer) -> Result<DshotBuffer, DshotBuffer> {
        // Single-buffer mode: the HAL stops the stream, swaps the buffer, and
        // re-enables it; no request reaches it until the timer opens one.
        // That cannot fail; the error arm exists because double-buffer mode
        // can.
        let mut next = Some(next);
        let old = self
            .transfer
            .next_transfer_with(|old, _, _| match next.take() {
                Some(next) => (next, Some(old)),
                None => (old, None),
            });
        match (old, next) {
            (Ok(Some(old)), _) => Ok(old),
            (_, Some(next)) => Err(next),
            (Ok(None) | Err(_), None) => {
                unreachable!("the stream took the next buffer and returned the old one")
            }
        }
    }

    fn start(&mut self) {
        self.transfer.start(|_| {});
    }

    fn status(&self) -> (bool, bool) {
        let errors = S::error_flags();
        (
            self.transfer.get_transfer_complete_flag(),
            errors.transfer_error || errors.direct_mode_error,
        )
    }

    fn pause_and_clear(&mut self) {
        self.transfer.pause(|_| {});
        self.transfer.clear_interrupts();
    }
}

/// A lane the board named, to be built against the bank's timers.
pub trait IntoLane<TS> {
    type Lane: LaneDma;
    fn into_lane(self, timers: &TS, buffer: DshotBuffer) -> Result<Self::Lane, DshotInitError>;
}

impl<TS, S, T, C> IntoLane<TS> for LaneStream<S, T, C>
where
    TS: TimerSet,
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    T: CompareChannel<C> + 'static,
    C: LaneChannel,
{
    type Lane = Lane<S, T, C>;

    fn into_lane(self, timers: &TS, buffer: DshotBuffer) -> Result<Self::Lane, DshotInitError> {
        let (slot, timer) = timers.find::<T>().ok_or(DshotInitError::LaneTimerMissing)?;
        buffer.fill(0);
        let transfer = Transfer::init(
            self.stream,
            TimerCompareEndpoint::<T, C>::new(timer),
            buffer,
            None,
            dma_config(),
        );
        Ok(Lane { slot, transfer })
    }
}

fn dma_config() -> DmaConfig {
    DmaConfig::default()
        .priority(stm32h7xx_hal::dma::config::Priority::VeryHigh)
        .memory_increment(true)
        .peripheral_increment(false)
        .transfer_complete_interrupt(true)
        .transfer_error_interrupt(true)
        .direct_mode_error_interrupt(true)
}

/// The timers and four lanes under a `DshotMotorBank`.
pub struct H7DshotLanes<TS, A, B, C, D> {
    timers: TS,
    /// Each timer slot's lane channels, bit n for channel n + 1.
    channels: [u8; 4],
    period_end: u16,
    motor1: A,
    motor2: B,
    motor3: C,
    motor4: D,
}

impl<TS, A, B, C, D> H7DshotLanes<TS, A, B, C, D>
where
    TS: TimerSet,
    A: LaneDma,
    B: LaneDma,
    C: LaneDma,
    D: LaneDma,
{
    fn lane(&self, motor: DshotMotor) -> &dyn LaneDma {
        match motor {
            DshotMotor::Motor1 => &self.motor1,
            DshotMotor::Motor2 => &self.motor2,
            DshotMotor::Motor3 => &self.motor3,
            DshotMotor::Motor4 => &self.motor4,
        }
    }

    fn lane_mut(&mut self, motor: DshotMotor) -> &mut dyn LaneDma {
        match motor {
            DshotMotor::Motor1 => &mut self.motor1,
            DshotMotor::Motor2 => &mut self.motor2,
            DshotMotor::Motor3 => &mut self.motor3,
            DshotMotor::Motor4 => &mut self.motor4,
        }
    }

    /// The slots that carry at least one lane, each with its channels.
    fn used_slots(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        (0..TS::LEN).filter_map(|slot| {
            let channels = self.channels[slot];
            (channels != 0).then_some((slot, channels))
        })
    }
}

impl<TS, A, B, C, D> DshotLanes for H7DshotLanes<TS, A, B, C, D>
where
    TS: TimerSet,
    A: LaneDma,
    B: LaneDma,
    C: LaneDma,
    D: LaneDma,
{
    type Buffer = DshotDmaBuffer;

    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut DshotDmaBuffer) -> u16 {
        let mut compares = [0; COMPARE_DMA_SLOTS];
        let first = encode_compare_sequence(packet, timing, &mut compares);
        for (word, compare) in buffer.iter_mut().zip(compares) {
            *word = u32::from(compare);
        }
        first
    }

    fn exchange_buffer(
        &mut self,
        motor: DshotMotor,
        next: DshotBuffer,
    ) -> Result<DshotBuffer, DshotBuffer> {
        self.lane_mut(motor).exchange(next)
    }

    fn start_dma(&mut self, motor: DshotMotor) {
        self.lane_mut(motor).start();
    }

    fn start_frame(&mut self, first_duties: [u16; 4]) {
        // Each timer slot's first duties, by channel.
        let mut first = [[0u16; 4]; 4];
        for (motor, duty) in DshotMotor::ALL.into_iter().zip(first_duties) {
            let lane = self.lane(motor);
            first[lane.slot()][lane.channel()] = duty;
        }
        for (slot, channels) in self.used_slots() {
            self.timers
                .timer(slot)
                .stage(channels, &first[slot], self.period_end);
        }
        for (slot, _) in self.used_slots() {
            self.timers.timer(slot).start();
        }
    }

    fn stop_frame(&mut self) {
        for (slot, channels) in self.used_slots() {
            self.timers.timer(slot).stop(channels);
        }
    }

    fn force_outputs_low(&mut self) {
        for (slot, channels) in self.used_slots() {
            self.timers.timer(slot).force_inactive(channels);
        }
    }

    fn dma_status(&self, motor: DshotMotor) -> (bool, bool) {
        self.lane(motor).status()
    }

    fn pause_and_clear(&mut self, motor: DshotMotor) {
        self.lane_mut(motor).pause_and_clear();
    }
}

/// Build a bank over `timers`, enabled and reset, with `lanes` for motors
/// 1-4. Each timer that carries a lane is configured with its outputs parked
/// low; the board connects the motor pads afterwards.
#[allow(clippy::type_complexity)]
pub fn init_dshot_motor_bank<TS, LA, LB, LC, LD>(
    timers: TS,
    lanes: (LA, LB, LC, LD),
    clocks: &CoreClocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank<TS, LA::Lane, LB::Lane, LC::Lane, LD::Lane>, DshotInitError>
where
    TS: TimerSet,
    LA: IntoLane<TS>,
    LB: IntoLane<TS>,
    LC: IntoLane<TS>,
    LD: IntoLane<TS>,
{
    let timing = DshotTiming::from_clocks(clocks.timx_ker_ck().raw(), DSHOT600_BITRATE_HZ)
        .map_err(DshotInitError::InvalidTiming)?;

    let [
        motor1_active,
        motor1_spare,
        motor2_active,
        motor2_spare,
        motor3_active,
        motor3_spare,
        motor4_active,
        motor4_spare,
    ] = storage.buffers.each_mut();
    let motor1 = lanes.0.into_lane(&timers, motor1_active)?;
    let motor2 = lanes.1.into_lane(&timers, motor2_active)?;
    let motor3 = lanes.2.into_lane(&timers, motor3_active)?;
    let motor4 = lanes.3.into_lane(&timers, motor4_active)?;

    let mut channels = [0u8; 4];
    for lane in [
        &motor1 as &dyn LaneDma,
        &motor2 as &dyn LaneDma,
        &motor3 as &dyn LaneDma,
        &motor4 as &dyn LaneDma,
    ] {
        let bit = 1 << lane.channel();
        if channels[lane.slot()] & bit != 0 {
            return Err(DshotInitError::LaneChannelReused);
        }
        channels[lane.slot()] |= bit;
    }
    for (slot, slot_channels) in channels.iter().enumerate().take(TS::LEN) {
        if *slot_channels != 0 {
            timers.timer(slot).configure(timing, *slot_channels);
        }
    }

    let lanes = H7DshotLanes {
        timers,
        channels,
        period_end: timing.arr,
        motor1,
        motor2,
        motor3,
        motor4,
    };
    Ok(DshotBank::from_lanes(
        lanes,
        [motor1_spare, motor2_spare, motor3_spare, motor4_spare],
        timing,
    ))
}
