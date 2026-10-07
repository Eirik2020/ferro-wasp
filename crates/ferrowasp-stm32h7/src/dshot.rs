//! STM32H743 four-motor DShot600 transmitter.
//!
//! The command lease, telemetry, completion, and fault policy are the shared
//! `DshotBank` state machine from `ferrowasp_stm32f4::dshot_bank`; this module
//! is the H7 hardware under it. The bank owns TIM3 and TIM5, DMA2 streams 0-3,
//! every motor pin, and all DMA buffers as one fault-containment unit.
//!
//! Lanes: motor 1 TIM3_CH3, motor 2 TIM3_CH4, motor 3 TIM5_CH1, motor 4
//! TIM5_CH2, on DMA2 streams 0 to 3 in that order.
//!
//! Unlike the F405 bank, whose TIM1 starts TIM8 through a trigger input, the
//! two timers here are started back to back in software. The skew is a few
//! bus cycles against a 1.67 us bit.
//!
//! Every lane writes 32-bit compare words: TIM5's compare registers are 32
//! bits wide, and TIM3 ignores the upper half.

use crate::dma_endpoints::{
    StreamErrors, TimerCompareEndpoint, tim3_ch3, tim3_ch4, tim5_ch1, tim5_ch2,
};
use ferrowasp_waveform::dshot::{DshotPacket, DshotTiming, encode_compare_sequence};
use stm32h7xx_hal::{
    dma::{
        DBTransfer, MemoryToPeripheral, Transfer,
        dma::{DMAReq, DmaConfig, Stream0, Stream1, Stream2, Stream3},
        traits::{DoubleBufferedStream, Stream},
    },
    gpio::{Alternate, PA0, PA1, PB0, PB1, Speed},
    pac::{DMA2, TIM3, TIM5},
    rcc::{CoreClocks, ResetEnable, rec},
};

pub use ferrowasp_stm32f4::dshot_bank::{
    COMPARE_DMA_SLOTS, DSHOT_COMMAND_MAX, DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS,
    DSHOT600_BITRATE_HZ, DshotBank, DshotCommandError, DshotInitError, DshotInterruptEvent,
    DshotLanes, DshotMotor, DshotServiceEvent, DshotStats, DshotTelemetryRequestError,
};

pub type DshotDmaBuffer = [u32; COMPARE_DMA_SLOTS];
type DshotBuffer = &'static mut DshotDmaBuffer;
type LaneTransfer<S, const REQUEST: u8> =
    Transfer<S, TimerCompareEndpoint<REQUEST>, MemoryToPeripheral, DshotBuffer, DBTransfer>;
type Motor1Transfer = LaneTransfer<Stream0<DMA2>, { DMAReq::Tim3Ch3 as u8 }>;
type Motor2Transfer = LaneTransfer<Stream1<DMA2>, { DMAReq::Tim3Ch4 as u8 }>;
type Motor3Transfer = LaneTransfer<Stream2<DMA2>, { DMAReq::Tim5Ch1 as u8 }>;
type Motor4Transfer = LaneTransfer<Stream3<DMA2>, { DMAReq::Tim5Ch2 as u8 }>;

/// The STM32H743 four-motor bank: the shared DShot state machine over TIM3,
/// TIM5, and DMA2 streams 0-3.
pub type DshotMotorBank = DshotBank<H7DshotLanes>;

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

pub struct DshotMotorBankResources {
    pub tim3: TIM3,
    pub tim3_rec: rec::Tim3,
    pub tim5: TIM5,
    pub tim5_rec: rec::Tim5,
    pub motor1_pin: PB0,
    pub motor2_pin: PB1,
    pub motor3_pin: PA0,
    pub motor4_pin: PA1,
    pub motor1_dma: Stream0<DMA2>,
    pub motor2_dma: Stream1<DMA2>,
    pub motor3_dma: Stream2<DMA2>,
    pub motor4_dma: Stream3<DMA2>,
}

