//! The four-motor DShot bank's safety state machine, independent of the
//! timers and DMA streams that put it on the wire.
//!
//! A bank owns every motor lane as one fault-containment unit: a command
//! lease that stops the motors when it is not renewed, one-shot telemetry
//! requests, frame completion that needs every lane's DMA, and a latched
//! fault that forces all outputs low. Each STM32 family's backend implements
//! `DshotLanes` over its own timers and DMA; the policy here is shared, so a
//! board on either family runs the flight-tested sequencing.
//!
//! In bidirectional mode (dshot.md) a frame set has two phases. Once every
//! lane has transmitted, the lanes turn around and sample their pins for the
//! ESCs' replies; the set completes when every sample window has. The
//! replies are decoded at the next `service`, outside the DMA interrupts. A
//! missing or bad reply is counted, never a fault; a lane that cannot turn
//! around, or a stream that never completes, faults the bank as a transmit
//! fault does.
//!
//! Requirements: HW-DSHOT-001
//! Safety-critical.

use ferrowasp_output::dshot::{
    DshotPacket, DshotTiming, command_lease_expired, four_motor_bidirectional_packets,
    four_motor_packets, throttles_to_dshot,
};
use ferrowasp_output::dshot_reply::{decode_gcr, reply_transitions};

pub use ferrowasp_output::dshot::{
    COMPARE_DMA_SLOTS, DSHOT600_BITRATE_HZ, DshotCommandSequence, DshotTimingError,
};
pub use ferrowasp_output::dshot_reply::{DshotReply, DshotReplyError, REPLY_WINDOW_SAMPLES};

pub const DSHOT_COMMAND_MAX: u16 = 2000;
/// The longest the bank goes without a frame set. Each accepted motor command
/// releases one at the control-loop rate (dshot.md); without commands - while
/// disarmed, in guarded idle, or after the control loop stops - a fallback
/// releases one at this period, so the ESCs always see frames and an
/// unrenewed lease is always found.
pub const DSHOT_SERVICE_PERIOD_MS: u32 = 2;
pub const DSHOT_FRAME_TIMEOUT_MS: u32 = 1;
const ALL_MOTORS_COMPLETE: u8 = 0b1111;

/// Whether the fallback must release the service: no frame set has started
/// for a full service period. Times are wrapping milliseconds, so the answer holds
/// across the counter's wrap.
pub const fn fallback_release_due(now_ms: u32, last_release_ms: u32) -> bool {
    now_ms.wrapping_sub(last_release_ms) >= DSHOT_SERVICE_PERIOD_MS
}

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
    /// A lane names a timer the bank was not given.
    LaneTimerMissing,
    /// Two lanes name the same timer channel.
    LaneChannelReused,
    /// The lane timers run from different clocks; one bank encodes every
    /// lane with one bit timing.
    LaneClocksDiffer,
    /// Bidirectional DShot was asked for, but a lane cannot receive replies.
    RepliesUnsupported,
}

/// The timer channels a DShot lane can drive. A lane's channel is part of
/// its type, so its compare register and DMA request are fixed at compile
/// time; `N` variants are an advanced timer's complementary outputs.
pub mod channel {
    /// A timer output channel: its zero-based index, and whether the lane
    /// drives the complementary output CHxN instead of CHx.
    pub trait LaneChannel: 'static {
        const INDEX: usize;
        const COMPLEMENTARY: bool;
    }

    macro_rules! lane_channels {
        ($($name:ident: $index:literal, $complementary:literal;)+) => {
            $(
                #[doc = concat!("Timer channel `", stringify!($name), "`.")]
                pub struct $name;

                impl LaneChannel for $name {
                    const INDEX: usize = $index;
                    const COMPLEMENTARY: bool = $complementary;
                }
            )+
        };
    }

    lane_channels! {
        Ch1: 0, false;
        Ch2: 1, false;
        Ch3: 2, false;
        Ch4: 3, false;
        Ch1N: 0, true;
        Ch2N: 1, true;
        Ch3N: 2, true;
    }
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

/// One frame set's decoded replies, by physical lane.
pub type DshotReplies = [Result<DshotReply, DshotReplyError>; 4];

