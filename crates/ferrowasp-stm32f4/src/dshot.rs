//! STM32F405 four-motor DShot600 transmitter.
//!
//! The command lease, telemetry, completion, and fault policy are the shared
//! `DshotBank` state machine; this module is the STM32F405 hardware under
//! it. The bank owns its timers, its four DMA streams, and all DMA buffers
//! as one fault-containment unit. A frame set is complete only after every
//! DMA lane completes.
//!
//! Which timers and channels carry the motors, and on which streams, is the
//! board's: it hands over its timers as a `TimerSet`, names each motor's
//! lane with `lane::<Timer, Channel, DMA_CHANNEL, _>(stream)`, and chooses
//! how the timers start: in software, one after another, or from one
//! timer's trigger output so every lane begins on the same edge. The board
//! latches the motor pads low as GPIO before building the bank and connects
//! them afterwards, so no pad sees a timer until its outputs are configured.
//!
//! Every lane timer must run from the same timer clock: one bank encodes
//! all lanes with one bit timing.
//!
//! # Unsafe boundary
//!
//! This is the only project-local unsafe boundary used by the DShot backend.
//! It proves these STM32F405 hardware facts to the HAL:
//!
//! - a lane endpoint's address is the compare register of channel `C` of a
//!   timer its caller owns;
//! - compare-register DMA transfers are 16-bit halfwords;
//! - each declared stream/channel route carries that timer channel's
//!   compare request (RM0090 table 43), memory to peripheral.
//!
//! Endpoints are built only from a reference to an owned timer, while the
//! bank takes exclusive ownership of it. DMA buffer ownership, stream
//! fencing, and buffer exchange remain delegated to
//! `stm32f4xx-hal::dma::Transfer`.

#![deny(unsafe_op_in_unsafe_fn)]

use core::any::Any;
use core::marker::PhantomData;

use ferrowasp_stm32::dshot_bank::channel::{Ch1, Ch3, Ch3N, Ch4, LaneChannel};
use ferrowasp_waveform::dshot::{
    COMPARE_DMA_SLOTS, DSHOT600_BITRATE_HZ, DshotPacket, DshotTiming, encode_compare_sequence,
};
use stm32f4xx_hal::{
    dma::{
        ChannelX, DMAError, MemoryToPeripheral, Stream1, Stream2, Stream6, Stream7, Transfer,
        config::{DmaConfig, Priority},
        traits::{Channel, DMASet, PeriAddress, StreamISR},
    },
    pac::{DMA2, TIM1, TIM8, tim1::RegisterBlock},
    rcc::Clocks,
};

pub use ferrowasp_stm32::dshot_bank::channel;
pub use ferrowasp_stm32::dshot_bank::{
    DSHOT_COMMAND_MAX, DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS, DshotBank,
    DshotCommandError, DshotCommandSequence, DshotInitError, DshotInterruptEvent, DshotLanes,
    DshotMotor, DshotMotors, DshotServiceEvent, DshotSpecialCommandError, DshotStats,
    DshotTelemetryRequestError,
};

pub type DshotDmaBuffer = [u16; COMPARE_DMA_SLOTS];
type DshotBuffer = &'static mut DshotDmaBuffer;

/// A bank over the timer set `TS` and lanes `A`-`D`, for motors 1-4.
pub type DshotMotorBank<TS, A, B, C, D> = DshotBank<Stm32f4DshotLanes<TS, A, B, C, D>>;

/// Static storage for two DMA buffers per motor lane.
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

/// Which outputs of one timer the bank drives: bit n is channel n + 1, and
/// a bit also set in `complementary` drives that channel's CHxN output
/// instead of CHx.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LaneOutputs {
    pub channels: u8,
    pub complementary: u8,
}

/// The register sequencing DShot needs from one timer.
pub trait LaneTimer: Any {
    /// The clock this timer counts, before its prescaler.
    fn timer_clock_hz(&self, clocks: &Clocks) -> u32;
    /// Count at the DShot bit rate with the outputs in PWM mode 1, preload
    /// on, enabled active high, every compare at zero, and the main output
    /// gate open.
    fn configure(&self, timing: DshotTiming, outputs: LaneOutputs);
    /// Stop the counter with the requests closed, load each channel's first
    /// duty from `first[channel]`, stage the counter at ARR so its first
    /// update latches them, and open the requests.
    fn stage(&self, channels: u8, first: &[u16; 4], period_end: u16);
    fn start(&self);
    /// Stop with every output held low: all compares at zero.
    fn stop(&self, channels: u8);
    /// Stop, then close the main output gate and disable the outputs until
    /// the bank is rebuilt.
    fn force_inactive(&self, outputs: LaneOutputs);
}

