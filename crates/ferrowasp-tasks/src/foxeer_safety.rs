//! HAL-independent Foxeer RC-link and safety-master policy.
//!
//! The state machine owns SBUS parsing, link qualification, the arming state,
//! and the temporary actuator permit. Callers may deliver observations and
//! carry its typed actuator requests, but cannot mutate those decisions.

use ferrowasp_core::safety::{
    ARM_THRESHOLD, ActuatorAuthority, ActuatorCmd, ActuatorGuardReport, ActuatorPreparationReport,
    ArmQualifier, ArmingAbortReason, ArmingState, PreArmHealth, PreArmHealthReport,
    RcLinkInvalidation, RcLinkState, RcLinkStatus, SafetyEvent, classify_rc_frame_flags,
    validate_arming_guard, validate_prearm_health,
};
use ferrowasp_drivers::serial_consumer::{SbusConsumer, SbusConsumerEvent};
use ferrowasp_io_core::serial::RcInputSnapshot;

use crate::drone_toolbox::remap_rc_throttle_channel;

/// Zero-based SBUS channel carrying the golden arm switch.
pub const FOXEER_ARM_CHANNEL_INDEX: usize = 8;

/// One bounded set of effects emitted by a safety-master transition.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SafetyMasterOutput {
    /// A safety-qualified RC snapshot to deliver to control.
    pub control_snapshot: Option<RcInputSnapshot>,
    /// Latest healthy channel-three observation for non-authoritative display.
    pub channel3: Option<u16>,
    /// Command requested from the separately safety-owned actuator task.
    pub actuator: Option<ActuatorCmd>,
    /// Safety transition or diagnostic reason produced by this input.
    pub event: Option<SafetyEvent>,
}

impl SafetyMasterOutput {
    const fn none() -> Self {
        Self {
            control_snapshot: None,
            channel3: None,
            actuator: None,
            event: None,
        }
    }
}

/// Exclusive persistent state for the golden RC and arming policy.
pub struct FoxeerSafetyMaster {
    parser: SbusConsumer,
    policy: SafetyPolicy,
    actuator_guard_sequence: u32,
}

struct SafetyPolicy {
    link: RcLinkState,
    arm_qualifier: ArmQualifier,
    arm_state: ArmingState,
    actuator_permit: bool,
    output_enabled: bool,
    arm_high: bool,
    throttle: u32,
    health: Option<PreArmHealthReport>,
}

impl FoxeerSafetyMaster {
    /// Creates the reviewed generated-build state, with physical output
    /// permanently inhibited.
    pub const fn output_inhibited() -> Self {
        Self::new(false)
    }

    /// Creates a state machine with an explicit actuator-output policy.
    ///
    /// Production generation uses [`Self::output_inhibited`]. Enabling this is
    /// useful only for pure state-machine verification until a later checklist
    /// checkpoint deliberately supplies the physical actuator implementation.
    pub const fn new(output_enabled: bool) -> Self {
        Self {
            parser: SbusConsumer::new(),
            policy: SafetyPolicy::new(output_enabled),
            actuator_guard_sequence: 0,
        }
    }

    /// Current safety-owned arming state.
    pub const fn arm_state(&self) -> ArmingState {
        self.policy.arm_state
    }

    /// Whether the short-lived actuator preparation permit is active.
    pub const fn actuator_permit(&self) -> bool {
        self.policy.actuator_permit
    }

    /// Current qualified RC-link status.
    pub fn link_status(&self, now_us: u32) -> RcLinkStatus {
        self.policy.link.status(now_us)
    }

    /// Retains the newest control-owned IMU health evidence and fails closed if
    /// active arming state loses a prerequisite.
    pub fn observe_control_health(
        &mut self,
        report: PreArmHealthReport,
        now_us: u32,
    ) -> SafetyMasterOutput {
        self.policy.observe_control_health(report, now_us)
    }

    /// Drops any incomplete SBUS frame and invalidates the authoritative link.
    pub fn transport_fault(&mut self, reason: RcLinkInvalidation) -> SafetyMasterOutput {
        self.parser.reset_parser();
        self.policy.invalidate_link(reason)
    }

    /// Applies the periodic freshness policy while a UART read is pending.
    pub fn poll(&mut self, now_us: u32) -> SafetyMasterOutput {
        self.policy.poll(now_us)
    }

