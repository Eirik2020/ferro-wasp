//! STM32F405 four-motor DShot600 transmitter.
//!
//! The backend owns TIM1, TIM8, four fixed DMA2 streams, every motor pin, and
//! all DMA buffers as one fault-containment unit. TIM1 is the frame master and
//! starts TIM8 through ITR0, so all four lanes begin from the same hardware
//! trigger. A frame set is complete only after every DMA lane completes.
//!
//! # Unsafe boundary
//!
//! This is the only project-local unsafe boundary used by the DShot backend.
//! It proves these fixed STM32F405 hardware facts to the HAL:
//!
//! - DMA peripheral addresses are the owned TIM1/TIM8 compare registers;
//! - compare-register DMA transfers are 16-bit halfwords;
//! - the four memory-to-timer DMA routes are exactly those documented below.
//!
//! Endpoint constructors are private. They can only be created while
//! `DshotMotorBank` takes exclusive ownership of both timers. DMA buffer
//! ownership, stream fencing, and buffer exchange remain delegated to
//! `stm32f4xx-hal::dma::Transfer`.

#![deny(unsafe_op_in_unsafe_fn)]

use ferrowasp_waveform::dshot::{
    COMPARE_DMA_SLOTS, DSHOT600_BITRATE_HZ, DshotTiming, DshotTimingError, command_lease_expired,
    encode_compare_sequence, four_motor_packets, throttles_to_dshot,
};
use stm32f4xx_hal::{
    dma::{
        ChannelX, DMAError, MemoryToPeripheral, Stream1, Stream2, Stream4, Stream6, Stream7,
        Transfer,
        config::{DmaConfig, Priority},
        traits::{Channel, DMASet, PeriAddress, StreamISR},
    },
    gpio::{Alternate, Input, PA8, PB15, PC8, PC9, Speed},
    pac::{DMA2, TIM1, TIM8, tim1::RegisterBlock},
    rcc::Clocks,
    timer::Timer,
};

pub const DSHOT_COMMAND_MAX: u16 = 2000;
pub const DSHOT_SERVICE_PERIOD_MS: u32 = 2;
pub const DSHOT_FRAME_TIMEOUT_MS: u32 = 1;
const ALL_MOTORS_COMPLETE: u8 = 0b1111;

pub type DshotDmaBuffer = [u16; COMPARE_DMA_SLOTS];
type DshotBuffer = &'static mut DshotDmaBuffer;
type Motor1Transfer = Transfer<Stream1<DMA2>, 6, Tim1Ch1Dma, MemoryToPeripheral, DshotBuffer>;
type Motor2Transfer = Transfer<Stream7<DMA2>, 7, Tim8Ch4Dma, MemoryToPeripheral, DshotBuffer>;
type Fcu3Motor3Transfer =
    Transfer<Stream4<DMA2>, 7, Tim8Ch3DmaFcu3, MemoryToPeripheral, DshotBuffer>;
type FoxeerMotor3Transfer =
    Transfer<Stream2<DMA2>, 0, Tim8Ch3DmaFoxeer, MemoryToPeripheral, DshotBuffer>;
type Motor4Transfer = Transfer<Stream6<DMA2>, 6, Tim1Ch3Dma, MemoryToPeripheral, DshotBuffer>;

/// Physical four-lane DShot bank output, before any airframe motor remap.
///
/// `Motor1` here means physical timer/DMA lane 1. It must not be interpreted as
/// Betaflight logical M1; the application performs that mapping separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotMotor {
    Motor1,
    Motor2,
    Motor3,
    Motor4,
}

impl DshotMotor {
    const fn index(self) -> usize {
        match self {
            Self::Motor1 => 0,
            Self::Motor2 => 1,
            Self::Motor3 => 2,
            Self::Motor4 => 3,
        }
    }