/// Each lane's bidirectional reply accounting (the blackbox logs it, design §21).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DshotReplyStats {
    pub valid: [u32; 4],
    pub invalid: [u32; 4],
    /// Missing or bad replies since each lane's last valid one.
    pub consecutive_invalid: [u32; 4],
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

    /// Whether every lane can receive bidirectional replies.
    fn receives_replies(&self) -> bool {
        false
    }

    /// After every lane has transmitted: release the pins, retime the timers
    /// to pace sampling, switch each stream to fill its lane's sample buffer
    /// from the pin, and start. `false` if a lane could not turn around.
    fn begin_receive(&mut self) -> bool {
        false
    }

    /// After every sample window: turn each lane back to transmit, with its
    /// output parked. `false` if a lane could not turn back.
    fn end_receive(&mut self) -> bool {
        false
    }

    /// `motor`'s samples from its last receive window, and its pin's mask in
    /// each sample.
    fn reply_samples(&self, _motor: DshotMotor) -> Option<(&[u16], u16)> {
        None
    }
}

/// A DShot motor bank, whatever lanes it drives: what the shared tasks
/// need, so they take any board's bank without naming its timers or streams.
pub trait DshotMotors {
    fn command_stop(&mut self);
    fn command_throttles(
        &mut self,
        commands: [u16; 4],
        now_ms: u64,
        lease_duration_ms: u32,
    ) -> Result<(), DshotCommandError>;
    fn command_special(
        &mut self,
        motor: DshotMotor,
        sequence: DshotCommandSequence,
    ) -> Result<(), DshotSpecialCommandError>;
    fn request_telemetry(&mut self, motor: DshotMotor) -> Result<(), DshotTelemetryRequestError>;
    fn cancel_telemetry_request(&mut self);
    fn take_telemetry_request_sent(&mut self) -> Option<DshotMotor>;
    fn service(&mut self, now_ms: u64) -> DshotServiceEvent;
    fn on_dma_interrupt(&mut self, motor: DshotMotor) -> DshotInterruptEvent;
    fn is_faulted(&self) -> bool;
    fn stats(&self) -> DshotStats;
    fn requested_values(&self) -> [u16; 4];
    fn is_bidirectional(&self) -> bool;
    fn take_replies(&mut self) -> Option<DshotReplies>;
    fn reply_stats(&self) -> DshotReplyStats;
}

impl<L> DshotMotors for DshotBank<L>
where
    L: DshotLanes,
{
    fn command_stop(&mut self) {
        DshotBank::command_stop(self)
    }

    fn command_throttles(
        &mut self,
        commands: [u16; 4],
        now_ms: u64,
        lease_duration_ms: u32,
    ) -> Result<(), DshotCommandError> {
        DshotBank::command_throttles(self, commands, now_ms, lease_duration_ms)
    }

    fn command_special(
        &mut self,
        motor: DshotMotor,
        sequence: DshotCommandSequence,
    ) -> Result<(), DshotSpecialCommandError> {
        DshotBank::command_special(self, motor, sequence)
    }

    fn request_telemetry(&mut self, motor: DshotMotor) -> Result<(), DshotTelemetryRequestError> {
        DshotBank::request_telemetry(self, motor)
    }

    fn cancel_telemetry_request(&mut self) {
        DshotBank::cancel_telemetry_request(self)
    }

    fn take_telemetry_request_sent(&mut self) -> Option<DshotMotor> {
        DshotBank::take_telemetry_request_sent(self)
    }

    fn service(&mut self, now_ms: u64) -> DshotServiceEvent {
        DshotBank::service(self, now_ms)
    }

    fn on_dma_interrupt(&mut self, motor: DshotMotor) -> DshotInterruptEvent {
        DshotBank::on_dma_interrupt(self, motor)
    }

    fn is_faulted(&self) -> bool {
        DshotBank::is_faulted(self)
    }

    fn stats(&self) -> DshotStats {
        DshotBank::stats(self)
    }

    fn requested_values(&self) -> [u16; 4] {
        DshotBank::requested_values(self)
    }

    fn is_bidirectional(&self) -> bool {
        DshotBank::is_bidirectional(self)
    }

    fn take_replies(&mut self) -> Option<DshotReplies> {
        DshotBank::take_replies(self)
    }

    fn reply_stats(&self) -> DshotReplyStats {
        DshotBank::reply_stats(self)
    }
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
    /// Inverted checksums and a receive phase per frame set (dshot.md).
    bidirectional: bool,
    /// The current frame set has transmitted and is sampling replies.
    receiving: bool,
    /// A receive window has completed and not been decoded yet.
    replies_waiting: bool,
    replies: Option<DshotReplies>,
    reply_stats: DshotReplyStats,
}