/// TIM3/TIM5 compare lanes and their DMA2 streams, the H7 hardware under a
/// `DshotMotorBank`.
pub struct H7DshotLanes {
    tim3: TIM3,
    tim5: TIM5,
    period_end: u16,
    _motor1_pin: PB0<Alternate<2>>,
    _motor2_pin: PB1<Alternate<2>>,
    _motor3_pin: PA0<Alternate<2>>,
    _motor4_pin: PA1<Alternate<2>>,
    motor1_transfer: Motor1Transfer,
    motor2_transfer: Motor2Transfer,
    motor3_transfer: Motor3Transfer,
    motor4_transfer: Motor4Transfer,
}

/// The register sequencing one timer's pair of DShot channels needs. TIM3
/// and TIM5 have different register blocks, so each gets its own impl.
macro_rules! dshot_timer {
    (
        $name:ident: $TIM:ident, $ccmr:ident,
        a: [$ia:literal, $ccas:ident, $ocape:ident, $ocam:ident, $ccap:ident, $ccae:ident, $ccade:ident],
        b: [$ib:literal, $ccbs:ident, $ocbpe:ident, $ocbm:ident, $ccbp:ident, $ccbe:ident, $ccbde:ident]
    ) => {
        mod $name {
            use super::*;

            pub(super) fn configure(timer: &$TIM, timing: DshotTiming) {
                timer.cr1.reset();
                timer.cr2.reset();
                timer.smcr.reset();
                timer.dier.reset();
                timer.ccer.reset();
                timer.psc.write(|w| w.psc().bits(0));
                timer.arr.write(|w| w.arr().bits(timing.arr.into()));
                park(timer);
                timer.cnt.write(|w| w.cnt().bits(0));
                timer.cr1.modify(|_, w| w.arpe().set_bit());
                timer.$ccmr().write(|w| {
                    w.$ccas()
                        .output()
                        .$ocape()
                        .enabled()
                        .$ocam()
                        .pwm_mode1()
                        .$ccbs()
                        .output()
                        .$ocbpe()
                        .enabled()
                        .$ocbm()
                        .pwm_mode1()
                });
                timer.cr2.modify(|_, w| w.ccds().on_compare());
                timer.ccer.modify(|_, w| {
                    w.$ccap()
                        .clear_bit()
                        .$ccae()
                        .set_bit()
                        .$ccbp()
                        .clear_bit()
                        .$ccbe()
                        .set_bit()
                });
                timer.egr.write(|w| w.ug().set_bit());
                timer.sr.reset();
            }

            /// Stop the counter with both requests closed, load the first
            /// duties, and stage the counter at ARR so its first update
            /// latches them.
            pub(super) fn stage(timer: &$TIM, first: [u16; 2], period_end: u16) {
                close_requests(timer);
                timer.cr1.modify(|_, w| w.cen().clear_bit());
                timer.ccr[$ia].write(|w| w.ccr().bits(first[0].into()));
                timer.ccr[$ib].write(|w| w.ccr().bits(first[1].into()));
                timer.cnt.write(|w| w.cnt().bits(period_end.into()));
                timer.sr.reset();
                timer
                    .dier
                    .modify(|_, w| w.$ccade().set_bit().$ccbde().set_bit());
            }

            pub(super) fn start(timer: &$TIM) {
                timer.cr1.modify(|_, w| w.cen().set_bit());
            }

            pub(super) fn close_requests(timer: &$TIM) {
                timer
                    .dier
                    .modify(|_, w| w.$ccade().clear_bit().$ccbde().clear_bit());
            }

            /// Stop with both outputs held low: zero compare in PWM mode 1.
            pub(super) fn stop(timer: &$TIM) {
                close_requests(timer);
                timer.cr1.modify(|_, w| w.cen().clear_bit());
                park(timer);
                timer.egr.write(|w| w.ug().set_bit());
                timer.sr.reset();
            }

            /// Hold both outputs inactive regardless of compare values, until
            /// the bank is rebuilt.
            pub(super) fn force_inactive(timer: &$TIM) {
                stop(timer);
                timer
                    .$ccmr()
                    .modify(|_, w| w.$ocam().force_inactive().$ocbm().force_inactive());
            }

            fn park(timer: &$TIM) {
                for ccr in &timer.ccr {
                    ccr.write(|w| w.ccr().bits(0));
                }
            }
        }
    };
}

