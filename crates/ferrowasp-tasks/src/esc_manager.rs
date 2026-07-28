use ferrowasp_drivers::blheli_telemetry::{
    EscTelemetry, ParserStats as WireParserStats, StreamParser,
};
use heapless::spsc::{Consumer, Producer, Queue};

pub const ESC_REQUEST_QUEUE_CAPACITY: usize = 4;
pub const ESC_ACK_QUEUE_CAPACITY: usize = 4;
pub const ESC_TELEMETRY_UPDATE_QUEUE_CAPACITY: usize = 16;

pub type EscRequestQueue = Queue<EscActuatorRequest, ESC_REQUEST_QUEUE_CAPACITY>;
pub type EscRequestProducer = Producer<'static, EscActuatorRequest>;
pub type EscRequestConsumer = Consumer<'static, EscActuatorRequest>;
pub type EscAckQueue = Queue<EscActuatorAck, ESC_ACK_QUEUE_CAPACITY>;
pub type EscAckProducer = Producer<'static, EscActuatorAck>;
pub type EscAckConsumer = Consumer<'static, EscActuatorAck>;
pub type EscTelemetryUpdateQueue = Queue<EscTelemetryUpdate, ESC_TELEMETRY_UPDATE_QUEUE_CAPACITY>;
pub type EscTelemetryUpdateProducer = Producer<'static, EscTelemetryUpdate>;
pub type EscTelemetryUpdateConsumer = Consumer<'static, EscTelemetryUpdate>;

/// Physical FCU motor-output lane selected for a telemetry request.
///
/// This is deliberately not a logical airframe motor number. Board/application
/// code owns the mapping between physical outputs and logical motor positions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscOutput {
    Output1,
    Output2,
    Output3,
    Output4,
}

