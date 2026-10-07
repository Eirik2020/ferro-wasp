//! The four-motor DShot bank's safety state machine, independent of the
//! timers and DMA streams that put it on the wire.
//!
//! A bank owns every motor lane as one fault-containment unit: a command
//! lease that stops the motors when it is not renewed, one-shot telemetry
//! requests, frame completion that needs every lane's DMA, and a latched
//! fault that forces all outputs low. Each STM32 family's backend implements
//! `DshotLanes` over its own timers and DMA; the policy here is shared, so a
//! board on either family runs the flight-tested sequencing.

use ferrowasp_waveform::dshot::{
    DshotPacket, DshotTiming, command_lease_expired, four_motor_packets, throttles_to_dshot,
};

pub use ferrowasp_waveform::dshot::{COMPARE_DMA_SLOTS, DSHOT600_BITRATE_HZ, DshotTimingError};

pub const DSHOT_COMMAND_MAX: u16 = 2000;
pub const DSHOT_SERVICE_PERIOD_MS: u32 = 2;
pub const DSHOT_FRAME_TIMEOUT_MS: u32 = 1;
const ALL_MOTORS_COMPLETE: u8 = 0b1111;

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
    pub const ALL: [Self; 4] = [Self::Motor1, Self::Motor2, Self::Motor3, Self::Motor4];

    pub const fn index(self) -> usize {
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

/// The hardware under a DShot bank: four timer compare lanes, each fed by
/// its own memory-to-timer DMA stream, and the timers that clock them.
pub trait DshotLanes {
    /// One lane's DMA buffer of compare values, in the word size its timer
    /// compare register takes.
    type Buffer: 'static;

    /// Encode `packet` into `buffer`, returning the first bit's compare duty
    /// for `start_frame` to load directly.
    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut Self::Buffer) -> u16;

    /// Swap `next` in as `motor`'s DMA buffer. `Ok` returns the buffer the
    /// lane gave up; `Err` returns a buffer the lane could not take.
    fn exchange_buffer(
        &mut self,
        motor: DshotMotor,
        next: &'static mut Self::Buffer,
    ) -> Result<&'static mut Self::Buffer, &'static mut Self::Buffer>;

    /// Enable `motor`'s DMA stream. No timer request is open yet.
    fn start_dma(&mut self, motor: DshotMotor);

    /// Load the first duties, open the timer DMA requests, and start the
    /// timers so all four lanes begin together.
    fn start_frame(&mut self, first_duties: [u16; 4]);

    /// Close the DMA requests, stop the timers, and park every output low.
    fn stop_frame(&mut self);

    /// Stop, then gate every output off until the bank is rebuilt.
    fn force_outputs_low(&mut self);

    /// `motor`'s DMA status: (transfer complete, transfer or direct-mode
    /// error).
    fn dma_status(&self, motor: DshotMotor) -> (bool, bool);

    /// Disable `motor`'s DMA stream and clear its flags.
    fn pause_and_clear(&mut self, motor: DshotMotor);
}

pub struct DshotBank<L>
where
    L: DshotLanes,
{
    lanes: L,
    spares: [Option<&'static mut L::Buffer>; 4],
    timing: DshotTiming,
    requested_values: [u16; 4],
    telemetry_request: Option<DshotMotor>,
    telemetry_request_sent: Option<DshotMotor>,
    lease_started_ms: u64,
    lease_duration_ms: Option<u32>,
    frame_started_ms: u64,
    completion_mask: u8,
    busy: bool,
    faulted: bool,
    stats: DshotStats,
}

impl<L> DshotBank<L>
where
    L: DshotLanes,
{
    /// A bank over lanes already holding their first buffers, with one spare
    /// per lane.
    pub fn from_lanes(lanes: L, spares: [&'static mut L::Buffer; 4], timing: DshotTiming) -> Self {
        let [motor1_spare, motor2_spare, motor3_spare, motor4_spare] = spares;
        Self {
            lanes,
            spares: [
                Some(motor1_spare),
                Some(motor2_spare),
                Some(motor3_spare),
                Some(motor4_spare),
            ],
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
        }
    }

    pub fn command_stop(&mut self) {
        self.requested_values = [0; 4];
        self.lease_duration_ms = None;
    }

    pub fn command_throttles(
        &mut self,
        commands: [u16; 4],
        now_ms: u64,
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

    pub fn service(&mut self, now_ms: u64) -> DshotServiceEvent {
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
            self.lanes.pause_and_clear(motor);
            return DshotInterruptEvent::Faulted;
        }

        let (transfer_complete, dma_error) = self.lanes.dma_status(motor);
        self.lanes.pause_and_clear(motor);

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
            self.lanes.stop_frame();
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

    fn send_requested(&mut self, now_ms: u64) -> Result<(), DshotCommandError> {
        if self.spares.iter().any(Option::is_none) {
            self.latch_fault(false);
            return Err(DshotCommandError::Faulted);
        }

        // Every spare is present, so each take below succeeds.
        let mut next = [
            self.spares[0].take(),
            self.spares[1].take(),
            self.spares[2].take(),
            self.spares[3].take(),
        ];

        let telemetry_request = self.telemetry_request;
        let packets = four_motor_packets(
            self.requested_values,
            telemetry_request.map(DshotMotor::index),
        );
        let mut first_duties = [0; 4];
        for (index, packet) in packets.into_iter().enumerate() {
            if let Some(buffer) = next[index].as_deref_mut() {
                first_duties[index] = L::encode(packet, self.timing, buffer);
            }
        }

        for motor in DshotMotor::ALL {
            let index = motor.index();
            let Some(buffer) = next[index].take() else {
                self.restore_spares_after(index, &mut next);
                self.latch_fault(false);
                return Err(DshotCommandError::Faulted);
            };
            match self.lanes.exchange_buffer(motor, buffer) {
                Ok(old) => self.spares[index] = Some(old),
                Err(returned) => {
                    self.spares[index] = Some(returned);
                    self.restore_spares_after(index, &mut next);
                    self.latch_fault(false);
                    return Err(DshotCommandError::Faulted);
                }
            }
        }

        self.frame_started_ms = now_ms;
        self.completion_mask = 0;
        self.busy = true;
        self.stats.frames_started = self.stats.frames_started.wrapping_add(1);

        // Enable all four DMA streams before opening any timer DMA request.
        for motor in DshotMotor::ALL {
            self.lanes.start_dma(motor);
        }
        self.lanes.start_frame(first_duties);
        self.telemetry_request_sent = telemetry_request;
        self.telemetry_request = None;
        Ok(())
    }

    /// Return the not-yet-exchanged buffers after lane `index` to their
    /// spares, so a failed frame loses no buffer.
    fn restore_spares_after(
        &mut self,
        index: usize,
        next: &mut [Option<&'static mut L::Buffer>; 4],
    ) {
        for (spare, buffer) in self.spares.iter_mut().zip(next.iter_mut()).skip(index + 1) {
            if let Some(buffer) = buffer.take() {
                *spare = Some(buffer);
            }
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
        self.lanes.force_outputs_low();
        for motor in DshotMotor::ALL {
            self.lanes.pause_and_clear(motor);
        }
    }
}