    const fn completion_bit(self) -> u8 {
        1 << self.index()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotInitError {
    InvalidTiming(DshotTimingError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotCommandError {
    ThrottleOutOfRange,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotTelemetryRequestError {
    Busy,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotServiceEvent {
    FrameStarted,
    Busy,
    LeaseExpired,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotInterruptEvent {
    Completed,
    Faulted,
    Spurious,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DshotStats {
    pub frames_started: u32,
    pub frames_completed: u32,
    pub lane_completions: [u32; 4],
    pub busy_skips: u32,
    pub lease_expiries: u32,
    pub frame_timeouts: u32,
    pub dma_faults: u32,
    pub spurious_interrupts: u32,
}

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

    fn split(&'static mut self) -> DshotBuffers {
        let [
            motor1_active,
            motor1_spare,
            motor2_active,
            motor2_spare,
            motor3_active,
            motor3_spare,
            motor4_active,
            motor4_spare,
        ] = self.buffers.each_mut();

        DshotBuffers {
            motor1_active,
            motor1_spare,
            motor2_active,
            motor2_spare,
            motor3_active,
            motor3_spare,
            motor4_active,
            motor4_spare,
        }
    }
}

impl Default for DshotDmaStorage {
    fn default() -> Self {
        Self::new()
    }
}

struct DshotBuffers {
    motor1_active: DshotBuffer,
    motor1_spare: DshotBuffer,
    motor2_active: DshotBuffer,
    motor2_spare: DshotBuffer,
    motor3_active: DshotBuffer,
    motor3_spare: DshotBuffer,
    motor4_active: DshotBuffer,
    motor4_spare: DshotBuffer,
}

macro_rules! dma_endpoint {
    (
        $(#[$meta:meta])*
        $name:ident,
        $stream:ty,
        $channel:literal
    ) => {
        $(#[$meta])*
        struct $name {
            address: u32,
        }

        impl $name {
            fn new(address: u32) -> Self {
                Self { address }
            }
        }

        // SAFETY: the endpoint is constructed only by `DshotMotorBank::new`
        // from a compare register belonging to its exclusively owned timer.
        // The STM32F405 timer compare register accepts 16-bit DMA writes.
        unsafe impl PeriAddress for $name {
            type MemSize = u16;

            #[inline]
            fn address(&self) -> u32 {
                self.address
            }
        }

        // SAFETY: RM0090 maps this fixed STM32F405 timer compare request to
        // the declared DMA2 stream/channel in the memory-to-peripheral
        // direction. The endpoint cannot be constructed outside this module.
        unsafe impl DMASet<$stream, $channel, MemoryToPeripheral> for $name {}
    };
}

dma_endpoint!(
    /// TIM1_CH1 compare endpoint for physical motor output 1.
    Tim1Ch1Dma,
    Stream1<DMA2>,
    6
);
dma_endpoint!(
    /// TIM8_CH4 compare endpoint for physical motor output 2.
    Tim8Ch4Dma,
    Stream7<DMA2>,
    7
);
dma_endpoint!(
    /// FCU3 TIM8_CH3 compare endpoint for physical motor output 3.
    Tim8Ch3DmaFcu3,
    Stream4<DMA2>,
    7
);
dma_endpoint!(
    /// Foxeer F405 V2 TIM8_CH3 compare endpoint for physical motor output 3.
    Tim8Ch3DmaFoxeer,
    Stream2<DMA2>,
    0
);
dma_endpoint!(
    /// TIM1_CH3 compare endpoint for physical TIM1_CH3N motor output 4.
    Tim1Ch3Dma,
    Stream6<DMA2>,
    6
);

/// Compile-time proof that the private endpoints implement the fixed FCU3
/// stream/channel routes consumed by the four-motor backend.
pub fn assert_four_motor_dma_routes_compile() {
    assert_dma_route::<Stream1<DMA2>, 6, Tim1Ch1Dma>();
    assert_dma_route::<Stream7<DMA2>, 7, Tim8Ch4Dma>();
    assert_dma_route::<Stream4<DMA2>, 7, Tim8Ch3DmaFcu3>();
    assert_dma_route::<Stream6<DMA2>, 6, Tim1Ch3Dma>();
}

/// Compile-time proof for the Foxeer F405 V2 motor DMA allocation.
pub fn assert_foxeer_four_motor_dma_routes_compile() {
    assert_dma_route::<Stream1<DMA2>, 6, Tim1Ch1Dma>();
    assert_dma_route::<Stream7<DMA2>, 7, Tim8Ch4Dma>();
    assert_dma_route::<Stream2<DMA2>, 0, Tim8Ch3DmaFoxeer>();
    assert_dma_route::<Stream6<DMA2>, 6, Tim1Ch3Dma>();
}

fn assert_dma_route<STREAM, const CHANNEL: u8, ENDPOINT>()
where
    STREAM: stm32f4xx_hal::dma::traits::Stream,
    ChannelX<CHANNEL>: Channel,
    ENDPOINT: PeriAddress<MemSize = u16> + DMASet<STREAM, CHANNEL, MemoryToPeripheral>,
{
}

struct DshotTimerBank {
    tim1: TIM1,
    tim8: TIM8,
    period_end: u16,
}

impl DshotTimerBank {
    fn new(tim1: TIM1, tim8: TIM8, timing: DshotTiming) -> Self {
        configure_timer_base(&tim1, timing);
        configure_timer_base(&tim8, timing);
        configure_tim1_channels(&tim1);
        configure_tim8_channels(&tim8);

        // TIM1 CEN is the master trigger. On its rising edge, TIM8 starts
        // through ITR0 in trigger mode. Both counters are staged at ARR before
        // the edge, so their first update loads the four first-bit duties.
        tim1.cr2()
            .modify(|_, w| w.ccds().on_compare().mms().enable());
        tim8.cr2().modify(|_, w| w.ccds().on_compare());
        tim8.smcr()
            .modify(|_, w| w.ts().itr0().sms().trigger_mode());

        // M4 uses TIM1_CH3N while TIM1_CH3 is disabled. RM0090 Table 96 defines
        // that case as OC3N = OC3REF xor CC3NP, so active-high polarity must
        // keep CC3NP clear for the same pulse shape as the ordinary outputs.
        tim1.ccer().modify(|_, w| {
            w.cc1p()
                .clear_bit()
                .cc1e()
                .set_bit()
                .cc3p()
                .clear_bit()
                .cc3e()
                .clear_bit()
                .cc3np()
                .active_high()
                .cc3ne()
                .set_bit()
        });
        tim8.ccer().modify(|_, w| {
            w.cc3p()
                .clear_bit()
                .cc3e()
                .set_bit()
                .cc4p()
                .clear_bit()
                .cc4e()
                .set_bit()
        });

        tim1.cr2()
            .modify(|_, w| w.ois1().clear_bit().ois3n().clear_bit());
        tim8.cr2()
            .modify(|_, w| w.ois3().clear_bit().ois4().clear_bit());
        configure_output_gate(&tim1);
        configure_output_gate(&tim8);
        force_update(&tim1);
        force_update(&tim8);
        clear_timer_flags(&tim1);
        clear_timer_flags(&tim8);

        Self {
            tim1,
            tim8,
            period_end: timing.arr,
        }
    }

    fn start_frame(&mut self, first_duties: [u16; 4]) {
        self.disable_dma_requests();
        self.tim1.cr1().modify(|_, w| w.cen().clear_bit());
        self.tim8.cr1().modify(|_, w| w.cen().clear_bit());

        write_compare(&self.tim1, 0, first_duties[0]);
        write_compare(&self.tim8, 3, first_duties[1]);
        write_compare(&self.tim8, 2, first_duties[2]);
        write_compare(&self.tim1, 2, first_duties[3]);
        write_counter(&self.tim1, self.period_end);
        write_counter(&self.tim8, self.period_end);
        clear_timer_flags(&self.tim1);
        clear_timer_flags(&self.tim8);

        self.tim1
            .dier()
            .modify(|_, w| w.cc1de().enabled().cc3de().enabled());
        self.tim8
            .dier()
            .modify(|_, w| w.cc3de().enabled().cc4de().enabled());

        // Only the master is started in software. TIM8 starts from TIM1_TRGO.
        self.tim1.cr1().modify(|_, w| w.cen().set_bit());
    }

    fn stop_frame(&mut self) {
        self.disable_dma_requests();
        self.tim1.cr1().modify(|_, w| w.cen().clear_bit());
        self.tim8.cr1().modify(|_, w| w.cen().clear_bit());
        write_all_compares_zero(&self.tim1);
        write_all_compares_zero(&self.tim8);
        force_update(&self.tim1);
        force_update(&self.tim8);
        clear_timer_flags(&self.tim1);
        clear_timer_flags(&self.tim8);
    }

    fn force_outputs_low(&mut self) {
        self.stop_frame();
        self.tim1.bdtr().modify(|_, w| w.moe().clear_bit());
        self.tim8.bdtr().modify(|_, w| w.moe().clear_bit());
        self.tim1
            .ccer()
            .modify(|_, w| w.cc1e().clear_bit().cc3ne().clear_bit());
        self.tim8
            .ccer()
            .modify(|_, w| w.cc3e().clear_bit().cc4e().clear_bit());
    }

    fn disable_dma_requests(&mut self) {
        self.tim1
            .dier()
            .modify(|_, w| w.cc1de().disabled().cc3de().disabled());
        self.tim8
            .dier()
            .modify(|_, w| w.cc3de().disabled().cc4de().disabled());
    }
}

enum Motor3DmaRoute {
    Fcu3(Stream4<DMA2>),
    Foxeer(Stream2<DMA2>),
}

enum Motor3Transfer {
    Fcu3(Fcu3Motor3Transfer),
    Foxeer(FoxeerMotor3Transfer),
}

impl Motor3Transfer {
    fn new(route: Motor3DmaRoute, address: u32, buffer: DshotBuffer) -> Self {
        match route {
            Motor3DmaRoute::Fcu3(stream) => {
                Self::Fcu3(init_transfer(stream, Tim8Ch3DmaFcu3::new(address), buffer))
            }
            Motor3DmaRoute::Foxeer(stream) => Self::Foxeer(init_transfer(
                stream,
                Tim8Ch3DmaFoxeer::new(address),
                buffer,
            )),
        }
    }

    fn exchange_buffer(
        &mut self,
        spare: &mut Option<DshotBuffer>,
        next: DshotBuffer,
    ) -> Result<(), ()> {
        match self {
            Self::Fcu3(transfer) => exchange_buffer(transfer, spare, next),
            Self::Foxeer(transfer) => exchange_buffer(transfer, spare, next),
        }
    }

    fn start(&mut self) {
        match self {
            Self::Fcu3(transfer) => transfer.start(|_| {}),
            Self::Foxeer(transfer) => transfer.start(|_| {}),
        }
    }

    fn is_transfer_complete(&self) -> bool {
        match self {
            Self::Fcu3(transfer) => transfer.is_transfer_complete(),
            Self::Foxeer(transfer) => transfer.is_transfer_complete(),
        }
    }

    fn is_error(&self) -> bool {
        match self {
            Self::Fcu3(transfer) => transfer.is_transfer_error() || transfer.is_direct_mode_error(),
            Self::Foxeer(transfer) => {
                transfer.is_transfer_error() || transfer.is_direct_mode_error()
            }
        }
    }

    fn pause_and_clear(&mut self) {
        match self {
            Self::Fcu3(transfer) => pause_and_clear(transfer),
            Self::Foxeer(transfer) => pause_and_clear(transfer),
        }
    }
}

pub struct DshotMotorBank {
    timers: DshotTimerBank,
    _motor1_pin: PA8<Alternate<1>>,
    _motor2_pin: PC9<Alternate<3>>,
    _motor3_pin: PC8<Alternate<3>>,
    _motor4_pin: PB15<Alternate<1>>,
    motor1_transfer: Motor1Transfer,
    motor2_transfer: Motor2Transfer,
    motor3_transfer: Motor3Transfer,
    motor4_transfer: Motor4Transfer,
    motor1_spare: Option<DshotBuffer>,
    motor2_spare: Option<DshotBuffer>,
    motor3_spare: Option<DshotBuffer>,
    motor4_spare: Option<DshotBuffer>,
    timing: DshotTiming,
    requested_values: [u16; 4],
    telemetry_request: Option<DshotMotor>,
    telemetry_request_sent: Option<DshotMotor>,
    lease_started_ms: u32,
    lease_duration_ms: Option<u32>,
    frame_started_ms: u32,
    completion_mask: u8,
    busy: bool,
    faulted: bool,
    stats: DshotStats,
}

impl DshotMotorBank {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        motor1_pin: PA8<Input>,
        motor2_pin: PC9<Input>,
        motor3_pin: PC8<Input>,
        motor4_pin: PB15<Input>,
        tim1: Timer<TIM1>,
        tim8: Timer<TIM8>,
        motor1_dma: Stream1<DMA2>,
        motor2_dma: Stream7<DMA2>,
        motor3_dma: Stream4<DMA2>,
        motor4_dma: Stream6<DMA2>,
        clocks: &Clocks,
        storage: &'static mut DshotDmaStorage,
    ) -> Result<Self, DshotInitError> {
        Self::new_with_motor3_route(
            motor1_pin,
            motor2_pin,
            motor3_pin,
            motor4_pin,
            tim1,
            tim8,
            motor1_dma,
            motor2_dma,
            Motor3DmaRoute::Fcu3(motor3_dma),
            motor4_dma,
            clocks,
            storage,
        )
    }

    /// Constructs the Foxeer F405 V2 bank with TIM8_CH3 on DMA2 Stream2
    /// Channel0. All other timer, pin, and containment behavior is shared with
    /// the flight-tested FCU3 implementation.
    #[allow(clippy::too_many_arguments)]
    pub fn new_foxeer(
        motor1_pin: PA8<Input>,
        motor2_pin: PC9<Input>,
        motor3_pin: PC8<Input>,
        motor4_pin: PB15<Input>,
        tim1: Timer<TIM1>,
        tim8: Timer<TIM8>,
        motor1_dma: Stream1<DMA2>,
        motor2_dma: Stream7<DMA2>,
        motor3_dma: Stream2<DMA2>,
        motor4_dma: Stream6<DMA2>,
        clocks: &Clocks,
        storage: &'static mut DshotDmaStorage,
    ) -> Result<Self, DshotInitError> {
        Self::new_with_motor3_route(
            motor1_pin,
            motor2_pin,
            motor3_pin,
            motor4_pin,
            tim1,
            tim8,
            motor1_dma,
            motor2_dma,
            Motor3DmaRoute::Foxeer(motor3_dma),
            motor4_dma,
            clocks,
            storage,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_motor3_route(
        motor1_pin: PA8<Input>,
        motor2_pin: PC9<Input>,
        motor3_pin: PC8<Input>,
        motor4_pin: PB15<Input>,
        tim1: Timer<TIM1>,
        tim8: Timer<TIM8>,
        motor1_dma: Stream1<DMA2>,
        motor2_dma: Stream7<DMA2>,
        motor3_dma: Motor3DmaRoute,
        motor4_dma: Stream6<DMA2>,
        clocks: &Clocks,
        storage: &'static mut DshotDmaStorage,
    ) -> Result<Self, DshotInitError> {
        let timing = DshotTiming::from_clocks(clocks.timclk2().raw(), DSHOT600_BITRATE_HZ)
            .map_err(DshotInitError::InvalidTiming)?;

        // Keep all external pads at a GPIO low latch until timer polarity,
        // off-state behavior, compare values, and output gates are configured.
        let mut motor1_pin = motor1_pin.into_push_pull_output();
        let mut motor2_pin = motor2_pin.into_push_pull_output();
        let mut motor3_pin = motor3_pin.into_push_pull_output();
        let mut motor4_pin = motor4_pin.into_push_pull_output();
        motor1_pin.set_low();
        motor2_pin.set_low();
        motor3_pin.set_low();
        motor4_pin.set_low();

        let tim1 = tim1.release();
        let tim8 = tim8.release();
        let motor1_endpoint = Tim1Ch1Dma::new(tim1.ccr(0).as_ptr() as u32);
        let motor2_endpoint = Tim8Ch4Dma::new(tim8.ccr(3).as_ptr() as u32);
        let motor4_endpoint = Tim1Ch3Dma::new(tim1.ccr(2).as_ptr() as u32);
        let motor3_endpoint_address = tim8.ccr(2).as_ptr() as u32;
        let timers = DshotTimerBank::new(tim1, tim8, timing);

        let motor1_pin = motor1_pin.into_alternate::<1>().speed(Speed::VeryHigh);
        let motor2_pin = motor2_pin.into_alternate::<3>().speed(Speed::VeryHigh);
        let motor3_pin = motor3_pin.into_alternate::<3>().speed(Speed::VeryHigh);
        let motor4_pin = motor4_pin.into_alternate::<1>().speed(Speed::VeryHigh);

        let buffers = storage.split();
        let motor1_transfer = init_transfer(motor1_dma, motor1_endpoint, buffers.motor1_active);
        let motor2_transfer = init_transfer(motor2_dma, motor2_endpoint, buffers.motor2_active);
        let motor3_transfer =
            Motor3Transfer::new(motor3_dma, motor3_endpoint_address, buffers.motor3_active);
        let motor4_transfer = init_transfer(motor4_dma, motor4_endpoint, buffers.motor4_active);

        Ok(Self {
            timers,
            _motor1_pin: motor1_pin,
            _motor2_pin: motor2_pin,
            _motor3_pin: motor3_pin,
            _motor4_pin: motor4_pin,
            motor1_transfer,
            motor2_transfer,
            motor3_transfer,
            motor4_transfer,
            motor1_spare: Some(buffers.motor1_spare),
            motor2_spare: Some(buffers.motor2_spare),
            motor3_spare: Some(buffers.motor3_spare),
            motor4_spare: Some(buffers.motor4_spare),
            timing,
            requested_values: [0; 4],
            telemetry_request: None,
            telemetry_request_sent: None,
            lease_started_ms: 0,
            lease_duration_ms: None,
            frame_started_ms: 0,
            completion_mask: 0,
            busy: false,
            faulted: false,
            stats: DshotStats::default(),
        })
    }

    pub fn command_stop(&mut self) {
        self.requested_values = [0; 4];
        self.lease_duration_ms = None;
    }

    pub fn command_throttles(
        &mut self,
        commands: [u16; 4],
        now_ms: u32,
        lease_duration_ms: u32,
    ) -> Result<(), DshotCommandError> {
        if self.faulted {
            return Err(DshotCommandError::Faulted);
        }
        if commands.iter().any(|command| *command > DSHOT_COMMAND_MAX) {
            return Err(DshotCommandError::ThrottleOutOfRange);
        }
        if commands.iter().all(|command| *command == 0) {
            self.command_stop();
            return Ok(());
        }

        self.requested_values = throttles_to_dshot(commands);
        self.lease_started_ms = now_ms;
        self.lease_duration_ms = Some(lease_duration_ms);
        Ok(())
    }

    /// Requests legacy UART telemetry from exactly one motor in the next
    /// successfully started DShot frame.
    pub fn request_telemetry(
        &mut self,
        motor: DshotMotor,
    ) -> Result<(), DshotTelemetryRequestError> {
        if self.faulted {
            return Err(DshotTelemetryRequestError::Faulted);
        }
        if self.telemetry_request.is_some() {
            return Err(DshotTelemetryRequestError::Busy);
        }
        self.telemetry_request = Some(motor);
        Ok(())
    }

    pub fn service(&mut self, now_ms: u32) -> DshotServiceEvent {
        self.telemetry_request_sent = None;
        if self.faulted {
            return DshotServiceEvent::Faulted;
        }

        let lease_expired = self.lease_duration_ms.is_some_and(|duration_ms| {
            command_lease_expired(self.lease_started_ms, duration_ms, now_ms)
        });
        if lease_expired {
            self.command_stop();
            self.stats.lease_expiries = self.stats.lease_expiries.wrapping_add(1);
        }

        if self.busy {
            self.stats.busy_skips = self.stats.busy_skips.wrapping_add(1);
            if command_lease_expired(self.frame_started_ms, DSHOT_FRAME_TIMEOUT_MS, now_ms) {
                self.stats.frame_timeouts = self.stats.frame_timeouts.wrapping_add(1);
                self.latch_fault(false);
                return DshotServiceEvent::Faulted;
            }

            return if lease_expired {
                DshotServiceEvent::LeaseExpired
            } else {
                DshotServiceEvent::Busy
            };
        }

        if self.send_requested(now_ms).is_err() {
            return DshotServiceEvent::Faulted;
        }

        if lease_expired {
            DshotServiceEvent::LeaseExpired
        } else {
            DshotServiceEvent::FrameStarted
        }
    }

    pub fn on_dma_interrupt(&mut self, motor: DshotMotor) -> DshotInterruptEvent {
        if self.faulted {
            self.pause_and_clear_motor(motor);
            return DshotInterruptEvent::Faulted;
        }

        let (transfer_complete, dma_error) = self.motor_dma_status(motor);
        self.pause_and_clear_motor(motor);

        let completion_bit = motor.completion_bit();
        if !self.busy || self.completion_mask & completion_bit != 0 {
            self.latch_fault(true);
            return DshotInterruptEvent::Spurious;
        }
        if dma_error {
            self.latch_fault(false);
            return DshotInterruptEvent::Faulted;
        }
        if !transfer_complete {
            self.latch_fault(true);
            return DshotInterruptEvent::Spurious;
        }

        self.completion_mask |= completion_bit;
        let lane = &mut self.stats.lane_completions[motor.index()];
        *lane = lane.wrapping_add(1);

        if self.completion_mask == ALL_MOTORS_COMPLETE {
            self.timers.stop_frame();
            self.busy = false;
            self.stats.frames_completed = self.stats.frames_completed.wrapping_add(1);
        }

        DshotInterruptEvent::Completed
    }

    pub const fn is_faulted(&self) -> bool {
        self.faulted
    }

    pub const fn stats(&self) -> DshotStats {
        self.stats
    }

    pub const fn requested_values(&self) -> [u16; 4] {
        self.requested_values
    }

    /// Takes the lane whose telemetry bit was included in the frame started by
    /// the most recent `service` call.
    pub fn take_telemetry_request_sent(&mut self) -> Option<DshotMotor> {
        self.telemetry_request_sent.take()
    }

    fn send_requested(&mut self, now_ms: u32) -> Result<(), DshotCommandError> {
        if self.any_spare_missing() {
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        }

        let Some(motor1_next) = self.motor1_spare.take() else {
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        };
        let Some(motor2_next) = self.motor2_spare.take() else {
            self.motor1_spare = Some(motor1_next);
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        };
        let Some(motor3_next) = self.motor3_spare.take() else {
            self.motor1_spare = Some(motor1_next);
            self.motor2_spare = Some(motor2_next);
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        };
        let Some(motor4_next) = self.motor4_spare.take() else {
            self.motor1_spare = Some(motor1_next);
            self.motor2_spare = Some(motor2_next);
            self.motor3_spare = Some(motor3_next);
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        };

        let telemetry_request = self.telemetry_request;
        let packets = four_motor_packets(
            self.requested_values,
            telemetry_request.map(DshotMotor::index),
        );
        let mut first_duties = [0; 4];
        for (index, (packet, buffer)) in packets
            .into_iter()
            .zip([
                &mut *motor1_next,
                &mut *motor2_next,
                &mut *motor3_next,
                &mut *motor4_next,
            ])
            .enumerate()
        {
            first_duties[index] = encode_compare_sequence(packet, self.timing, buffer);
        }

        if exchange_buffer(
            &mut self.motor1_transfer,
            &mut self.motor1_spare,
            motor1_next,
        )
        .is_err()
        {
            self.motor2_spare = Some(motor2_next);
            self.motor3_spare = Some(motor3_next);
            self.motor4_spare = Some(motor4_next);
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        }
        if exchange_buffer(
            &mut self.motor2_transfer,
            &mut self.motor2_spare,
            motor2_next,
        )
        .is_err()
        {
            self.motor3_spare = Some(motor3_next);
            self.motor4_spare = Some(motor4_next);
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        }
        if self
            .motor3_transfer
            .exchange_buffer(&mut self.motor3_spare, motor3_next)
            .is_err()
        {
            self.motor4_spare = Some(motor4_next);
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        }
        if exchange_buffer(
            &mut self.motor4_transfer,
            &mut self.motor4_spare,
            motor4_next,
        )
        .is_err()
        {
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        }

        self.frame_started_ms = now_ms;
        self.completion_mask = 0;
        self.busy = true;
        self.stats.frames_started = self.stats.frames_started.wrapping_add(1);

        // Enable all four DMA streams before opening any timer DMA request.
        self.motor1_transfer.start(|_| {});
        self.motor2_transfer.start(|_| {});
        self.motor3_transfer.start();
        self.motor4_transfer.start(|_| {});
        self.timers.start_frame(first_duties);
        self.telemetry_request_sent = telemetry_request;
        self.telemetry_request = None;
        Ok(())
    }

    fn any_spare_missing(&self) -> bool {
        self.motor1_spare.is_none()
            || self.motor2_spare.is_none()
            || self.motor3_spare.is_none()
            || self.motor4_spare.is_none()
    }

    fn motor_dma_status(&self, motor: DshotMotor) -> (bool, bool) {
        match motor {
            DshotMotor::Motor1 => (
                self.motor1_transfer.is_transfer_complete(),
                self.motor1_transfer.is_transfer_error()
                    || self.motor1_transfer.is_direct_mode_error(),
            ),
            DshotMotor::Motor2 => (
                self.motor2_transfer.is_transfer_complete(),
                self.motor2_transfer.is_transfer_error()
                    || self.motor2_transfer.is_direct_mode_error(),
            ),
            DshotMotor::Motor3 => (
                self.motor3_transfer.is_transfer_complete(),
                self.motor3_transfer.is_error(),
            ),
            DshotMotor::Motor4 => (
                self.motor4_transfer.is_transfer_complete(),
                self.motor4_transfer.is_transfer_error()
                    || self.motor4_transfer.is_direct_mode_error(),
            ),
        }
    }

    fn pause_and_clear_motor(&mut self, motor: DshotMotor) {
        match motor {
            DshotMotor::Motor1 => pause_and_clear(&mut self.motor1_transfer),
            DshotMotor::Motor2 => pause_and_clear(&mut self.motor2_transfer),
            DshotMotor::Motor3 => self.motor3_transfer.pause_and_clear(),
            DshotMotor::Motor4 => pause_and_clear(&mut self.motor4_transfer),
        }
    }

    fn latch_fault(&mut self, spurious: bool) {
        if !self.faulted {
            self.stats.dma_faults = self.stats.dma_faults.wrapping_add(1);
        }
        if spurious {
            self.stats.spurious_interrupts = self.stats.spurious_interrupts.wrapping_add(1);
        }

        self.faulted = true;
        self.busy = false;
        self.telemetry_request = None;
        self.telemetry_request_sent = None;
        self.command_stop();
        self.timers.force_outputs_low();
        pause_and_clear(&mut self.motor1_transfer);
        pause_and_clear(&mut self.motor2_transfer);
        self.motor3_transfer.pause_and_clear();
        pause_and_clear(&mut self.motor4_transfer);
    }
}

fn configure_timer_base(timer: &RegisterBlock, timing: DshotTiming) {
    timer.cr1().reset();
    timer.cr2().reset();
    timer.smcr().reset();
    timer.dier().reset();
    timer.sr().reset();
    timer.ccer().reset();
    timer.bdtr().reset();
    timer.psc().write(|w| w.psc().set(0));
    timer.rcr().write(|w| w.rep().set(0));
    timer.arr().write(|w| w.arr().set(timing.arr));
    write_all_compares_zero(timer);
    write_counter(timer, 0);
    timer.cr1().modify(|_, w| w.arpe().set_bit());
}

fn configure_tim1_channels(timer: &RegisterBlock) {
    timer
        .ccmr1_output()
        .write(|w| w.cc1s().output().oc1pe().enabled().oc1m().pwm_mode1());
    timer
        .ccmr2_output()
        .write(|w| w.cc3s().output().oc3pe().enabled().oc3m().pwm_mode1());
}

fn configure_tim8_channels(timer: &RegisterBlock) {
    timer.ccmr2_output().write(|w| {
        w.cc3s()
            .output()
            .oc3pe()
            .enabled()
            .oc3m()
            .pwm_mode1()
            .cc4s()
            .output()
            .oc4pe()
            .enabled()
            .oc4m()
            .pwm_mode1()
    });
}

fn configure_output_gate(timer: &RegisterBlock) {
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
}

fn write_compare(timer: &RegisterBlock, channel_index: usize, value: u16) {
    timer.ccr(channel_index).write(|w| w.ccr().set(value));
}

fn write_all_compares_zero(timer: &RegisterBlock) {
    for channel_index in 0..4 {
        write_compare(timer, channel_index, 0);
    }
}

fn write_counter(timer: &RegisterBlock, value: u16) {
    timer.cnt().write(|w| w.cnt().set(value));
}

fn force_update(timer: &RegisterBlock) {
    timer.egr().write(|w| w.ug().update());
}

fn clear_timer_flags(timer: &RegisterBlock) {
    timer.sr().reset();
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

fn init_transfer<STREAM, const CHANNEL: u8, ENDPOINT>(
    stream: STREAM,
    endpoint: ENDPOINT,
    buffer: DshotBuffer,
) -> Transfer<STREAM, CHANNEL, ENDPOINT, MemoryToPeripheral, DshotBuffer>
where
    STREAM: stm32f4xx_hal::dma::traits::Stream,
    ChannelX<CHANNEL>: Channel,
    ENDPOINT: PeriAddress<MemSize = u16> + DMASet<STREAM, CHANNEL, MemoryToPeripheral>,
{
    buffer.fill(0);
    Transfer::init_memory_to_peripheral(stream, endpoint, buffer, None, dma_config())
}

fn exchange_buffer<STREAM, const CHANNEL: u8, ENDPOINT>(
    transfer: &mut Transfer<STREAM, CHANNEL, ENDPOINT, MemoryToPeripheral, DshotBuffer>,
    spare: &mut Option<DshotBuffer>,
    next: DshotBuffer,
) -> Result<(), ()>
where
    STREAM: stm32f4xx_hal::dma::traits::Stream,
    ChannelX<CHANNEL>: Channel,
    ENDPOINT: PeriAddress<MemSize = u16> + DMASet<STREAM, CHANNEL, MemoryToPeripheral>,
{
    match transfer.next_transfer(next) {
        Ok((old, _)) => {
            *spare = Some(old);
            Ok(())
        }
        Err(error) => {
            *spare = Some(dma_error_buffer(error));
            Err(())
        }
    }
}

fn dma_error_buffer(error: DMAError<DshotBuffer>) -> DshotBuffer {
    match error {
        DMAError::NotReady(buffer) | DMAError::SmallBuffer(buffer) | DMAError::Overrun(buffer) => {
            buffer
        }
    }
}

fn pause_and_clear<STREAM, const CHANNEL: u8, ENDPOINT>(
    transfer: &mut Transfer<STREAM, CHANNEL, ENDPOINT, MemoryToPeripheral, DshotBuffer>,
) where
    STREAM: stm32f4xx_hal::dma::traits::Stream,
    ChannelX<CHANNEL>: Channel,
    ENDPOINT: PeriAddress<MemSize = u16> + DMASet<STREAM, CHANNEL, MemoryToPeripheral>,
{
    transfer.pause(|_| {});
    transfer.clear_transfer_complete();
    transfer.clear_half_transfer();
    transfer.clear_transfer_error();
    transfer.clear_direct_mode_error();
    transfer.clear_fifo_error();
}