dshot_timer!(tim3_lanes: TIM3, ccmr2_output,
    a: [2, cc3s, oc3pe, oc3m, cc3p, cc3e, cc3de],
    b: [3, cc4s, oc4pe, oc4m, cc4p, cc4e, cc4de]);
dshot_timer!(tim5_lanes: TIM5, ccmr1_output,
    a: [0, cc1s, oc1pe, oc1m, cc1p, cc1e, cc1de],
    b: [1, cc2s, oc2pe, oc2m, cc2p, cc2e, cc2de]);

fn dma_config() -> DmaConfig {
    DmaConfig::default()
        .priority(stm32h7xx_hal::dma::config::Priority::VeryHigh)
        .memory_increment(true)
        .peripheral_increment(false)
        .transfer_complete_interrupt(true)
        .transfer_error_interrupt(true)
        .direct_mode_error_interrupt(true)
}

fn exchange<S, const REQUEST: u8>(
    transfer: &mut LaneTransfer<S, REQUEST>,
    next: DshotBuffer,
) -> Result<DshotBuffer, DshotBuffer>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig>,
{
    // Single-buffer mode: the HAL stops the stream, swaps the buffer, and
    // re-enables it; no request reaches it until the timer opens one. That
    // cannot fail; the error arm exists because double-buffer mode can.
    let mut next = Some(next);
    let old = transfer.next_transfer_with(|old, _, _| match next.take() {
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

fn status<S, const REQUEST: u8>(transfer: &LaneTransfer<S, REQUEST>) -> (bool, bool)
where
    S: Stream + StreamErrors,
{
    let errors = S::error_flags();
    (
        transfer.get_transfer_complete_flag(),
        errors.transfer_error || errors.direct_mode_error,
    )
}

fn pause_and_clear<S, const REQUEST: u8>(transfer: &mut LaneTransfer<S, REQUEST>)
where
    S: Stream<Config = DmaConfig>,
{
    transfer.pause(|_| {});
    transfer.clear_interrupts();
}

impl DshotLanes for H7DshotLanes {
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
        match motor {
            DshotMotor::Motor1 => exchange(&mut self.motor1_transfer, next),
            DshotMotor::Motor2 => exchange(&mut self.motor2_transfer, next),
            DshotMotor::Motor3 => exchange(&mut self.motor3_transfer, next),
            DshotMotor::Motor4 => exchange(&mut self.motor4_transfer, next),
        }
    }

    fn start_dma(&mut self, motor: DshotMotor) {
        match motor {
            DshotMotor::Motor1 => self.motor1_transfer.start(|_| {}),
            DshotMotor::Motor2 => self.motor2_transfer.start(|_| {}),
            DshotMotor::Motor3 => self.motor3_transfer.start(|_| {}),
            DshotMotor::Motor4 => self.motor4_transfer.start(|_| {}),
        }
    }

    fn start_frame(&mut self, first_duties: [u16; 4]) {
        tim3_lanes::stage(
            &self.tim3,
            [first_duties[0], first_duties[1]],
            self.period_end,
        );
        tim5_lanes::stage(
            &self.tim5,
            [first_duties[2], first_duties[3]],
            self.period_end,
        );
        tim3_lanes::start(&self.tim3);
        tim5_lanes::start(&self.tim5);
    }

    fn stop_frame(&mut self) {
        tim3_lanes::stop(&self.tim3);
        tim5_lanes::stop(&self.tim5);
    }

    fn force_outputs_low(&mut self) {
        tim3_lanes::force_inactive(&self.tim3);
        tim5_lanes::force_inactive(&self.tim5);
    }

    fn dma_status(&self, motor: DshotMotor) -> (bool, bool) {
        match motor {
            DshotMotor::Motor1 => status(&self.motor1_transfer),
            DshotMotor::Motor2 => status(&self.motor2_transfer),
            DshotMotor::Motor3 => status(&self.motor3_transfer),
            DshotMotor::Motor4 => status(&self.motor4_transfer),
        }
    }

    fn pause_and_clear(&mut self, motor: DshotMotor) {
        match motor {
            DshotMotor::Motor1 => pause_and_clear(&mut self.motor1_transfer),
            DshotMotor::Motor2 => pause_and_clear(&mut self.motor2_transfer),
            DshotMotor::Motor3 => pause_and_clear(&mut self.motor3_transfer),
            DshotMotor::Motor4 => pause_and_clear(&mut self.motor4_transfer),
        }
    }
}

/// Build the Lucid H7 bank. Pads are latched low as GPIO until both timers
/// are configured with their outputs parked low.
pub fn init_dshot_motor_bank(
    resources: DshotMotorBankResources,
    clocks: &CoreClocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank, DshotInitError> {
    let DshotMotorBankResources {
        tim3,
        tim3_rec,
        tim5,
        tim5_rec,
        motor1_pin,
        motor2_pin,
        motor3_pin,
        motor4_pin,
        motor1_dma,
        motor2_dma,
        motor3_dma,
        motor4_dma,
    } = resources;

    let timing = DshotTiming::from_clocks(clocks.timx_ker_ck().raw(), DSHOT600_BITRATE_HZ)
        .map_err(DshotInitError::InvalidTiming)?;

    let mut motor1_pin = motor1_pin.into_push_pull_output();
    let mut motor2_pin = motor2_pin.into_push_pull_output();
    let mut motor3_pin = motor3_pin.into_push_pull_output();
    let mut motor4_pin = motor4_pin.into_push_pull_output();
    motor1_pin.set_low();
    motor2_pin.set_low();
    motor3_pin.set_low();
    motor4_pin.set_low();

    tim3_rec.enable().reset();
    tim5_rec.enable().reset();
    tim3_lanes::configure(&tim3, timing);
    tim5_lanes::configure(&tim5, timing);

    let motor1_pin = motor1_pin.into_alternate::<2>().speed(Speed::VeryHigh);
    let motor2_pin = motor2_pin.into_alternate::<2>().speed(Speed::VeryHigh);
    let motor3_pin = motor3_pin.into_alternate::<2>().speed(Speed::VeryHigh);
    let motor4_pin = motor4_pin.into_alternate::<2>().speed(Speed::VeryHigh);

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
    let init = |buffer: DshotBuffer| {
        buffer.fill(0);
        buffer
    };
    let motor1_transfer = Transfer::init(
        motor1_dma,
        tim3_ch3(&tim3),
        init(motor1_active),
        None,
        dma_config(),
    );
    let motor2_transfer = Transfer::init(
        motor2_dma,
        tim3_ch4(&tim3),
        init(motor2_active),
        None,
        dma_config(),
    );
    let motor3_transfer = Transfer::init(
        motor3_dma,
        tim5_ch1(&tim5),
        init(motor3_active),
        None,
        dma_config(),
    );
    let motor4_transfer = Transfer::init(
        motor4_dma,
        tim5_ch2(&tim5),
        init(motor4_active),
        None,
        dma_config(),
    );

    let lanes = H7DshotLanes {
        tim3,
        tim5,
        period_end: timing.arr,
        _motor1_pin: motor1_pin,
        _motor2_pin: motor2_pin,
        _motor3_pin: motor3_pin,
        _motor4_pin: motor4_pin,
        motor1_transfer,
        motor2_transfer,
        motor3_transfer,
        motor4_transfer,
    };
    Ok(DshotBank::from_lanes(
        lanes,
        [motor1_spare, motor2_spare, motor3_spare, motor4_spare],
        timing,
    ))
}