/// TIM1 and TIM8 share the advanced-timer register block.
macro_rules! advanced_lane_timers {
    ($($TIM:ident),+) => {
        $(
            impl LaneTimer for $TIM {
                fn timer_clock_hz(&self, clocks: &Clocks) -> u32 {
                    clocks.timclk2().raw()
                }

                fn configure(&self, timing: DshotTiming, outputs: LaneOutputs) {
                    configure_advanced(self, timing, outputs);
                }

                fn stage(&self, channels: u8, first: &[u16; 4], period_end: u16) {
                    stage(self, channels, first, period_end);
                }

                fn start(&self) {
                    self.cr1().modify(|_, w| w.cen().set_bit());
                }

                fn stop(&self, channels: u8) {
                    stop(self, channels);
                }

                fn force_inactive(&self, outputs: LaneOutputs) {
                    stop(self, outputs.channels);
                    self.bdtr().modify(|_, w| w.moe().clear_bit());
                    set_outputs_enabled(self, outputs, false);
                }
            }
        )+
    };
}

advanced_lane_timers!(TIM1, TIM8);

fn configure_advanced(timer: &RegisterBlock, timing: DshotTiming, outputs: LaneOutputs) {
    timer.cr1().reset();
    timer.cr2().reset();
    timer.smcr().reset();
    timer.dier().reset();
    timer.sr().reset();
    timer.ccer().reset();
    timer.bdtr().reset();
    timer.ccmr1_output().reset();
    timer.ccmr2_output().reset();
    timer.psc().write(|w| w.psc().set(0));
    timer.rcr().write(|w| w.rep().set(0));
    timer.arr().write(|w| w.arr().set(timing.arr));
    write_all_compares_zero(timer);
    timer.cnt().write(|w| w.cnt().set(0));
    timer.cr1().modify(|_, w| w.arpe().set_bit());

    let used = |index: u8| outputs.channels & (1 << index) != 0;
    if used(0) {
        timer
            .ccmr1_output()
            .modify(|_, w| w.cc1s().output().oc1pe().enabled().oc1m().pwm_mode1());
    }
    if used(1) {
        timer
            .ccmr1_output()
            .modify(|_, w| w.cc2s().output().oc2pe().enabled().oc2m().pwm_mode1());
    }
    if used(2) {
        timer
            .ccmr2_output()
            .modify(|_, w| w.cc3s().output().oc3pe().enabled().oc3m().pwm_mode1());
    }
    if used(3) {
        timer
            .ccmr2_output()
            .modify(|_, w| w.cc4s().output().oc4pe().enabled().oc4m().pwm_mode1());
    }
    timer.cr2().modify(|_, w| w.ccds().on_compare());

    // A complementary lane drives CHxN with CHx disabled. RM0090 table 96
    // defines that case as OCxN = OCxREF xor CCxNP, so active-high polarity
    // keeps CCxNP clear for the same pulse shape as an ordinary output.
    set_outputs_enabled(timer, outputs, true);
    timer.cr2().modify(|_, w| {
        w.ois1()
            .clear_bit()
            .ois1n()
            .clear_bit()
            .ois2()
            .clear_bit()
            .ois2n()
            .clear_bit()
            .ois3()
            .clear_bit()
            .ois3n()
            .clear_bit()
            .ois4()
            .clear_bit()
    });
    timer.bdtr().modify(|_, w| {
        w.ossi()
            .set_bit()
            .ossr()
            .set_bit()
            .bke()
            .clear_bit()
            .aoe()
            .clear_bit()
            .moe()
            .set_bit()
    });
    force_update(timer);
    timer.sr().reset();
}

