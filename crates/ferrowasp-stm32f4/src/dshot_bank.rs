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

pub use ferrowasp_waveform::dshot::{
    COMPARE_DMA_SLOTS, DSHOT600_BITRATE_HZ, DshotCommandSequence, DshotTimingError,
};

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

    /// The motor on zero-based lane `index`, the order motor commands use.
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Motor1),
            1 => Some(Self::Motor2),
            2 => Some(Self::Motor3),
            3 => Some(Self::Motor4),
            _ => None,
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
pub enum DshotSpecialCommandError {
    Faulted,
    /// A special command needs every motor stopped and no command lease.
    MotorsRunning,
    /// A special-command run is already being sent.
    CommandPending,
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
    /// Special-command frames still to send, for one motor. Sent only while
    /// every throttle is zero; any throttle demand cancels it.
    command_sequence: Option<(DshotMotor, DshotCommandSequence)>,
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
            command_sequence: None,
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

    /// Queues a run of special-command frames for one motor, such as a spin
    /// direction change and save.
    ///
    /// Refused unless every motor is stopped with no command lease, so a
    /// command frame never replaces a throttle frame on a spinning motor.
    pub fn command_special(
        &mut self,
        motor: DshotMotor,
        sequence: DshotCommandSequence,
    ) -> Result<(), DshotSpecialCommandError> {
        if self.faulted {
            return Err(DshotSpecialCommandError::Faulted);
        }
        if self.lease_duration_ms.is_some() || self.requested_values.iter().any(|v| *v != 0) {
            return Err(DshotSpecialCommandError::MotorsRunning);
        }
        if self.command_sequence.is_some() {
            return Err(DshotSpecialCommandError::CommandPending);
        }
        self.command_sequence = Some((motor, sequence));
        Ok(())
    }

    /// Whether a special-command run is still being sent.
    pub const fn command_pending(&self) -> bool {
        self.command_sequence.is_some()
    }