impl EscOutput {
    pub const fn index(self) -> usize {
        match self {
            Self::Output1 => 0,
            Self::Output2 => 1,
            Self::Output3 => 2,
            Self::Output4 => 3,
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Output1 => Self::Output2,
            Self::Output2 => Self::Output3,
            Self::Output3 => Self::Output4,
            Self::Output4 => Self::Output1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscOperation {
    RequestTelemetry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EscActuatorRequest {
    pub sequence: u32,
    pub output: EscOutput,
    pub operation: EscOperation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EscActuatorAck {
    pub request: EscActuatorRequest,
    pub started_at_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EscTelemetryObservation {
    pub sample: EscTelemetry,
    pub observed_at_ms: u32,
    pub request_sequence: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EscTelemetryUpdate {
    pub output: EscOutput,
    pub observation: EscTelemetryObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscAckOutcome {
    Accepted,
    Sample(EscTelemetryUpdate),
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscManagerTimeout {
    ActuatorAck(EscActuatorRequest),
    TelemetryResponse(EscActuatorRequest),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EscManagerConfig {
    pub boot_delay_ms: u32,
    pub request_period_ms: u32,
    pub actuator_ack_timeout_ms: u32,
    pub telemetry_response_timeout_ms: u32,
}

impl EscManagerConfig {
    pub const fn legacy_uart() -> Self {
        Self {
            boot_delay_ms: 5_000,
            request_period_ms: 20,
            actuator_ack_timeout_ms: 20,
            telemetry_response_timeout_ms: 100,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EscManagerStats {
    pub requests_queued: u32,
    pub requests_started: u32,
    pub actuator_ack_timeouts: u32,
    pub telemetry_response_timeouts: u32,
    pub mismatched_acks: u32,
    pub unsolicited_frames: u32,
    pub wire: WireParserStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingRequest {
    Queued {
        request: EscActuatorRequest,
        queued_at_ms: u32,
        early_observation: Option<EscTelemetryObservation>,
    },
    Started {
        request: EscActuatorRequest,
        started_at_ms: u32,
    },
}

pub struct EscManager {
    config: EscManagerConfig,
    boot_started_ms: u32,
    next_output: EscOutput,
    next_sequence: u32,
    last_request_ms: Option<u32>,
    pending: Option<PendingRequest>,
    // Legacy BLHeli frames carry no output identity. Once either side of a
    // request times out, a late acknowledgement or frame cannot be safely
    // associated with a later request, so telemetry remains fail-closed until
    // reboot.
    faulted: bool,
    parser: StreamParser,
    samples: [Option<EscTelemetryObservation>; 4],
    stats: EscManagerStats,
}

impl EscManager {
    pub const fn new(config: EscManagerConfig, boot_started_ms: u32) -> Self {
        Self {
            config,
            boot_started_ms,
            next_output: EscOutput::Output1,
            next_sequence: 0,
            last_request_ms: None,
            pending: None,
            faulted: false,
            parser: StreamParser::new(),
            samples: [None; 4],
            stats: EscManagerStats {
                requests_queued: 0,
                requests_started: 0,
                actuator_ack_timeouts: 0,
                telemetry_response_timeouts: 0,
                mismatched_acks: 0,
                unsolicited_frames: 0,
                wire: WireParserStats {
                    valid_frames: 0,
                    crc_failures: 0,
                    discarded_bytes: 0,
                },
            },
        }
    }

    /// Returns the next operation the manager wants the actuator owner to
    /// execute. The caller must invoke `mark_request_queued` only after the
    /// bounded channel accepts it.
    pub fn next_request(&self, now_ms: u32) -> Option<EscActuatorRequest> {
        if self.faulted
            || self.pending.is_some()
            || !elapsed_at_least(self.boot_started_ms, now_ms, self.config.boot_delay_ms)
            || self
                .last_request_ms
                .is_some_and(|last| !elapsed_at_least(last, now_ms, self.config.request_period_ms))
        {
            return None;
        }

        Some(EscActuatorRequest {
            sequence: self.next_sequence,
            output: self.next_output,
            operation: EscOperation::RequestTelemetry,
        })
    }

    pub fn mark_request_queued(&mut self, request: EscActuatorRequest, now_ms: u32) -> bool {
        if self.pending.is_some() || self.next_request(now_ms) != Some(request) {
            return false;
        }

        self.pending = Some(PendingRequest::Queued {
            request,
            queued_at_ms: now_ms,
            early_observation: None,
        });
        self.last_request_ms = Some(now_ms);
        self.next_output = self.next_output.next();
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.stats.requests_queued = self.stats.requests_queued.wrapping_add(1);
        true
    }

    pub fn on_actuator_ack(&mut self, ack: EscActuatorAck) -> EscAckOutcome {
        let Some(PendingRequest::Queued {
            request,
            early_observation,
            ..
        }) = self.pending
        else {
            self.stats.mismatched_acks = self.stats.mismatched_acks.wrapping_add(1);
            return EscAckOutcome::Rejected;
        };
        if ack.request != request {
            self.stats.mismatched_acks = self.stats.mismatched_acks.wrapping_add(1);
            return EscAckOutcome::Rejected;
        }

        self.stats.requests_started = self.stats.requests_started.wrapping_add(1);
        if let Some(observation) = early_observation {
            self.pending = None;
            self.samples[request.output.index()] = Some(observation);
            EscAckOutcome::Sample(EscTelemetryUpdate {
                output: request.output,
                observation,
            })
        } else {
            self.pending = Some(PendingRequest::Started {
                request,
                started_at_ms: ack.started_at_ms,
            });
            EscAckOutcome::Accepted
        }
    }

    pub fn poll_timeout(&mut self, now_ms: u32) -> Option<EscManagerTimeout> {
        match self.pending {
            Some(PendingRequest::Queued {
                request,
                queued_at_ms,
                ..
            }) if elapsed_at_least(queued_at_ms, now_ms, self.config.actuator_ack_timeout_ms) => {
                self.pending = None;
                self.faulted = true;
                self.stats.actuator_ack_timeouts = self.stats.actuator_ack_timeouts.wrapping_add(1);
                Some(EscManagerTimeout::ActuatorAck(request))
            }
            Some(PendingRequest::Started {
                request,
                started_at_ms,
            }) if elapsed_at_least(
                started_at_ms,
                now_ms,
                self.config.telemetry_response_timeout_ms,
            ) =>
            {
                self.pending = None;
                self.faulted = true;
                self.stats.telemetry_response_timeouts =
                    self.stats.telemetry_response_timeouts.wrapping_add(1);
                Some(EscManagerTimeout::TelemetryResponse(request))
            }
            _ => None,
        }
    }

    pub fn push_wire_byte(&mut self, byte: u8, observed_at_ms: u32) -> Option<EscTelemetryUpdate> {
        let sample = self.parser.push(byte)?;
        self.stats.wire = self.parser.stats();

        match self.pending {
            Some(PendingRequest::Queued {
                request,
                queued_at_ms,
                ..
            }) if request.operation == EscOperation::RequestTelemetry => {
                // The actuator task runs at a higher priority and can enqueue
                // its acknowledgement after this manager iteration has
                // already drained the ack queue. Hold the validated frame
                // until that exact sequence/output acknowledgement is read.
                let observation = EscTelemetryObservation {
                    sample,
                    observed_at_ms,
                    request_sequence: request.sequence,
                };
                self.pending = Some(PendingRequest::Queued {
                    request,
                    queued_at_ms,
                    early_observation: Some(observation),
                });
                None
            }
            Some(PendingRequest::Started { request, .. })
                if request.operation == EscOperation::RequestTelemetry =>
            {
                let observation = EscTelemetryObservation {
                    sample,
                    observed_at_ms,
                    request_sequence: request.sequence,
                };
                self.pending = None;
                self.samples[request.output.index()] = Some(observation);
                Some(EscTelemetryUpdate {
                    output: request.output,
                    observation,
                })
            }
            _ => {
                self.stats.unsolicited_frames = self.stats.unsolicited_frames.wrapping_add(1);
                None
            }
        }
    }

    pub fn record_wire_discontinuity(&mut self) {
        self.parser.reset();
    }

    pub fn refresh_wire_stats(&mut self) {
        self.stats.wire = self.parser.stats();
    }

    pub const fn samples(&self) -> [Option<EscTelemetryObservation>; 4] {
        self.samples
    }

    /// Whether an association timeout has latched telemetry off until reboot.
    pub const fn is_faulted(&self) -> bool {
        self.faulted
    }

    pub const fn stats(&self) -> EscManagerStats {
        self.stats
    }
}

const fn elapsed_at_least(start_ms: u32, now_ms: u32, duration_ms: u32) -> bool {
    now_ms.wrapping_sub(start_ms) >= duration_ms
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EscIdleQualificationConfig {
    pub min_erpm_div100: u16,
    pub max_erpm_div100: u16,
    pub spinup_grace_ms: u32,
    pub timeout_ms: u32,
    pub max_sample_age_ms: u32,
    pub required_consecutive_samples: u8,
}

impl EscIdleQualificationConfig {
    pub const fn is_valid(self) -> bool {
        self.min_erpm_div100 > 0
            && self.min_erpm_div100 < self.max_erpm_div100
            && self.spinup_grace_ms < self.timeout_ms
            && self.max_sample_age_ms > 0
            && self.required_consecutive_samples > 0
    }
}

// Target-proven on both FCU3 and Foxeer F405 V2. A board that cannot use this
// policy must provide a separately qualified profile rather than silently
// changing one field.
pub const DSHOT_PREARM_STOP_HOLD_MS: u32 = 100;
pub const DSHOT_IDLE_THROTTLE_COMMAND: u16 = 65;
pub const DSHOT_IDLE_QUALIFICATION_CONFIG: EscIdleQualificationConfig =
    EscIdleQualificationConfig {
        min_erpm_div100: 30,
        max_erpm_div100: 100,
        spinup_grace_ms: 250,
        timeout_ms: 1_200,
        max_sample_age_ms: 200,
        required_consecutive_samples: 3,
    };

const _: () = assert!(DSHOT_IDLE_QUALIFICATION_CONFIG.is_valid());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscIdleQualificationFailure {
    InvalidConfig,
    Overspeed { output: EscOutput, erpm_div100: u16 },
    Timeout { consecutive_samples: [u8; 4] },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscIdleQualificationStatus {
    Pending,
    Qualified,
    Failed(EscIdleQualificationFailure),
}

pub struct EscIdleQualification {
    config: EscIdleQualificationConfig,
    started_at_ms: u32,
    consecutive_samples: [u8; 4],
    last_valid_sample_ms: [Option<u32>; 4],
    terminal: Option<EscIdleQualificationStatus>,
}

impl EscIdleQualification {
    pub const fn new(config: EscIdleQualificationConfig, started_at_ms: u32) -> Self {
        Self {
            config,
            started_at_ms,
            consecutive_samples: [0; 4],
            last_valid_sample_ms: [None; 4],
            terminal: None,
        }
    }

    pub fn observe(
        &mut self,
        update: EscTelemetryUpdate,
        now_ms: u32,
    ) -> EscIdleQualificationStatus {
        if let Some(terminal) = self.terminal {
            return terminal;
        }
        if !self.config.is_valid() {
            return self.fail(EscIdleQualificationFailure::InvalidConfig);
        }
        if !elapsed_at_least(self.started_at_ms, now_ms, self.config.spinup_grace_ms) {
            return EscIdleQualificationStatus::Pending;
        }

        let index = update.output.index();
        let erpm = update.observation.sample.erpm_div100;
        if erpm > self.config.max_erpm_div100 {
            return self.fail(EscIdleQualificationFailure::Overspeed {
                output: update.output,
                erpm_div100: erpm,
            });
        }
        if erpm < self.config.min_erpm_div100 {
            self.consecutive_samples[index] = 0;
            self.last_valid_sample_ms[index] = None;
        } else {
            self.consecutive_samples[index] = self.consecutive_samples[index].saturating_add(1);
            self.last_valid_sample_ms[index] = Some(update.observation.observed_at_ms);
        }

        self.status(now_ms)
    }

    pub fn status(&mut self, now_ms: u32) -> EscIdleQualificationStatus {
        if let Some(terminal) = self.terminal {
            return terminal;
        }
        if !self.config.is_valid() {
            return self.fail(EscIdleQualificationFailure::InvalidConfig);
        }

        for index in 0..4 {
            if self.last_valid_sample_ms[index].is_some_and(|observed_at_ms| {
                elapsed_at_least(
                    observed_at_ms,
                    now_ms,
                    self.config.max_sample_age_ms.saturating_add(1),
                )
            }) {
                self.consecutive_samples[index] = 0;
                self.last_valid_sample_ms[index] = None;
            }
        }

        if self
            .consecutive_samples
            .iter()
            .all(|count| *count >= self.config.required_consecutive_samples)
        {
            self.terminal = Some(EscIdleQualificationStatus::Qualified);
            return EscIdleQualificationStatus::Qualified;
        }

        if elapsed_at_least(self.started_at_ms, now_ms, self.config.timeout_ms) {
            return self.fail(EscIdleQualificationFailure::Timeout {
                consecutive_samples: self.consecutive_samples,
            });
        }

        EscIdleQualificationStatus::Pending
    }

    pub const fn consecutive_samples(&self) -> [u8; 4] {
        self.consecutive_samples
    }

    fn fail(&mut self, failure: EscIdleQualificationFailure) -> EscIdleQualificationStatus {
        let status = EscIdleQualificationStatus::Failed(failure);
        self.terminal = Some(status);
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_drivers::blheli_telemetry::{FRAME_LEN, crc8};

    fn frame() -> [u8; FRAME_LEN] {
        let mut frame = [42, 0x06, 0x7c, 0x00, 0xfa, 0x01, 0x2c, 0x04, 0xd2, 0];
        frame[9] = crc8(&frame[..9]);
        frame
    }

    fn telemetry_update(
        output: EscOutput,
        erpm_div100: u16,
        observed_at_ms: u32,
        request_sequence: u32,
    ) -> EscTelemetryUpdate {
        EscTelemetryUpdate {
            output,
            observation: EscTelemetryObservation {
                sample: EscTelemetry {
                    temperature_c: 0,
                    voltage_cv: 0,
                    current_ca: 0,
                    consumption_mah: 0,
                    erpm_div100,
                },
                observed_at_ms,
                request_sequence,
            },
        }
    }

    fn idle_qualification_config() -> EscIdleQualificationConfig {
        DSHOT_IDLE_QUALIFICATION_CONFIG
    }

    #[test]
    fn waits_for_boot_and_never_overlaps_requests() {
        let mut manager = EscManager::new(EscManagerConfig::legacy_uart(), 0);
        assert_eq!(manager.next_request(4_999), None);

        let request = manager.next_request(5_000).unwrap();
        assert_eq!(request.output, EscOutput::Output1);
        assert!(manager.mark_request_queued(request, 5_000));
        assert_eq!(manager.next_request(5_020), None);
    }

    #[test]
    fn actuator_ack_then_frame_associates_sample_with_requested_output() {
        let mut manager = EscManager::new(EscManagerConfig::legacy_uart(), 0);
        let request = manager.next_request(5_000).unwrap();
        assert!(manager.mark_request_queued(request, 5_000));
        assert_eq!(
            manager.on_actuator_ack(EscActuatorAck {
                request,
                started_at_ms: 5_002,
            }),
            EscAckOutcome::Accepted
        );

        let mut update = None;
        for byte in frame() {
            update = manager.push_wire_byte(byte, 5_003).or(update);
        }

        assert_eq!(update.unwrap().output, EscOutput::Output1);
        assert_eq!(manager.samples()[0].unwrap().sample.voltage_cv, 1_660);
        assert_eq!(manager.stats().requests_started, 1);
    }

    #[test]
    fn actuator_ack_timeout_latches_manager_fault() {
        let mut manager = EscManager::new(EscManagerConfig::legacy_uart(), 0);
        let request = manager.next_request(5_000).unwrap();
        assert!(manager.mark_request_queued(request, 5_000));
        assert_eq!(
            manager.poll_timeout(5_020),
            Some(EscManagerTimeout::ActuatorAck(request))
        );
        assert!(manager.is_faulted());
        assert_eq!(manager.next_request(5_020), None);
        assert_eq!(
            manager.on_actuator_ack(EscActuatorAck {
                request,
                started_at_ms: 5_021,
            }),
            EscAckOutcome::Rejected
        );

        for byte in frame() {
            assert_eq!(manager.push_wire_byte(byte, 5_021), None);
        }
        assert_eq!(manager.samples(), [None; 4]);
        assert_eq!(manager.stats().mismatched_acks, 1);
        assert_eq!(manager.stats().unsolicited_frames, 1);
    }

    #[test]
    fn telemetry_response_timeout_latches_manager_fault() {
        let mut manager = EscManager::new(EscManagerConfig::legacy_uart(), 0);
        let request = manager.next_request(5_000).unwrap();
        assert!(manager.mark_request_queued(request, 5_000));
        assert_eq!(
            manager.on_actuator_ack(EscActuatorAck {
                request,
                started_at_ms: 5_002,
            }),
            EscAckOutcome::Accepted
        );
        assert_eq!(
            manager.poll_timeout(5_102),
            Some(EscManagerTimeout::TelemetryResponse(request))
        );
        assert!(manager.is_faulted());
        assert_eq!(manager.next_request(5_102), None);

        for byte in frame() {
            assert_eq!(manager.push_wire_byte(byte, 5_103), None);
        }
        assert_eq!(manager.samples(), [None; 4]);
        assert_eq!(manager.stats().unsolicited_frames, 1);
    }

    #[test]
    fn mismatched_ack_cannot_relabel_a_response() {
        let mut manager = EscManager::new(EscManagerConfig::legacy_uart(), 0);
        let request = manager.next_request(5_000).unwrap();
        assert!(manager.mark_request_queued(request, 5_000));
        let wrong = EscActuatorRequest {
            sequence: request.sequence.wrapping_add(1),
            ..request
        };
        assert_eq!(
            manager.on_actuator_ack(EscActuatorAck {
                request: wrong,
                started_at_ms: 5_002,
            }),
            EscAckOutcome::Rejected
        );

        for byte in frame() {
            assert_eq!(manager.push_wire_byte(byte, 5_003), None);
        }
        assert_eq!(manager.samples(), [None; 4]);
        assert_eq!(manager.stats().mismatched_acks, 1);
    }

    #[test]
    fn frame_seen_during_preemption_waits_for_matching_ack() {
        let mut manager = EscManager::new(EscManagerConfig::legacy_uart(), 0);
        let request = manager.next_request(5_000).unwrap();
        assert!(manager.mark_request_queued(request, 5_000));

        for byte in frame() {
            assert_eq!(manager.push_wire_byte(byte, 5_003), None);
        }
        assert_eq!(manager.samples(), [None; 4]);

        let outcome = manager.on_actuator_ack(EscActuatorAck {
            request,
            started_at_ms: 5_002,
        });
        assert!(matches!(outcome, EscAckOutcome::Sample(_)));
        assert_eq!(manager.samples()[0].unwrap().sample.voltage_cv, 1_660);
        assert_eq!(manager.stats().unsolicited_frames, 0);
    }

    #[test]
    fn idle_qualification_requires_fresh_consecutive_samples_from_every_output() {
        let config = idle_qualification_config();
        let mut qualification = EscIdleQualification::new(config, 1_000);
        let outputs = [
            EscOutput::Output1,
            EscOutput::Output2,
            EscOutput::Output3,
            EscOutput::Output4,
        ];

        for round in 0..3 {
            for (index, output) in outputs.into_iter().enumerate() {
                let now_ms = 1_250 + round * 60 + index as u32 * 10;
                let status = qualification.observe(
                    telemetry_update(output, 70, now_ms, round * 4 + index as u32),
                    now_ms,
                );
                if round == 2 && index == 3 {
                    assert_eq!(status, EscIdleQualificationStatus::Qualified);
                } else {
                    assert_eq!(status, EscIdleQualificationStatus::Pending);
                }
            }
        }
    }

    #[test]
    fn zero_erpm_never_qualifies_and_times_out() {
        let config = idle_qualification_config();
        let mut qualification = EscIdleQualification::new(config, 1_000);
        for sequence in 0..12 {
            let now_ms = 1_250 + sequence * 70;
            let output = match sequence & 3 {
                0 => EscOutput::Output1,
                1 => EscOutput::Output2,
                2 => EscOutput::Output3,
                _ => EscOutput::Output4,
            };
            assert_eq!(
                qualification.observe(telemetry_update(output, 0, now_ms, sequence), now_ms),
                EscIdleQualificationStatus::Pending
            );
        }
        assert_eq!(
            qualification.status(2_200),
            EscIdleQualificationStatus::Failed(EscIdleQualificationFailure::Timeout {
                consecutive_samples: [0; 4],
            })
        );
    }

    #[test]
    fn idle_overspeed_fails_immediately_after_spinup_grace() {
        let config = idle_qualification_config();
        let mut qualification = EscIdleQualification::new(config, 1_000);
        assert_eq!(
            qualification.observe(telemetry_update(EscOutput::Output3, 101, 1_250, 0), 1_250),
            EscIdleQualificationStatus::Failed(EscIdleQualificationFailure::Overspeed {
                output: EscOutput::Output3,
                erpm_div100: 101,
            })
        );
    }

    #[test]
    fn stale_output_sample_prevents_idle_qualification() {
        let mut config = idle_qualification_config();
        config.required_consecutive_samples = 1;
        let mut qualification = EscIdleQualification::new(config, 1_000);
        for (index, output) in [
            EscOutput::Output1,
            EscOutput::Output2,
            EscOutput::Output3,
            EscOutput::Output4,
        ]
        .into_iter()
        .enumerate()
        {
            let now_ms = 1_250 + index as u32 * 80;
            let _ =
                qualification.observe(telemetry_update(output, 70, now_ms, index as u32), now_ms);
        }

        // Output 1 is older than the 200 ms freshness bound by the time output 4 arrives.
        assert_eq!(
            qualification.status(1_490),
            EscIdleQualificationStatus::Pending
        );
        assert_eq!(qualification.consecutive_samples()[0], 0);
    }
}