/// Enable (or disable) each lane's output, active high: CHx for an ordinary
/// lane, CHxN with CHx off for a complementary one.
fn set_outputs_enabled(timer: &RegisterBlock, outputs: LaneOutputs, enabled: bool) {
    let normal = |index: u8| {
        outputs.channels & (1 << index) != 0 && outputs.complementary & (1 << index) == 0
    };
    let complementary = |index: u8| {
        outputs.channels & (1 << index) != 0 && outputs.complementary & (1 << index) != 0
    };
    timer.ccer().modify(|_, w| {
        if normal(0) {
            w.cc1p().clear_bit().cc1e().bit(enabled);
        }
        if complementary(0) {
            w.cc1p().clear_bit().cc1e().clear_bit();
            w.cc1np().active_high().cc1ne().bit(enabled);
        }
        if normal(1) {
            w.cc2p().clear_bit().cc2e().bit(enabled);
        }
        if complementary(1) {
            w.cc2p().clear_bit().cc2e().clear_bit();
            w.cc2np().active_high().cc2ne().bit(enabled);
        }
        if normal(2) {
            w.cc3p().clear_bit().cc3e().bit(enabled);
        }
        if complementary(2) {
            w.cc3p().clear_bit().cc3e().clear_bit();
            w.cc3np().active_high().cc3ne().bit(enabled);
        }
        if normal(3) {
            w.cc4p().clear_bit().cc4e().bit(enabled);
        }
        w
    });
}

fn stage(timer: &RegisterBlock, channels: u8, first: &[u16; 4], period_end: u16) {
    set_requests(timer, channels, false);
    timer.cr1().modify(|_, w| w.cen().clear_bit());
    for (index, duty) in first.iter().enumerate() {
        if channels & (1 << index) != 0 {
            timer.ccr(index).write(|w| w.ccr().set(*duty));
        }
    }
    timer.cnt().write(|w| w.cnt().set(period_end));
    timer.sr().reset();
    set_requests(timer, channels, true);
}

fn stop(timer: &RegisterBlock, channels: u8) {
    set_requests(timer, channels, false);
    timer.cr1().modify(|_, w| w.cen().clear_bit());
    write_all_compares_zero(timer);
    force_update(timer);
    timer.sr().reset();
}

