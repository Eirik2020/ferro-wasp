//! STM32H743 four-motor DShot600 transmitter.
//!
//! The command lease, telemetry, completion, and fault policy are the shared
//! `DshotBank` state machine from `ferrowasp_stm32_critical::dshot_bank`; this module
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

use core::marker::PhantomData;

use crate::dma_endpoints::{CompareChannel, StreamErrors, TimerCompareEndpoint};
use ferrowasp_output::dshot::DshotTiming;
use ferrowasp_stm32_critical::dshot_bank::channel::LaneChannel;
use ferrowasp_stm32_critical::dshot_lanes::{self, build_bank};
use stm32h7xx_hal::{
    dma::{
        DBTransfer, MemoryToPeripheral, Transfer,
        dma::DmaConfig,
        traits::{DoubleBufferedStream, Stream},
    },
    pac::{TIM2, TIM3, TIM4, TIM5},
    rcc::CoreClocks,
};

pub use ferrowasp_stm32_critical::dshot_bank::channel;
pub use ferrowasp_stm32_critical::dshot_bank::{
    COMPARE_DMA_SLOTS, DSHOT_COMMAND_MAX, DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS,
    DSHOT600_BITRATE_HZ, DshotBank, DshotCommandError, DshotCommandSequence, DshotInitError,
    DshotInterruptEvent, DshotLanes, DshotMotor, DshotMotors, DshotServiceEvent,
    DshotSpecialCommandError, DshotStats, DshotTelemetryRequestError, fallback_release_due,
};
pub use ferrowasp_stm32_critical::dshot_bank::{
    DshotReplies, DshotReply, DshotReplyError, DshotReplyStats,
};
pub use ferrowasp_stm32_critical::dshot_lanes::{
    DshotMode, DshotMotorBank, IntoLane, LaneDma, LaneOutputs, LaneTimer, SoftwareStart, TimerSet,
};

pub type DshotDmaBuffer = [u32; COMPARE_DMA_SLOTS];
type DshotBuffer = &'static mut DshotDmaBuffer;
/// Two DMA buffers per motor lane. It must be placed in memory DMA2 can
/// reach.
pub type DshotDmaStorage = dshot_lanes::DshotDmaStorage<DshotDmaBuffer>;

/// The register sequencing DShot needs from one timer, for the channels in
/// `channels` (bit n is channel n + 1). Implemented here for the PAC's
/// timers; the shared `LaneTimer` reaches it through `DshotTimer`.
trait RawLaneTimer {
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
            impl RawLaneTimer for $TIM {
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

/// A timer the bank drives. The shared lane traits are implemented for this
/// wrapper, since neither they nor the PAC's timer types belong to this
/// crate.
pub struct DshotTimer<T>(pub T);

/// The clock domain of TIM2-TIM5, which all count the APB1 timer clock.
const APB1_TIMERS: u8 = 1;

impl<T: RawLaneTimer + 'static> LaneTimer for DshotTimer<T> {
    fn clock_domain(&self) -> u8 {
        APB1_TIMERS
    }

    fn configure(&self, timing: DshotTiming, outputs: LaneOutputs) {
        self.0.configure(timing, outputs.channels);
    }

    fn stage(&self, channels: u8, first: &[u16; 4], period_end: u16) {
        self.0.stage(channels, first, period_end);
    }

    fn start(&self) {
        self.0.start();
    }

    fn stop(&self, channels: u8) {
        self.0.stop(channels);
    }

    fn force_inactive(&self, outputs: LaneOutputs) {
        self.0.force_inactive(outputs.channels);
    }
}

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

impl<S, T, C> LaneDma for Lane<S, T, C>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    T: CompareChannel<C>,
    C: LaneChannel,
{
    type Buffer = DshotDmaBuffer;

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

impl<TS, S, T, C> IntoLane<TS> for LaneStream<S, T, C>
where
    TS: TimerSet,
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    T: CompareChannel<C> + 'static,
    C: LaneChannel,
{
    type Lane = Lane<S, T, C>;

    fn into_lane(self, timers: &TS, buffer: DshotBuffer) -> Result<Self::Lane, DshotInitError> {
        let (slot, timer) = timers
            .find::<DshotTimer<T>>()
            .ok_or(DshotInitError::LaneTimerMissing)?;
        buffer.fill(0);
        let transfer = Transfer::init(
            self.stream,
            TimerCompareEndpoint::<T, C>::new(&timer.0),
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

/// Build a bank over `timers`, enabled and reset, with `lanes` for motors
/// 1-4, started in software. Each timer that carries a lane is configured
/// with its outputs parked low; the board connects the motor pads
/// afterwards.
#[allow(clippy::type_complexity)]
pub fn init_dshot_motor_bank<TS, LA, LB, LC, LD>(
    timers: TS,
    lanes: (LA, LB, LC, LD),
    clocks: &CoreClocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank<TS, LA::Lane, LB::Lane, LC::Lane, LD::Lane>, DshotInitError>
where
    TS: TimerSet,
    LA: IntoLane<TS, Lane: LaneDma<Buffer = DshotDmaBuffer>>,
    LB: IntoLane<TS, Lane: LaneDma<Buffer = DshotDmaBuffer>>,
    LC: IntoLane<TS, Lane: LaneDma<Buffer = DshotDmaBuffer>>,
    LD: IntoLane<TS, Lane: LaneDma<Buffer = DshotDmaBuffer>>,
{
    // H7 lanes do not receive replies yet (ADR-0045 leaves the Lucid's
    // receive path open), so an H7 bank is always unidirectional.
    build_bank(
        timers,
        lanes,
        SoftwareStart,
        dshot_lanes::DshotMode::Unidirectional,
        |_domain| {
            DshotTiming::from_clocks(clocks.timx_ker_ck().raw(), DSHOT600_BITRATE_HZ)
                .map_err(DshotInitError::InvalidTiming)
        },
        storage,
    )
}