    /// Consumes one bounded UART chunk and emits every parser transition.
    pub fn consume_bytes(
        &mut self,
        bytes: &[u8],
        now_us: u32,
        mut emit: impl FnMut(SafetyMasterOutput),
    ) {
        let Self { parser, policy, .. } = self;
        parser.consume_events(bytes, |event| {
            let output = match event {
                SbusConsumerEvent::ParserError(_) => {
                    policy.invalidate_link(RcLinkInvalidation::ParserError)
                }
                SbusConsumerEvent::Frame(snapshot) => policy.observe_snapshot(snapshot, now_us),
            };
            if output != SafetyMasterOutput::none() {
                emit(output);
            }
        });
    }

    /// Processes one classified snapshot. This is public to make every fault
    /// and recovery transition directly testable without UART hardware.
    pub fn observe_snapshot(
        &mut self,
        snapshot: RcInputSnapshot,
        now_us: u32,
    ) -> SafetyMasterOutput {
        self.policy.observe_snapshot(snapshot, now_us)
    }

    /// Completes actuator preparation using a fresh revalidation of every
    /// arming guard. Only the safety-owned actuator completion path may call
    /// this transition in a future output-enabled composition.
    pub fn actuator_idling(&mut self, now_us: u32) -> SafetyMasterOutput {
        self.policy.actuator_idling(now_us)
    }

    /// Applies completion or failure evidence from the safety-owned actuator.
    pub fn actuator_report(
        &mut self,
        report: ActuatorPreparationReport,
        now_us: u32,
    ) -> SafetyMasterOutput {
        match report {
            ActuatorPreparationReport::Qualified => self.policy.actuator_idling(now_us),
            ActuatorPreparationReport::Aborted(reason) => self.policy.abort_arming(reason),
            ActuatorPreparationReport::Faulted => self
                .policy
                .abort_arming(ArmingAbortReason::CompletionDeliveryFailed),
        }
    }

    /// Publishes the current safety-owned authority without transferring ownership.
    pub fn actuator_guard_report(&mut self, now_us: u32) -> ActuatorGuardReport {
        self.actuator_guard_sequence = self.actuator_guard_sequence.wrapping_add(1);
        let authority = if !self.policy.output_enabled {
            ActuatorAuthority::Inhibited
        } else {
            match self.policy.arm_state {
                ArmingState::Arming if self.policy.actuator_permit => ActuatorAuthority::Preparing,
                ArmingState::Armed => ActuatorAuthority::Armed,
                ArmingState::Disarmed | ArmingState::Arming => ActuatorAuthority::Inhibited,
            }
        };
        ActuatorGuardReport {
            authority,
            sequence: self.actuator_guard_sequence,
            observed_at_us: now_us,
        }
    }
}

impl SafetyPolicy {
    const fn new(output_enabled: bool) -> Self {
        Self {
            link: RcLinkState::new(),
            arm_qualifier: ArmQualifier::new(),
            arm_state: ArmingState::Disarmed,
            actuator_permit: false,
            output_enabled,
            arm_high: false,
            throttle: 0,
            health: None,
        }
    }

    fn observe_control_health(
        &mut self,
        report: PreArmHealthReport,
        now_us: u32,
    ) -> SafetyMasterOutput {
        self.health = Some(report);
        if self.arm_state == ArmingState::Disarmed {
            return SafetyMasterOutput::none();
        }
        if let Err(reason) = validate_prearm_health(report.health_at(now_us)) {
            return self.abort_arming(reason);
        }
        SafetyMasterOutput::none()
    }

    fn poll(&mut self, now_us: u32) -> SafetyMasterOutput {
        if self.link.status(now_us).timed_out {
            return self.invalidate_link(RcLinkInvalidation::Timeout);
        }
        if self.arm_state != ArmingState::Disarmed {
            if !self.link.status(now_us).valid || !self.arm_high {
                return self.abort_arming(ArmingAbortReason::RcLinkInvalid);
            }
            if let Err(reason) = validate_prearm_health(self.current_health(now_us)) {
                return self.abort_arming(reason);
            }
        }
        SafetyMasterOutput::none()
    }