impl<L> DshotBank<L>
where
    L: DshotLanes,
{
    /// A bank over lanes already holding their first buffers, with one spare
    /// per lane.
    /// The lanes, for tests of the code that builds them.
    #[cfg(test)]
    pub(crate) fn lanes(&self) -> &L {
        &self.lanes
    }

    #[cfg(test)]
    pub(crate) fn lanes_mut(&mut self) -> &mut L {
        &mut self.lanes
    }

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
            bidirectional: false,
            receiving: false,
            replies_waiting: false,
            replies: None,
            reply_stats: DshotReplyStats::default(),
        }
    }

    /// Switches to bidirectional DShot, before the first frame. Refused if a
    /// lane cannot receive replies.
    pub fn enable_bidirectional(&mut self) -> Result<(), DshotInitError> {
        if !self.lanes.receives_replies() {
            return Err(DshotInitError::RepliesUnsupported);
        }
        self.bidirectional = true;
        Ok(())
    }

    pub const fn is_bidirectional(&self) -> bool {
        self.bidirectional
    }

    /// The replies decoded since the last call, once.
    pub fn take_replies(&mut self) -> Option<DshotReplies> {
        self.replies.take()
    }

    pub const fn reply_stats(&self) -> DshotReplyStats {
        self.reply_stats
    }

    /// Decodes the last receive window. Extended telemetry is not enabled
    /// yet, so every value is read as eRPM.
    fn decode_replies(&mut self) {
        let replies = DshotMotor::ALL.map(|motor| {
            let (samples, pin_mask) = self
                .lanes
                .reply_samples(motor)
                .ok_or(DshotReplyError::NoReply)?;
            let value = decode_gcr(reply_transitions(samples, pin_mask)?)?;
            DshotReply::from_value(value, false)
        });
        for (index, reply) in replies.iter().enumerate() {
            let stats = &mut self.reply_stats;
            if reply.is_ok() {
                stats.valid[index] = stats.valid[index].wrapping_add(1);
                stats.consecutive_invalid[index] = 0;
            } else {
                stats.invalid[index] = stats.invalid[index].wrapping_add(1);
                stats.consecutive_invalid[index] =
                    stats.consecutive_invalid[index].saturating_add(1);
            }
        }
        self.replies = Some(replies);
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

        if self.replies_waiting {
            self.replies_waiting = false;
            self.decode_replies();
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
        if !self.receiving {
            let lane = &mut self.stats.lane_completions[motor.index()];
            *lane = lane.wrapping_add(1);
        }

        if self.completion_mask == ALL_MOTORS_COMPLETE {
            if self.bidirectional && !self.receiving {
                if !self.lanes.begin_receive() {
                    self.latch_fault(false);
                    return DshotInterruptEvent::Faulted;
                }
                self.receiving = true;
                self.completion_mask = 0;
                return DshotInterruptEvent::Completed;
            }
            if self.receiving {
                self.receiving = false;
                if !self.lanes.end_receive() {
                    self.latch_fault(false);
                    return DshotInterruptEvent::Faulted;
                }
                self.replies_waiting = true;
            } else {
                self.lanes.stop_frame();
            }
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
        let packets = if self.bidirectional {
            four_motor_bidirectional_packets(values, telemetry_lane)
        } else {
            four_motor_packets(values, telemetry_lane)
        };
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
        self.receiving = false;
        self.replies_waiting = false;
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
    use ferrowasp_output::dshot::{
        DSHOT_CMD_SAVE_SETTINGS, DSHOT_CMD_SPIN_DIRECTION_REVERSED, DSHOT_COMMAND_REPEATS,
    };
    use std::{boxed::Box, vec::Vec};

    /// Lanes that record, for each started frame, every lane's DShot value
    /// and telemetry bit, and complete every DMA transfer. With `replies`
    /// set they can receive: each lane's ESC answers with its value, or
    /// stays silent for `None`.
    #[derive(Default)]
    struct RecordingLanes {
        frames: Vec<[(u16, bool); 4]>,
        packets: Vec<[u16; 4]>,
        replies: Option<[Option<u16>; 4]>,
        samples: [Vec<u16>; 4],
        receive_calls: Vec<&'static str>,
        fail_begin: bool,
        fail_end: bool,
    }

    /// A pin sampled through one receive window: idle high, then `value`'s
    /// reply at three samples per bit, then idle.
    fn sampled_reply(value: Option<u16>) -> Vec<u16> {
        let mut samples = std::vec![1u16; 40];
        if let Some(value) = value {
            let transitions = ferrowasp_output::dshot_reply::reply_transitions_for(value);
            let mut level = 1;
            for bit in (0..21).rev() {
                if transitions & (1 << bit) != 0 {
                    level ^= 1;
                }
                samples.extend([level; 3]);
            }
        }
        samples.resize(REPLY_WINDOW_SAMPLES, 1);
        samples
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
            if motor == DshotMotor::Motor1 {
                self.packets.push([0; 4]);
            }
            let frame = self.frames.last_mut().expect("motor 1 starts each frame");
            frame[motor.index()] = (*next >> 5, (*next >> 4) & 1 == 1);
            self.packets.last_mut().expect("motor 1 starts each frame")[motor.index()] = *next;
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

        fn receives_replies(&self) -> bool {
            self.replies.is_some()
        }

        fn begin_receive(&mut self) -> bool {
            self.receive_calls.push("begin");
            if let Some(replies) = self.replies {
                for (samples, reply) in self.samples.iter_mut().zip(replies) {
                    *samples = sampled_reply(reply);
                }
            }
            !self.fail_begin
        }

        fn end_receive(&mut self) -> bool {
            self.receive_calls.push("end");
            !self.fail_end
        }

        fn reply_samples(&self, motor: DshotMotor) -> Option<(&[u16], u16)> {
            Some((&self.samples[motor.index()], 1))
        }
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

    /// A bidirectional bank whose ESCs answer with `replies`.
    fn bidirectional_bank(replies: [Option<u16>; 4]) -> DshotBank<RecordingLanes> {
        let mut bank = bank();
        bank.lanes.replies = Some(replies);
        bank.enable_bidirectional().unwrap();
        bank
    }

    /// Start a bidirectional frame set and complete both phases on every
    /// lane.
    fn bidirectional_frame(bank: &mut DshotBank<RecordingLanes>, now_ms: u64) {
        assert_eq!(bank.service(now_ms), DshotServiceEvent::FrameStarted);
        for _phase in 0..2 {
            for motor in DshotMotor::ALL {
                assert_eq!(bank.on_dma_interrupt(motor), DshotInterruptEvent::Completed);
            }
        }
    }

    /// eRPM/100 of a reply value: 1,000 us is 500 << 1, 600.
    const PERIOD_1000_US: u16 = (1 << 9) | 500;

    #[test]
    fn bidirectional_needs_lanes_that_receive() {
        let mut bank = bank();
        assert_eq!(
            bank.enable_bidirectional(),
            Err(DshotInitError::RepliesUnsupported)
        );
        assert!(!bank.is_bidirectional());
    }

    #[test]
    fn bidirectional_frames_invert_every_checksum() {
        let mut bank = bidirectional_bank([None; 4]);
        bank.command_throttles([100, 200, 300, 400], 0, 250)
            .unwrap();
        bidirectional_frame(&mut bank, 0);
        let packets = bank.lanes.packets[0];
        let values = throttles_to_dshot([100, 200, 300, 400]);
        for (packet, value) in packets.into_iter().zip(values) {
            assert_eq!(
                DshotPacket(packet),
                DshotPacket::from_value_bidirectional(value, false)
            );
        }
    }

    #[test]
    fn a_bidirectional_frame_set_completes_only_after_its_receive_window() {
        let mut bank = bidirectional_bank([Some(PERIOD_1000_US); 4]);
        assert_eq!(bank.service(0), DshotServiceEvent::FrameStarted);
        for motor in DshotMotor::ALL {
            bank.on_dma_interrupt(motor);
        }
        assert_eq!(bank.lanes.receive_calls, ["begin"]);
        assert_eq!(bank.service(0), DshotServiceEvent::Busy);
        for motor in DshotMotor::ALL {
            assert_eq!(bank.on_dma_interrupt(motor), DshotInterruptEvent::Completed);
        }
        assert_eq!(bank.lanes.receive_calls, ["begin", "end"]);
        assert_eq!(bank.stats().frames_completed, 1);
        assert_eq!(bank.stats().lane_completions, [1; 4]);
        assert_eq!(bank.service(1), DshotServiceEvent::FrameStarted);
    }

    #[test]
    fn replies_are_decoded_at_the_next_service_and_taken_once() {
        let mut bank = bidirectional_bank([Some(PERIOD_1000_US), Some(0x0fff), None, Some(1)]);
        bidirectional_frame(&mut bank, 0);
        assert_eq!(
            bank.take_replies(),
            None,
            "decoded only at the next service"
        );
        bidirectional_frame(&mut bank, 1);
        assert_eq!(
            bank.take_replies(),
            Some([
                Ok(DshotReply::Erpm { erpm_div100: 600 }),
                Ok(DshotReply::Erpm { erpm_div100: 0 }),
                Err(DshotReplyError::NoReply),
                Ok(DshotReply::Erpm {
                    erpm_div100: 600_000
                }),
            ])
        );
        assert_eq!(bank.take_replies(), None);
    }

    #[test]
    fn missing_replies_are_counted_never_a_fault() {
        let mut bank = bidirectional_bank([Some(PERIOD_1000_US), None, None, None]);
        for now in 0..5 {
            bidirectional_frame(&mut bank, now);
        }
        // Four windows decoded; the fifth waits for the next service.
        let stats = bank.reply_stats();
        assert_eq!(stats.valid, [4, 0, 0, 0]);
        assert_eq!(stats.invalid, [0, 4, 4, 4]);
        assert_eq!(stats.consecutive_invalid, [0, 4, 4, 4]);
        assert!(!bank.is_faulted());

        bank.lanes.replies = Some([Some(PERIOD_1000_US); 4]);
        bidirectional_frame(&mut bank, 5);
        bidirectional_frame(&mut bank, 6);
        assert_eq!(bank.reply_stats().consecutive_invalid, [0, 0, 0, 0]);
    }

    #[test]
    fn a_lane_that_cannot_turn_around_faults_the_bank() {
        let mut bank = bidirectional_bank([None; 4]);
        bank.lanes.fail_begin = true;
        assert_eq!(bank.service(0), DshotServiceEvent::FrameStarted);
        for motor in [DshotMotor::Motor1, DshotMotor::Motor2, DshotMotor::Motor3] {
            assert_eq!(bank.on_dma_interrupt(motor), DshotInterruptEvent::Completed);
        }
        assert_eq!(
            bank.on_dma_interrupt(DshotMotor::Motor4),
            DshotInterruptEvent::Faulted
        );
        assert!(bank.is_faulted());
    }

    #[test]
    fn a_lane_that_cannot_turn_back_faults_the_bank() {
        let mut bank = bidirectional_bank([None; 4]);
        bank.lanes.fail_end = true;
        assert_eq!(bank.service(0), DshotServiceEvent::FrameStarted);
        for motor in DshotMotor::ALL {
            bank.on_dma_interrupt(motor);
        }
        for motor in [DshotMotor::Motor1, DshotMotor::Motor2, DshotMotor::Motor3] {
            bank.on_dma_interrupt(motor);
        }
        assert_eq!(
            bank.on_dma_interrupt(DshotMotor::Motor4),
            DshotInterruptEvent::Faulted
        );
        assert!(bank.is_faulted());
    }

    #[test]
    fn a_receive_window_that_never_closes_times_out_as_a_frame() {
        let mut bank = bidirectional_bank([None; 4]);
        assert_eq!(bank.service(0), DshotServiceEvent::FrameStarted);
        for motor in DshotMotor::ALL {
            bank.on_dma_interrupt(motor);
        }
        assert_eq!(bank.service(1), DshotServiceEvent::Busy);
        assert_eq!(bank.service(2), DshotServiceEvent::Faulted);
        assert_eq!(bank.stats().frame_timeouts, 1);
    }

    #[test]
    fn a_unidirectional_bank_never_receives() {
        let mut bank = bank();
        frame(&mut bank, 0);
        frame(&mut bank, 1);
        assert!(bank.lanes.receive_calls.is_empty());
        assert_eq!(bank.take_replies(), None);
        let normal = DshotPacket::from_value(0, false).0;
        assert!(bank.lanes.packets.iter().all(|set| set == &[normal; 4]));
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
    fn the_fallback_waits_out_one_service_period_after_any_release() {
        assert!(!fallback_release_due(100, 100));
        assert!(!fallback_release_due(101, 100));
        assert!(fallback_release_due(102, 100));
        assert!(fallback_release_due(150, 100));
    }

    #[test]
    fn commands_at_the_control_rate_hold_the_fallback_off() {
        // A 2 kHz control loop releases twice per millisecond tick, so the
        // newest release is never a full period old.
        for tick in 0..10u32 {
            assert!(!fallback_release_due(tick, tick));
            assert!(!fallback_release_due(tick + 1, tick));
        }
    }

    #[test]
    fn the_fallback_survives_the_millisecond_counter_wrap() {
        assert!(!fallback_release_due(0, u32::MAX));
        assert!(fallback_release_due(1, u32::MAX));
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