fn set_requests(timer: &RegisterBlock, channels: u8, open: bool) {
    timer.dier().modify(|r, w| {
        let bit = |index: u8, current: bool| {
            if channels & (1 << index) != 0 {
                open
            } else {
                current
            }
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

fn write_all_compares_zero(timer: &RegisterBlock) {
    for index in 0..4 {
        timer.ccr(index).write(|w| w.ccr().set(0));
    }
}

fn force_update(timer: &RegisterBlock) {
    timer.egr().write(|w| w.ug().update());
}

/// A timer whose trigger output can start another: its TRGO follows its
/// counter enable.
pub trait TriggerMaster: LaneTimer {
    fn lead(&self);
}

/// A timer that `M`'s trigger output can start: the internal trigger input
/// wired to `M` (RM0090 table 94), in trigger mode.
pub trait TriggerSlave<M>: LaneTimer {
    fn follow(&self);
}

impl TriggerMaster for TIM1 {
    fn lead(&self) {
        self.cr2().modify(|_, w| w.mms().enable());
    }
}

impl TriggerSlave<TIM1> for TIM8 {
    fn follow(&self) {
        self.smcr()
            .modify(|_, w| w.ts().itr0().sms().trigger_mode());
    }
}

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

/// How a bank's timers start each frame set.
pub trait StartMode<TS> {
    /// Wire the timers up; return the slot that is started by trigger
    /// rather than in software, if any.
    fn wire(&self, timers: &TS) -> Result<Option<usize>, DshotInitError>;
}

/// Start every timer in software, one after another.
pub struct SoftwareStart;

impl<TS: TimerSet> StartMode<TS> for SoftwareStart {
    fn wire(&self, _timers: &TS) -> Result<Option<usize>, DshotInitError> {
        Ok(None)
    }
}

/// Start `M` in software and `S` from `M`'s trigger output, so both begin
/// on the same edge.
pub struct TriggeredStart<M, S>(PhantomData<(M, S)>);

impl<M, S> TriggeredStart<M, S> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<M, S> Default for TriggeredStart<M, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<TS, M, S> StartMode<TS> for TriggeredStart<M, S>
where
    TS: TimerSet,
    M: TriggerMaster,
    S: TriggerSlave<M>,
{
    fn wire(&self, timers: &TS) -> Result<Option<usize>, DshotInitError> {
        let (_, master) = timers.find::<M>().ok_or(DshotInitError::LaneTimerMissing)?;
        let (slave_slot, slave) = timers.find::<S>().ok_or(DshotInitError::LaneTimerMissing)?;
        master.lead();
        slave.follow();
        Ok(Some(slave_slot))
    }
}

/// A timer channel's compare register, as a DMA destination.
pub struct CompareEndpoint<T, C> {
    address: u32,
    _channel: PhantomData<(T, C)>,
}

/// A timer that has compare channel `C`: the address of its CCRx.
pub trait CompareRegister<C: LaneChannel> {
    fn compare_address(&self) -> u32;
}

impl<C: LaneChannel> CompareRegister<C> for TIM1 {
    fn compare_address(&self) -> u32 {
        self.ccr(C::INDEX).as_ptr() as u32
    }
}

impl<C: LaneChannel> CompareRegister<C> for TIM8 {
    fn compare_address(&self) -> u32 {
        self.ccr(C::INDEX).as_ptr() as u32
    }
}

impl<T, C> CompareEndpoint<T, C>
where
    T: CompareRegister<C>,
    C: LaneChannel,
{
    fn new(timer: &T) -> Self {
        Self {
            address: timer.compare_address(),
            _channel: PhantomData,
        }
    }
}

// SAFETY: `new` takes the address of channel `C`'s compare register from a
// timer its caller owns, and the STM32F405 compare registers accept 16-bit
// DMA writes.
unsafe impl<T, C> PeriAddress for CompareEndpoint<T, C>
where
    T: CompareRegister<C>,
    C: LaneChannel,
{
    type MemSize = u16;

    #[inline]
    fn address(&self) -> u32 {
        self.address
    }
}

/// Declare stream/channel routes for timer compare requests. Each line must
/// match RM0090 table 43; a channel's complementary lane uses the same
/// compare register and request as the channel itself.
macro_rules! compare_routes {
    ($($TIM:ident [$($C:ident),+] => $stream:ty, $channel:literal;)+) => {
        $($(
            // SAFETY: RM0090 table 43 maps this STM32F405 timer compare
            // request to the declared DMA2 stream and channel, memory to
            // peripheral.
            unsafe impl DMASet<$stream, $channel, MemoryToPeripheral>
                for CompareEndpoint<$TIM, $C>
            {
            }
        )+)+
    };
}

compare_routes! {
    TIM1 [Ch1] => Stream1<DMA2>, 6;
    TIM1 [Ch3, Ch3N] => Stream6<DMA2>, 6;
    TIM8 [Ch3] => Stream2<DMA2>, 0;
    TIM8 [Ch4] => Stream7<DMA2>, 7;
}

/// A motor lane as the board names it: channel `C` of timer `T`, on DMA
/// `stream` and stream channel `DMA_CHANNEL`.
pub struct LaneStream<S, T, C, const DMA_CHANNEL: u8> {
    stream: S,
    _lane: PhantomData<(T, C)>,
}

/// Name a motor lane: channel `C` of timer `T`, carried by `stream` on its
/// channel `DMA_CHANNEL`. The route must be one declared above.
pub fn lane<T, C, const DMA_CHANNEL: u8, S>(stream: S) -> LaneStream<S, T, C, DMA_CHANNEL> {
    LaneStream {
        stream,
        _lane: PhantomData,
    }
}

type LaneTransfer<S, T, C, const DMA_CHANNEL: u8> =
    Transfer<S, DMA_CHANNEL, CompareEndpoint<T, C>, MemoryToPeripheral, DshotBuffer>;

/// A built lane: its timer slot and its DMA transfer.
pub struct Lane<S, T, C, const DMA_CHANNEL: u8>
where
    S: stm32f4xx_hal::dma::traits::Stream,
    ChannelX<DMA_CHANNEL>: Channel,
    T: CompareRegister<C>,
    C: LaneChannel,
    CompareEndpoint<T, C>: DMASet<S, DMA_CHANNEL, MemoryToPeripheral>,
{
    slot: usize,
    transfer: LaneTransfer<S, T, C, DMA_CHANNEL>,
}

/// One built lane, whatever its stream, timer, and channel.
pub trait LaneDma {
    fn slot(&self) -> usize;
    fn channel(&self) -> usize;
    fn complementary(&self) -> bool;
    fn exchange(&mut self, next: DshotBuffer) -> Result<DshotBuffer, DshotBuffer>;
    fn start(&mut self);
    fn status(&self) -> (bool, bool);
    fn pause_and_clear(&mut self);
}

impl<S, T, C, const DMA_CHANNEL: u8> LaneDma for Lane<S, T, C, DMA_CHANNEL>
where
    S: stm32f4xx_hal::dma::traits::Stream + StreamISR,
    ChannelX<DMA_CHANNEL>: Channel,
    T: CompareRegister<C>,
    C: LaneChannel,
    CompareEndpoint<T, C>: DMASet<S, DMA_CHANNEL, MemoryToPeripheral>,
{
    fn slot(&self) -> usize {
        self.slot
    }

    fn channel(&self) -> usize {
        C::INDEX
    }

    fn complementary(&self) -> bool {
        C::COMPLEMENTARY
    }

    fn exchange(&mut self, next: DshotBuffer) -> Result<DshotBuffer, DshotBuffer> {
        self.transfer
            .next_transfer(next)
            .map(|(old, _)| old)
            .map_err(dma_error_buffer)
    }

    fn start(&mut self) {
        self.transfer.start(|_| {});
    }

    fn status(&self) -> (bool, bool) {
        (
            self.transfer.is_transfer_complete(),
            self.transfer.is_transfer_error() || self.transfer.is_direct_mode_error(),
        )
    }

    fn pause_and_clear(&mut self) {
        self.transfer.pause(|_| {});
        self.transfer.clear_transfer_complete();
        self.transfer.clear_half_transfer();
        self.transfer.clear_transfer_error();
        self.transfer.clear_direct_mode_error();
        self.transfer.clear_fifo_error();
    }
}

fn dma_error_buffer(error: DMAError<DshotBuffer>) -> DshotBuffer {
    match error {
        DMAError::NotReady(buffer) | DMAError::SmallBuffer(buffer) | DMAError::Overrun(buffer) => {
            buffer
        }
    }
}

/// A lane the board named, to be built against the bank's timers.
pub trait IntoLane<TS> {
    type Lane: LaneDma;
    fn into_lane(self, timers: &TS, buffer: DshotBuffer) -> Result<Self::Lane, DshotInitError>;
}

impl<TS, S, T, C, const DMA_CHANNEL: u8> IntoLane<TS> for LaneStream<S, T, C, DMA_CHANNEL>
where
    TS: TimerSet,
    S: stm32f4xx_hal::dma::traits::Stream + StreamISR,
    ChannelX<DMA_CHANNEL>: Channel,
    T: CompareRegister<C> + 'static,
    C: LaneChannel,
    CompareEndpoint<T, C>: DMASet<S, DMA_CHANNEL, MemoryToPeripheral>,
{
    type Lane = Lane<S, T, C, DMA_CHANNEL>;

    fn into_lane(self, timers: &TS, buffer: DshotBuffer) -> Result<Self::Lane, DshotInitError> {
        let (slot, timer) = timers.find::<T>().ok_or(DshotInitError::LaneTimerMissing)?;
        buffer.fill(0);
        let transfer = Transfer::init_memory_to_peripheral(
            self.stream,
            CompareEndpoint::<T, C>::new(timer),
            buffer,
            None,
            dma_config(),
        );
        Ok(Lane { slot, transfer })
    }
}

fn dma_config() -> DmaConfig {
    DmaConfig::default()
        .priority(Priority::VeryHigh)
        .memory_increment(true)
        .peripheral_increment(false)
        .transfer_complete_interrupt(true)
        .transfer_error_interrupt(true)
        .direct_mode_error_interrupt(true)
}

/// The timers and four lanes under a `DshotMotorBank`.
pub struct Stm32f4DshotLanes<TS, A, B, C, D> {
    timers: TS,
    /// Each timer slot's lane outputs.
    outputs: [LaneOutputs; 4],
    /// The slot started by another timer's trigger, if any.
    triggered_slot: Option<usize>,
    period_end: u16,
    motor1: A,
    motor2: B,
    motor3: C,
    motor4: D,
}

impl<TS, A, B, C, D> Stm32f4DshotLanes<TS, A, B, C, D>
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

    /// The slots that carry at least one lane, each with its outputs.
    fn used_slots(&self) -> impl Iterator<Item = (usize, LaneOutputs)> + '_ {
        (0..TS::LEN).filter_map(|slot| {
            let outputs = self.outputs[slot];
            (outputs.channels != 0).then_some((slot, outputs))
        })
    }
}

impl<TS, A, B, C, D> DshotLanes for Stm32f4DshotLanes<TS, A, B, C, D>
where
    TS: TimerSet,
    A: LaneDma,
    B: LaneDma,
    C: LaneDma,
    D: LaneDma,
{
    type Buffer = DshotDmaBuffer;

    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut DshotDmaBuffer) -> u16 {
        encode_compare_sequence(packet, timing, buffer)
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
        for (slot, outputs) in self.used_slots() {
            self.timers
                .timer(slot)
                .stage(outputs.channels, &first[slot], self.period_end);
        }
        // A triggered timer starts from its master's trigger output.
        for (slot, _) in self.used_slots() {
            if Some(slot) != self.triggered_slot {
                self.timers.timer(slot).start();
            }
        }
    }

    fn stop_frame(&mut self) {
        for (slot, outputs) in self.used_slots() {
            self.timers.timer(slot).stop(outputs.channels);
        }
    }

    fn force_outputs_low(&mut self) {
        for (slot, outputs) in self.used_slots() {
            self.timers.timer(slot).force_inactive(outputs);
        }
    }

    fn dma_status(&self, motor: DshotMotor) -> (bool, bool) {
        self.lane(motor).status()
    }

    fn pause_and_clear(&mut self, motor: DshotMotor) {
        self.lane_mut(motor).pause_and_clear();
    }
}

/// Build a bank over `timers` with `lanes` for motors 1-4, started as
/// `start` says. Each timer that carries a lane is configured with its
/// outputs parked low; the board connects the motor pads afterwards.
#[allow(clippy::type_complexity)]
pub fn init_dshot_motor_bank<TS, LA, LB, LC, LD>(
    timers: TS,
    lanes: (LA, LB, LC, LD),
    start: impl StartMode<TS>,
    clocks: &Clocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank<TS, LA::Lane, LB::Lane, LC::Lane, LD::Lane>, DshotInitError>
where
    TS: TimerSet,
    LA: IntoLane<TS>,
    LB: IntoLane<TS>,
    LC: IntoLane<TS>,
    LD: IntoLane<TS>,
{
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

    let mut outputs = [LaneOutputs::default(); 4];
    for lane in [
        &motor1 as &dyn LaneDma,
        &motor2 as &dyn LaneDma,
        &motor3 as &dyn LaneDma,
        &motor4 as &dyn LaneDma,
    ] {
        let bit = 1 << lane.channel();
        let slot = &mut outputs[lane.slot()];
        if slot.channels & bit != 0 {
            return Err(DshotInitError::LaneChannelReused);
        }
        slot.channels |= bit;
        if lane.complementary() {
            slot.complementary |= bit;
        }
    }

    // One bit timing serves every lane, so every lane timer must count the
    // same clock.
    let mut timer_clock_hz = None;
    for (slot, slot_outputs) in outputs.iter().enumerate().take(TS::LEN) {
        if slot_outputs.channels == 0 {
            continue;
        }
        let hz = timers.timer(slot).timer_clock_hz(clocks);
        match timer_clock_hz {
            None => timer_clock_hz = Some(hz),
            Some(first) if first != hz => return Err(DshotInitError::LaneClocksDiffer),
            Some(_) => {}
        }
    }
    let timing = DshotTiming::from_clocks(
        timer_clock_hz.ok_or(DshotInitError::LaneTimerMissing)?,
        DSHOT600_BITRATE_HZ,
    )
    .map_err(DshotInitError::InvalidTiming)?;

    for (slot, slot_outputs) in outputs.iter().enumerate().take(TS::LEN) {
        if slot_outputs.channels != 0 {
            timers.timer(slot).configure(timing, *slot_outputs);
        }
    }
    let triggered_slot = start.wire(&timers)?;

    let lanes = Stm32f4DshotLanes {
        timers,
        outputs,
        triggered_slot,
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