    fn observe_snapshot(&mut self, snapshot: RcInputSnapshot, now_us: u32) -> SafetyMasterOutput {
        if let Err(reason) = classify_rc_frame_flags(snapshot.failsafe, snapshot.frame_lost) {
            return self.invalidate_link(reason);
        }
        if !snapshot.has_valid_frame {
            return self.invalidate_link(RcLinkInvalidation::ParserError);
        }

        self.arm_high = snapshot.channels[FOXEER_ARM_CHANNEL_INDEX] > ARM_THRESHOLD;
        self.throttle = remap_rc_throttle_channel(snapshot.channels[2]);
        let status = self.link.observe_healthy_frame(now_us, self.arm_high);
        let mut output = SafetyMasterOutput {
            control_snapshot: status.valid.then_some(snapshot),
            channel3: Some(snapshot.channels[2]),
            actuator: None,
            event: None,
        };

        if !status.valid || (self.arm_high && !status.armable) {
            self.arm_qualifier.reset();
            return output;
        }

        let Some(event) = self.arm_qualifier.update(self.arm_high, now_us) else {
            return output;
        };
        match event {
            SafetyEvent::ArmRequested => {
                self.arm_state = ArmingState::Disarmed;
                self.actuator_permit = false;
                let guard = validate_arming_guard(
                    self.output_enabled,
                    status.armable,
                    self.arm_high,
                    self.throttle,
                )
                .and_then(|()| validate_prearm_health(self.current_health(now_us)));
                match guard {
                    Ok(()) => {
                        self.actuator_permit = true;
                        self.arm_state = ArmingState::Arming;
                        output.actuator = Some(ActuatorCmd::EnterIdle);
                        output.event = Some(SafetyEvent::ArmRequested);
                    }
                    Err(reason) => {
                        output.actuator = Some(ActuatorCmd::Disarm);
                        output.event = Some(SafetyEvent::ArmingAborted(reason));
                    }
                }
            }
            SafetyEvent::DisarmRequested => {
                self.arm_state = ArmingState::Disarmed;
                self.actuator_permit = false;
                output.actuator = Some(ActuatorCmd::Disarm);
                output.event = Some(SafetyEvent::DisarmRequested);
            }
            SafetyEvent::ActuatorIdling
            | SafetyEvent::ArmingAborted(_)
            | SafetyEvent::RcLinkInvalid(_) => {}
        }
        output
    }

    fn actuator_idling(&mut self, now_us: u32) -> SafetyMasterOutput {
        if self.arm_state != ArmingState::Arming {
            return self.abort_arming(ArmingAbortReason::CompletionDeliveryFailed);
        }
        let link = self.link.status(now_us);
        let guard = validate_arming_guard(
            self.actuator_permit,
            link.armable,
            self.arm_high,
            self.throttle,
        )
        .and_then(|()| validate_prearm_health(self.current_health(now_us)));
        if let Err(reason) = guard {
            return self.abort_arming(reason);
        }

        self.actuator_permit = false;
        self.arm_state = ArmingState::Armed;
        SafetyMasterOutput {
            event: Some(SafetyEvent::ActuatorIdling),
            ..SafetyMasterOutput::none()
        }
    }

    fn current_health(&self, now_us: u32) -> PreArmHealth {
        self.health.map_or(
            PreArmHealth {
                imu_ready: false,
                imu_bias_calibrated: false,
                imu_fresh: false,
            },
            |report| report.health_at(now_us),
        )
    }

    fn abort_arming(&mut self, reason: ArmingAbortReason) -> SafetyMasterOutput {
        self.arm_state = ArmingState::Disarmed;
        self.actuator_permit = false;
        SafetyMasterOutput {
            actuator: Some(ActuatorCmd::Disarm),
            event: Some(SafetyEvent::ArmingAborted(reason)),
            ..SafetyMasterOutput::none()
        }
    }