    /// The next special-command frame, as (motor, value); `None` when there
    /// is none or a throttle demand has cancelled it.
    fn next_command_frame(&mut self) -> Option<(DshotMotor, u16)> {
        if self.requested_values.iter().any(|v| *v != 0) {
            self.command_sequence = None;
            return None;
        }
        let (motor, sequence) = self.command_sequence.as_mut()?;
        let motor = *motor;
        match sequence.next_value() {
            Some(value) => Some((motor, value)),
            None => {
                self.command_sequence = None;
                None
            }
        }
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

    /// Withdraws a telemetry request no frame has carried yet. The ESC
    /// manager has given up on it, so it must not reach an ESC later.
    pub fn cancel_telemetry_request(&mut self) {
        self.telemetry_request = None;
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

        // A special command needs the telemetry bit on its own lane, so a
        // pending telemetry request waits for the next ordinary frame.
        let (values, telemetry_lane, telemetry_request) = match self.next_command_frame() {
            Some((motor, command)) => {
                let mut values = [0; 4];
                values[motor.index()] = command;
                (values, Some(motor.index()), None)
            }
            None => (
                self.requested_values,
                self.telemetry_request.map(DshotMotor::index),
                self.telemetry_request,
            ),
        };
        let packets = four_motor_packets(values, telemetry_lane);
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
        if telemetry_request.is_some() {
            self.telemetry_request_sent = telemetry_request;
            self.telemetry_request = None;
        }
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
        self.command_sequence = None;
        self.command_stop();
        self.lanes.force_outputs_low();
        for motor in DshotMotor::ALL {
            self.lanes.pause_and_clear(motor);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use ferrowasp_waveform::dshot::{
        DSHOT_CMD_SAVE_SETTINGS, DSHOT_CMD_SPIN_DIRECTION_REVERSED, DSHOT_COMMAND_REPEATS,
    };
    use std::{boxed::Box, vec::Vec};

    /// Lanes that record, for each started frame, every lane's DShot value
    /// and telemetry bit, and complete every DMA transfer.
    #[derive(Default)]
    struct RecordingLanes {
        frames: Vec<[(u16, bool); 4]>,
    }

    impl DshotLanes for RecordingLanes {
        type Buffer = u16;

        fn encode(packet: DshotPacket, _timing: DshotTiming, buffer: &mut u16) -> u16 {
            *buffer = packet.0;
            0
        }

        fn exchange_buffer(
            &mut self,
            motor: DshotMotor,
            next: &'static mut u16,
        ) -> Result<&'static mut u16, &'static mut u16> {
            if motor == DshotMotor::Motor1 {
                self.frames.push([(0, false); 4]);
            }
            let frame = self.frames.last_mut().expect("motor 1 starts each frame");
            frame[motor.index()] = (*next >> 5, (*next >> 4) & 1 == 1);
            Ok(next)
        }

        fn start_dma(&mut self, _motor: DshotMotor) {}
        fn start_frame(&mut self, _first_duties: [u16; 4]) {}
        fn stop_frame(&mut self) {}
        fn force_outputs_low(&mut self) {}

        fn dma_status(&self, _motor: DshotMotor) -> (bool, bool) {
            (true, false)
        }

        fn pause_and_clear(&mut self, _motor: DshotMotor) {}
    }

    fn bank() -> DshotBank<RecordingLanes> {
        let spares = core::array::from_fn(|_| Box::leak(Box::new(0u16)));
        let timing = DshotTiming::from_clocks(168_000_000, DSHOT600_BITRATE_HZ).unwrap();
        DshotBank::from_lanes(RecordingLanes::default(), spares, timing)
    }

    /// Start a frame and complete it on every lane.
    fn frame(bank: &mut DshotBank<RecordingLanes>, now_ms: u64) {
        assert_eq!(bank.service(now_ms), DshotServiceEvent::FrameStarted);
        for motor in DshotMotor::ALL {
            assert_eq!(bank.on_dma_interrupt(motor), DshotInterruptEvent::Completed);
        }
    }

    #[test]
    fn a_direction_run_sends_each_command_ten_times_on_its_lane_with_telemetry() {
        let mut bank = bank();
        let sequence = DshotCommandSequence::spin_direction(true);
        bank.command_special(DshotMotor::Motor3, sequence).unwrap();
        assert!(bank.command_pending());

        let frames = usize::from(DSHOT_COMMAND_REPEATS) * 2;
        for now in 0..=frames as u64 {
            frame(&mut bank, now);
        }
        assert!(!bank.command_pending());

        let sent = &bank.lanes.frames;
        for (index, lanes) in sent[..frames].iter().enumerate() {
            let expected = if index < usize::from(DSHOT_COMMAND_REPEATS) {
                DSHOT_CMD_SPIN_DIRECTION_REVERSED
            } else {
                DSHOT_CMD_SAVE_SETTINGS
            };
            assert_eq!(lanes[2], (expected, true), "frame {index}");
            for lane in [0, 1, 3] {
                assert_eq!(lanes[lane], (0, false), "frame {index} lane {lane}");
            }
        }
        // The run is spent: the next frame is an ordinary stopped frame.
        assert_eq!(sent[frames], [(0, false); 4]);
    }

    #[test]
    fn a_command_run_is_refused_while_any_motor_runs_or_a_lease_holds() {
        let mut bank = bank();
        bank.command_throttles([0, 100, 0, 0], 0, 250).unwrap();
        assert_eq!(
            bank.command_special(
                DshotMotor::Motor1,
                DshotCommandSequence::spin_direction(false)
            ),
            Err(DshotSpecialCommandError::MotorsRunning)
        );

        let mut bank = self::bank();
        bank.command_special(
            DshotMotor::Motor1,
            DshotCommandSequence::spin_direction(false),
        )
        .unwrap();
        assert_eq!(
            bank.command_special(
                DshotMotor::Motor2,
                DshotCommandSequence::spin_direction(false)
            ),
            Err(DshotSpecialCommandError::CommandPending)
        );
    }

    #[test]
    fn a_throttle_demand_cancels_a_command_run() {
        let mut bank = bank();
        bank.command_special(
            DshotMotor::Motor1,
            DshotCommandSequence::spin_direction(true),
        )
        .unwrap();
        frame(&mut bank, 0);
        bank.command_throttles([100, 0, 0, 0], 1, 250).unwrap();
        frame(&mut bank, 1);

        assert!(!bank.command_pending());
        let last = bank.lanes.frames.last().unwrap();
        assert_ne!(last[0].0, DSHOT_CMD_SPIN_DIRECTION_REVERSED);
        assert!(last[0].0 >= 48, "a throttle value, not a command");
    }

    #[test]
    fn a_telemetry_request_waits_for_the_command_run() {
        let mut bank = bank();
        bank.command_special(
            DshotMotor::Motor2,
            DshotCommandSequence::spin_direction(false),
        )
        .unwrap();
        bank.request_telemetry(DshotMotor::Motor4).unwrap();
        frame(&mut bank, 0);
        assert_eq!(bank.take_telemetry_request_sent(), None);
        assert_eq!(bank.lanes.frames[0][3], (0, false));
    }

    #[test]
    fn a_withdrawn_telemetry_request_is_never_sent() {
        let mut bank = bank();
        bank.command_special(
            DshotMotor::Motor2,
            DshotCommandSequence::spin_direction(false),
        )
        .unwrap();
        bank.request_telemetry(DshotMotor::Motor4).unwrap();
        frame(&mut bank, 0);
        bank.cancel_telemetry_request();
        while bank.command_pending() {
            frame(&mut bank, 1);
        }
        frame(&mut bank, 1);
        assert_eq!(bank.take_telemetry_request_sent(), None);
        assert!(bank.lanes.frames.iter().all(|lanes| !lanes[3].1));
        // The slot is free for the next request.
        bank.request_telemetry(DshotMotor::Motor1).unwrap();
    }
}