    fn invalidate_link(&mut self, reason: RcLinkInvalidation) -> SafetyMasterOutput {
        if !self.link.invalidate(reason) {
            return SafetyMasterOutput::none();
        }
        self.arm_qualifier.reset();
        self.arm_state = ArmingState::Disarmed;
        self.actuator_permit = false;
        self.arm_high = false;
        self.throttle = 0;
        SafetyMasterOutput {
            control_snapshot: Some(RcInputSnapshot::new()),
            channel3: None,
            actuator: Some(ActuatorCmd::Disarm),
            event: Some(SafetyEvent::RcLinkInvalid(reason)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(value: u16, arm_high: bool) -> RcInputSnapshot {
        let mut snapshot = RcInputSnapshot::new();
        let mut channels = [value; 16];
        channels[2] = 192;
        channels[FOXEER_ARM_CHANNEL_INDEX] = if arm_high { 1_600 } else { 1_000 };
        snapshot.record_frame(channels, false, false, false, false);
        snapshot
    }

    fn healthy_report(now_us: u32) -> PreArmHealthReport {
        PreArmHealthReport {
            health: PreArmHealth {
                imu_ready: true,
                imu_bias_calibrated: true,
                imu_fresh: true,
            },
            sequence: 1,
            observed_at_us: now_us,
        }
    }

    fn qualify_low(master: &mut FoxeerSafetyMaster) {
        master.observe_snapshot(frame(900, false), 1_000);
        master.observe_snapshot(frame(900, false), 2_000);
        let recovered = master.observe_snapshot(frame(900, false), 3_000);
        assert!(recovered.control_snapshot.is_some());
        assert!(master.link_status(3_000).armable);
    }

    #[test]
    fn output_inhibited_build_never_leaves_disarmed() {
        let mut master = FoxeerSafetyMaster::output_inhibited();
        qualify_low(&mut master);
        master.observe_snapshot(frame(900, true), 4_000);
        master.observe_snapshot(frame(900, true), 104_000);
        master.observe_control_health(healthy_report(204_000), 204_000);
        let output = master.observe_snapshot(frame(900, true), 204_000);

        assert_eq!(master.arm_state(), ArmingState::Disarmed);
        assert!(!master.actuator_permit());
        assert_eq!(output.actuator, Some(ActuatorCmd::Disarm));
        assert_eq!(
            output.event,
            Some(SafetyEvent::ArmingAborted(ArmingAbortReason::PermitRevoked))
        );
    }

    #[test]
    fn healthy_low_to_high_hold_can_only_create_a_temporary_permit() {
        let mut master = FoxeerSafetyMaster::new(true);
        qualify_low(&mut master);
        master.observe_snapshot(frame(900, true), 4_000);
        master.observe_snapshot(frame(900, true), 104_000);
        master.observe_control_health(healthy_report(204_000), 204_000);
        let request = master.observe_snapshot(frame(900, true), 204_000);

        assert_eq!(request.event, Some(SafetyEvent::ArmRequested));
        assert_eq!(request.actuator, Some(ActuatorCmd::EnterIdle));
        assert_eq!(master.arm_state(), ArmingState::Arming);
        assert!(master.actuator_permit());

        let completed = master.actuator_idling(204_001);
        assert_eq!(completed.event, Some(SafetyEvent::ActuatorIdling));
        assert_eq!(master.arm_state(), ArmingState::Armed);
        assert!(!master.actuator_permit());
    }

    #[test]
    fn actuator_reports_cannot_bypass_safety_owned_state() {
        let mut master = FoxeerSafetyMaster::new(true);
        assert_eq!(
            master.actuator_guard_report(1).authority,
            ActuatorAuthority::Inhibited
        );
        let unsolicited = master.actuator_report(ActuatorPreparationReport::Qualified, 2);
        assert_eq!(master.arm_state(), ArmingState::Disarmed);
        assert_eq!(unsolicited.actuator, Some(ActuatorCmd::Disarm));

        qualify_low(&mut master);
        master.observe_snapshot(frame(900, true), 4_000);
        master.observe_snapshot(frame(900, true), 104_000);
        master.observe_control_health(healthy_report(204_000), 204_000);
        master.observe_snapshot(frame(900, true), 204_000);
        assert_eq!(
            master.actuator_guard_report(204_001).authority,
            ActuatorAuthority::Preparing
        );
        master.actuator_report(
            ActuatorPreparationReport::Aborted(ArmingAbortReason::EscIdleTelemetryTimeout),
            204_002,
        );
        assert_eq!(master.arm_state(), ArmingState::Disarmed);
        assert_eq!(
            master.actuator_guard_report(204_003).authority,
            ActuatorAuthority::Inhibited
        );
    }

    #[test]
    fn arm_high_during_recovery_cannot_rearm_without_fresh_low_then_high() {
        let mut master = FoxeerSafetyMaster::new(true);
        master.observe_snapshot(frame(900, true), 1_000);
        master.observe_snapshot(frame(900, true), 2_000);
        master.observe_snapshot(frame(900, true), 3_000);
        master.observe_snapshot(frame(900, true), 103_000);
        master.observe_control_health(healthy_report(203_000), 203_000);
        let held_high = master.observe_snapshot(frame(900, true), 203_000);
        assert_eq!(held_high.event, None);
        assert!(!master.link_status(203_000).armable);

        master.observe_snapshot(frame(900, false), 204_000);
        master.observe_snapshot(frame(900, true), 205_000);
        master.observe_snapshot(frame(900, true), 305_000);
        master.observe_control_health(healthy_report(405_000), 405_000);
        let fresh_hold = master.observe_snapshot(frame(900, true), 405_000);
        assert_eq!(fresh_hold.event, Some(SafetyEvent::ArmRequested));
    }

    #[test]
    fn failsafe_precedes_frame_lost_and_timeout_fails_closed() {
        let mut master = FoxeerSafetyMaster::new(true);
        qualify_low(&mut master);
        let mut invalid = frame(900, false);
        invalid.frame_lost = true;
        invalid.failsafe = true;
        let output = master.observe_snapshot(invalid, 4_000);
        assert_eq!(
            output.event,
            Some(SafetyEvent::RcLinkInvalid(RcLinkInvalidation::SbusFailsafe))
        );

        qualify_low(&mut master);
        let timeout = master.poll(103_001);
        assert_eq!(
            timeout.event,
            Some(SafetyEvent::RcLinkInvalid(RcLinkInvalidation::Timeout))
        );
    }

    #[test]
    fn parser_and_transport_faults_neutralize_control_input() {
        let mut master = FoxeerSafetyMaster::new(true);
        qualify_low(&mut master);
        let mut parser_error = frame(900, false);
        parser_error.has_valid_frame = false;
        let parser = master.observe_snapshot(parser_error, 4_000);
        assert_eq!(
            parser.event,
            Some(SafetyEvent::RcLinkInvalid(RcLinkInvalidation::ParserError))
        );
        assert_eq!(parser.control_snapshot, Some(RcInputSnapshot::new()));

        qualify_low(&mut master);
        let transport = master.transport_fault(RcLinkInvalidation::DmaError);
        assert_eq!(
            transport.event,
            Some(SafetyEvent::RcLinkInvalid(RcLinkInvalidation::DmaError))
        );
        assert_eq!(master.arm_state(), ArmingState::Disarmed);
    }

    #[test]
    fn prearm_health_guard_order_is_readiness_then_bias_then_freshness() {
        let cases = [
            (
                PreArmHealth {
                    imu_ready: false,
                    imu_bias_calibrated: false,
                    imu_fresh: false,
                },
                ArmingAbortReason::ImuUnavailable,
            ),
            (
                PreArmHealth {
                    imu_ready: true,
                    imu_bias_calibrated: false,
                    imu_fresh: false,
                },
                ArmingAbortReason::ImuBiasUncalibrated,
            ),
            (
                PreArmHealth {
                    imu_ready: true,
                    imu_bias_calibrated: true,
                    imu_fresh: false,
                },
                ArmingAbortReason::ImuStale,
            ),
        ];

        for (health, expected) in cases {
            let mut master = FoxeerSafetyMaster::new(true);
            qualify_low(&mut master);
            master.observe_snapshot(frame(900, true), 4_000);
            master.observe_snapshot(frame(900, true), 104_000);
            master.observe_control_health(
                PreArmHealthReport {
                    health,
                    sequence: 1,
                    observed_at_us: 204_000,
                },
                204_000,
            );
            let output = master.observe_snapshot(frame(900, true), 204_000);
            assert_eq!(output.event, Some(SafetyEvent::ArmingAborted(expected)));
        }
    }

    #[test]
    fn stale_health_report_is_rejected_even_when_reported_fresh() {
        let mut master = FoxeerSafetyMaster::new(true);
        qualify_low(&mut master);
        master.observe_snapshot(frame(900, true), 4_000);
        master.observe_snapshot(frame(900, true), 104_000);
        master.observe_control_health(healthy_report(100_000), 100_000);
        let output = master.observe_snapshot(frame(900, true), 204_000);
        assert_eq!(
            output.event,
            Some(SafetyEvent::ArmingAborted(ArmingAbortReason::ImuStale))
        );
    }
}
