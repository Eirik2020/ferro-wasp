#![allow(dead_code)]

pub const ARM_HOLD_US: u64 = 200_000;
pub const ARM_THRESHOLD: u16 = 1500;
pub const RC_LINK_TIMEOUT_US: u32 = 100_000;
pub const RC_LINK_RECOVERY_FRAMES: u8 = 3;

pub const ZERO_THROTTLE: f32 = 0.0;
pub const ARM_IDLE_THROTTLE: f32 = 300.0;

pub const ESC_LOW_THROTTLE: f32 = 0.0;
// Default RC-PWM idle. Protocol/board-specific actuator paths may supply a
// different validated floor without changing this legacy PWM behavior.
pub const ESC_IDLE_THROTTLE: f32 = 65.0;
pub const ARMING_MAX_THROTTLE: u32 = 65;
pub const ESC_ARM_THROTTLE: f32 = 900.0;

pub const BLHELI_ARM_LOW_HOLD_MS: u32 = 2_500;
pub const BLHELI_ARM_IDLE_HOLD_MS: u32 = 500;
pub const ESC_LOW_HOLD_MS: u32 = BLHELI_ARM_LOW_HOLD_MS;
pub const ESC_IDLE_HOLD_MS: u32 = BLHELI_ARM_IDLE_HOLD_MS;
pub const ESC_MAX_THROTTLE: f32 = 2000.0;

pub const PWM_CAL_MAX_HOLD_MS: u32 = 10_000;
pub const PWM_CAL_LOW_HOLD_MS: u32 = 10_000;
pub const PWM_CAL_MOTOR: usize = 1;

pub const MOTOR_CMD_QUEUE_CAP: usize = 4;
pub const MOTOR_CMD_MAX_AGE_MS: u32 = 20;

#[cfg(test)]
extern crate std;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SafetyEvent {
    ArmRequested,
    // Legacy name: protocol-specific actuator preparation completed. PWM has
    // completed its idle sequence; DShot has qualified all four ESC idle RPMs.
    ActuatorIdling,
    ArmingAborted(ArmingAbortReason),
    DisarmRequested,
    RcLinkInvalid(RcLinkInvalidation),
}

#[derive(Clone, Copy, PartialEq)]
pub enum ActuatorCmd {
    // Protocol-specific arming preparation. The actuator owner decides whether
    // this means a PWM idle sequence or guarded DShot idle-RPM qualification.
    EnterIdle,
    Disarm,
    Calibrate,

    // Wake actuator_output and make it consume the latest SPSC motor command.
    ApplyLatestThrottle,

    #[cfg(any(
        feature = "bench_motor1_only",
        feature = "bench_motor2_only",
        feature = "bench_motor3_only",
        feature = "bench_motor4_only",
        feature = "bench_logical_motor1_only",
        feature = "bench_logical_motor2_only",
        feature = "bench_logical_motor3_only",
        feature = "bench_logical_motor4_only",
        feature = "bench_dshot_unequal_motors"
    ))]
    ApplyBenchSelectedMotor,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ArmingState {
    Disarmed,
    Arming,
    Armed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ArmingAbortReason {
    PermitRevoked,
    RcLinkInvalid,
    ArmSwitchLow,
    ThrottleHigh,
    ImuUnavailable,
    ImuBiasUncalibrated,
    ImuStale,
    EscIdleTelemetryTimeout,
    EscIdleRpmOutOfRange,
    EscIdleQualificationInvalid,
    CompletionDeliveryFailed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PreArmHealth {
    pub imu_ready: bool,
    pub imu_bias_calibrated: bool,
    pub imu_fresh: bool,
}

pub const fn validate_prearm_health(health: PreArmHealth) -> Result<(), ArmingAbortReason> {
    if !health.imu_ready {
        Err(ArmingAbortReason::ImuUnavailable)
    } else if !health.imu_bias_calibrated {
        Err(ArmingAbortReason::ImuBiasUncalibrated)
    } else if !health.imu_fresh {
        Err(ArmingAbortReason::ImuStale)
    } else {
        Ok(())
    }
}

pub const fn validate_arming_guard(
    permit: bool,
    rc_link_armable: bool,
    arm_high: bool,
    throttle: u32,
) -> Result<(), ArmingAbortReason> {
    if !permit {
        Err(ArmingAbortReason::PermitRevoked)
    } else if !rc_link_armable {
        Err(ArmingAbortReason::RcLinkInvalid)
    } else if !arm_high {
        Err(ArmingAbortReason::ArmSwitchLow)
    } else if throttle > ARMING_MAX_THROTTLE {
        Err(ArmingAbortReason::ThrottleHigh)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RcRates {
    pub roll: i16,
    pub pitch: i16,
    pub yaw: i16,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RcLinkInvalidation {
    Startup,
    TransportDiscontinuity,
    DmaError,
    ParserError,
    SbusFrameLost,
    SbusFailsafe,
    Timeout,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RcLinkStatus {
    pub valid: bool,
    pub timed_out: bool,
    pub armable: bool,
    pub last_healthy_frame_us: u32,
    pub consecutive_healthy_frames: u8,
    pub invalidation_sequence: u32,
    pub last_invalidation: RcLinkInvalidation,
}

#[derive(Debug, Clone, Copy)]
pub struct RcLinkState {
    valid: bool,
    rearm_allowed: bool,
    last_healthy_frame_us: u32,
    consecutive_healthy_frames: u8,
    invalidation_sequence: u32,
    last_invalidation: RcLinkInvalidation,
}

impl RcLinkState {
    pub const fn new() -> Self {
        Self {
            valid: false,
            rearm_allowed: false,
            last_healthy_frame_us: 0,
            consecutive_healthy_frames: 0,
            invalidation_sequence: 0,
            last_invalidation: RcLinkInvalidation::Startup,
        }
    }

    pub fn observe_healthy_frame(&mut self, now_us: u32, arm_high: bool) -> RcLinkStatus {
        if self.valid && now_us.wrapping_sub(self.last_healthy_frame_us) > RC_LINK_TIMEOUT_US {
            let _ = self.invalidate(RcLinkInvalidation::Timeout);
        }

        self.last_healthy_frame_us = now_us;
        self.consecutive_healthy_frames = self.consecutive_healthy_frames.saturating_add(1);
        if self.consecutive_healthy_frames >= RC_LINK_RECOVERY_FRAMES {
            self.valid = true;
        }
        // Do not let a transient low arm value seen while the receiver link is
        // still qualifying satisfy the boot/reconnect rearm interlock. The
        // switch must be observed low on a link that has reached valid state.
        if self.valid && !arm_high {
            self.rearm_allowed = true;
        }

        self.status(now_us)
    }

    pub fn invalidate(&mut self, reason: RcLinkInvalidation) -> bool {
        let changed = self.valid
            || self.rearm_allowed
            || self.consecutive_healthy_frames != 0
            || self.last_invalidation != reason;
        self.valid = false;
        self.rearm_allowed = false;
        self.consecutive_healthy_frames = 0;
        if changed {
            self.invalidation_sequence = self.invalidation_sequence.wrapping_add(1);
        }
        self.last_invalidation = reason;
        changed
    }

    pub fn status(&self, now_us: u32) -> RcLinkStatus {
        let timed_out =
            self.valid && now_us.wrapping_sub(self.last_healthy_frame_us) > RC_LINK_TIMEOUT_US;
        let valid = self.valid && !timed_out;
        RcLinkStatus {
            valid,
            timed_out,
            armable: valid && self.rearm_allowed,
            last_healthy_frame_us: self.last_healthy_frame_us,
            consecutive_healthy_frames: self.consecutive_healthy_frames,
            invalidation_sequence: self.invalidation_sequence,
            last_invalidation: self.last_invalidation,
        }
    }
}

impl Default for RcLinkState {
    fn default() -> Self {
        Self::new()
    }
}

pub const fn classify_rc_frame_flags(
    failsafe: bool,
    frame_lost: bool,
) -> Result<(), RcLinkInvalidation> {
    if failsafe {
        Err(RcLinkInvalidation::SbusFailsafe)
    } else if frame_lost {
        Err(RcLinkInvalidation::SbusFrameLost)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MotorCmd {
    pub motors: [f32; 4],
    pub seq: u32,
    pub issued_at_ms: u32,
}

impl MotorCmd {
    pub fn is_fresh(&self, now_ms: u32, max_age_ms: u32) -> bool {
        now_ms.wrapping_sub(self.issued_at_ms) <= max_age_ms
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MotorCmdReadError {
    Missing,
    Stale { seq: u32, age_ms: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotorOutputValidationError {
    NonFinite,
    InvalidIdleThrottle,
}

pub fn validate_active_motor_outputs(
    motors: [f32; 4],
) -> Result<[f32; 4], MotorOutputValidationError> {
    validate_active_motor_outputs_with_idle(motors, ESC_IDLE_THROTTLE)
}

pub fn validate_active_motor_outputs_with_idle(
    mut motors: [f32; 4],
    idle_throttle: f32,
) -> Result<[f32; 4], MotorOutputValidationError> {
    if !idle_throttle.is_finite()
        || idle_throttle <= ESC_LOW_THROTTLE
        || idle_throttle > ESC_MAX_THROTTLE
    {
        return Err(MotorOutputValidationError::InvalidIdleThrottle);
    }

    for motor in motors.iter_mut() {
        if !motor.is_finite() {
            return Err(MotorOutputValidationError::NonFinite);
        }

        *motor = motor.clamp(idle_throttle, ESC_MAX_THROTTLE);
    }

    Ok(motors)
}

pub mod signals {
    use super::{
        MOTOR_CMD_QUEUE_CAP, MotorCmd, MotorCmdReadError, RcLinkInvalidation, RcLinkState,
        RcLinkStatus, RcRates,
    };
    use core::cell::RefCell;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use critical_section::Mutex;
    use heapless::spsc::{Consumer, Producer, Queue};

    pub type MotorCmdQueue = Queue<MotorCmd, MOTOR_CMD_QUEUE_CAP>;
    pub type MotorCmdProducer = Producer<'static, MotorCmd>;
    pub type MotorCmdConsumer = Consumer<'static, MotorCmd>;

    pub struct RcArmHighWriter(&'static AtomicBool);
    pub struct RcThrottleWriter(&'static AtomicU32);
    pub struct SafetyArmWriter(&'static AtomicBool);
    pub struct ActuatorArmPermitWriter(&'static AtomicBool);
    pub struct ActuatorArmDoneWriter(&'static AtomicBool);
    pub struct RcRatesWriter(&'static Mutex<RefCell<RcRates>>);
    pub struct RcLinkFrameWriter(&'static Mutex<RefCell<RcLinkState>>);
    pub struct RcLinkInvalidator(&'static Mutex<RefCell<RcLinkState>>);

    pub struct MotorCmdWriter(MotorCmdProducer);
    pub struct MotorCmdReader(MotorCmdConsumer);

    #[derive(Clone, Copy)]
    pub struct RcArmHighReader(&'static AtomicBool);
    #[derive(Clone, Copy)]
    pub struct RcThrottleReader(&'static AtomicU32);
    #[derive(Clone, Copy)]
    pub struct SafetyArmReader(&'static AtomicBool);
    #[derive(Clone, Copy)]
    pub struct ActuatorArmPermitReader(&'static AtomicBool);
    #[derive(Clone, Copy)]
    pub struct ActuatorArmDoneReader(&'static AtomicBool);
    #[derive(Clone, Copy)]
    pub struct RcRatesReader(&'static Mutex<RefCell<RcRates>>);
    #[derive(Clone, Copy)]
    pub struct RcLinkReader(&'static Mutex<RefCell<RcLinkState>>);

    pub fn split_rc_arm_high(value: &'static AtomicBool) -> (RcArmHighWriter, RcArmHighReader) {
        (RcArmHighWriter(value), RcArmHighReader(value))
    }

    pub fn split_rc_throttle(value: &'static AtomicU32) -> (RcThrottleWriter, RcThrottleReader) {
        (RcThrottleWriter(value), RcThrottleReader(value))
    }

    pub fn split_safety_arm(value: &'static AtomicBool) -> (SafetyArmWriter, SafetyArmReader) {
        (SafetyArmWriter(value), SafetyArmReader(value))
    }

    pub fn split_actuator_arm_permit(
        value: &'static AtomicBool,
    ) -> (ActuatorArmPermitWriter, ActuatorArmPermitReader) {
        (
            ActuatorArmPermitWriter(value),
            ActuatorArmPermitReader(value),
        )
    }

    pub fn split_actuator_arm_done(
        value: &'static AtomicBool,
    ) -> (ActuatorArmDoneWriter, ActuatorArmDoneReader) {
        (ActuatorArmDoneWriter(value), ActuatorArmDoneReader(value))
    }

    pub fn split_rc_rates(
        value: &'static Mutex<RefCell<RcRates>>,
    ) -> (RcRatesWriter, RcRatesReader) {
        (RcRatesWriter(value), RcRatesReader(value))
    }

    pub fn split_rc_link(
        value: &'static Mutex<RefCell<RcLinkState>>,
    ) -> (RcLinkFrameWriter, RcLinkInvalidator, RcLinkReader) {
        (
            RcLinkFrameWriter(value),
            RcLinkInvalidator(value),
            RcLinkReader(value),
        )
    }

    pub fn split_motor_cmd_queue(
        queue: &'static mut MotorCmdQueue,
    ) -> (MotorCmdWriter, MotorCmdReader) {
        let (producer, consumer) = queue.split();
        (MotorCmdWriter(producer), MotorCmdReader(consumer))
    }

    impl RcArmHighWriter {
        pub fn write(&self, value: bool) {
            self.0.store(value, Ordering::Release);
        }
    }

    impl RcArmHighReader {
        pub fn read(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    impl RcThrottleWriter {
        pub fn write(&self, value: u32) {
            self.0.store(value, Ordering::Release);
        }
    }

    impl RcThrottleReader {
        pub fn read(&self) -> u32 {
            self.0.load(Ordering::Acquire)
        }
    }

    impl SafetyArmWriter {
        pub fn write(&self, value: bool) {
            self.0.store(value, Ordering::Release);
        }

        pub fn arm(&self) {
            self.write(true);
        }

        pub fn disarm(&self) {
            self.write(false);
        }
    }

    impl SafetyArmReader {
        pub fn read(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    impl ActuatorArmPermitWriter {
        pub fn write(&self, value: bool) {
            self.0.store(value, Ordering::Release);
        }

        pub fn is_allowed(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }

        pub fn allow(&self) {
            self.write(true);
        }

        pub fn revoke(&self) -> bool {
            self.0.swap(false, Ordering::AcqRel)
        }
    }

    impl ActuatorArmPermitReader {
        pub fn read(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    impl ActuatorArmDoneWriter {
        pub fn write(&self, value: bool) {
            self.0.store(value, Ordering::Release);
        }

        pub fn clear(&self) {
            self.write(false);
        }

        pub fn set_done(&self) {
            self.write(true);
        }
    }

    impl ActuatorArmDoneReader {
        pub fn read(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    impl RcRatesWriter {
        pub fn write(&self, value: RcRates) {
            critical_section::with(|cs| {
                *self.0.borrow_ref_mut(cs) = value;
            });
        }
    }

    impl RcRatesReader {
        pub fn read(&self) -> RcRates {
            critical_section::with(|cs| *self.0.borrow_ref(cs))
        }
    }

    impl RcLinkFrameWriter {
        pub fn observe_healthy_frame(&self, now_us: u32, arm_high: bool) -> RcLinkStatus {
            critical_section::with(|cs| {
                self.0
                    .borrow_ref_mut(cs)
                    .observe_healthy_frame(now_us, arm_high)
            })
        }
    }

    impl RcLinkInvalidator {
        pub fn invalidate(&self, reason: RcLinkInvalidation) -> bool {
            critical_section::with(|cs| self.0.borrow_ref_mut(cs).invalidate(reason))
        }
    }

    impl RcLinkReader {
        pub fn status(&self, now_us: u32) -> RcLinkStatus {
            critical_section::with(|cs| self.0.borrow_ref(cs).status(now_us))
        }

        pub fn is_valid(&self, now_us: u32) -> bool {
            self.status(now_us).valid
        }

        pub fn is_armable(&self, now_us: u32) -> bool {
            self.status(now_us).armable
        }
    }

    impl MotorCmdWriter {
        pub fn enqueue(&mut self, cmd: MotorCmd) -> Result<(), MotorCmd> {
            self.0.enqueue(cmd)
        }
    }

    impl MotorCmdReader {
        pub fn dequeue(&mut self) -> Option<MotorCmd> {
            self.0.dequeue()
        }

        pub fn take_latest(&mut self) -> Option<MotorCmd> {
            let mut latest = None;

            while let Some(cmd) = self.0.dequeue() {
                latest = Some(cmd);
            }

            latest
        }

        pub fn discard_all(&mut self) {
            while self.0.dequeue().is_some() {}
        }

        pub fn take_latest_fresh(
            &mut self,
            now_ms: u32,
            max_age_ms: u32,
        ) -> Result<MotorCmd, MotorCmdReadError> {
            let command = self.take_latest().ok_or(MotorCmdReadError::Missing)?;
            let age_ms = now_ms.wrapping_sub(command.issued_at_ms);

            if command.is_fresh(now_ms, max_age_ms) {
                Ok(command)
            } else {
                Err(MotorCmdReadError::Stale {
                    seq: command.seq,
                    age_ms,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EdgeDetector {
    prev: bool,
}

impl EdgeDetector {
    pub fn new() -> Self {
        Self { prev: false }
    }

    pub fn update(&mut self, current: bool) -> bool {
        let rising = !self.prev && current;
        self.prev = current;
        rising
    }
}

impl Default for EdgeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct ArmQualifier {
    was_high: bool,
    candidate_start_us: Option<u64>,
    request_sent: bool,
}

impl ArmQualifier {
    pub fn reset(&mut self) {
        self.was_high = false;
        self.candidate_start_us = None;
        self.request_sent = false;
    }

    pub fn update(&mut self, arm_high: bool, now_us: u32) -> Option<SafetyEvent> {
        if !arm_high {
            let was_high = self.was_high;

            self.was_high = false;
            self.candidate_start_us = None;
            self.request_sent = false;

            return was_high.then_some(SafetyEvent::DisarmRequested);
        }

        if !self.was_high {
            self.was_high = true;
            self.candidate_start_us = Some(now_us as u64);
            self.request_sent = false;
            return None;
        }

        if let Some(start) = self.candidate_start_us
            && !self.request_sent
            && (now_us as u64).saturating_sub(start) >= ARM_HOLD_US
        {
            self.request_sent = true;
            return Some(SafetyEvent::ArmRequested);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motor_command_freshness_handles_normal_and_wrapping_time() {
        let cmd = MotorCmd {
            motors: [1.0, 2.0, 3.0, 4.0],
            seq: 7,
            issued_at_ms: 100,
        };

        assert!(cmd.is_fresh(119, MOTOR_CMD_MAX_AGE_MS));
        assert!(cmd.is_fresh(120, MOTOR_CMD_MAX_AGE_MS));
        assert!(!cmd.is_fresh(121, MOTOR_CMD_MAX_AGE_MS));

        let wrapped = MotorCmd {
            issued_at_ms: u32::MAX - 5,
            ..cmd
        };
        assert!(wrapped.is_fresh(4, 10));
        assert!(!wrapped.is_fresh(6, 10));
    }

    #[test]
    fn rc_link_requires_healthy_recovery_and_arm_switch_low() {
        let mut link = RcLinkState::new();

        assert!(!link.observe_healthy_frame(1_000, true).valid);
        assert!(!link.observe_healthy_frame(2_000, true).valid);
        let recovered = link.observe_healthy_frame(3_000, true);
        assert!(recovered.valid);
        assert!(!recovered.armable);

        let armable = link.observe_healthy_frame(4_000, false);
        assert!(armable.valid);
        assert!(armable.armable);
    }

    #[test]
    fn rc_link_ignores_arm_low_transients_before_recovery_completes() {
        let mut link = RcLinkState::new();

        assert!(!link.observe_healthy_frame(1_000, false).valid);
        assert!(!link.observe_healthy_frame(2_000, false).valid);

        let recovered_high = link.observe_healthy_frame(3_000, true);
        assert!(recovered_high.valid);
        assert!(!recovered_high.armable);

        let observed_low_while_valid = link.observe_healthy_frame(4_000, false);
        assert!(observed_low_while_valid.armable);
    }

    #[test]
    fn rc_link_invalidation_requires_fresh_frames_and_new_low_switch_observation() {
        let mut link = RcLinkState::new();
        link.observe_healthy_frame(1_000, false);
        link.observe_healthy_frame(2_000, false);
        assert!(link.observe_healthy_frame(3_000, false).armable);

        assert!(link.invalidate(RcLinkInvalidation::TransportDiscontinuity));
        assert!(!link.invalidate(RcLinkInvalidation::TransportDiscontinuity));
        let invalid = link.status(3_001);
        assert!(!invalid.valid);
        assert!(!invalid.armable);
        assert_eq!(invalid.invalidation_sequence, 1);

        link.observe_healthy_frame(4_000, true);
        link.observe_healthy_frame(5_000, true);
        let recovered_high = link.observe_healthy_frame(6_000, true);
        assert!(recovered_high.valid);
        assert!(!recovered_high.armable);
        assert!(link.observe_healthy_frame(7_000, false).armable);
    }

    #[test]
    fn rc_link_timeout_is_wrap_safe_and_forces_requalification() {
        let mut link = RcLinkState::new();
        let start = u32::MAX - 20_000;
        link.observe_healthy_frame(start.wrapping_sub(2), false);
        link.observe_healthy_frame(start.wrapping_sub(1), false);
        link.observe_healthy_frame(start, false);

        assert!(link.status(start.wrapping_add(RC_LINK_TIMEOUT_US)).valid);
        let expired = link.status(start.wrapping_add(RC_LINK_TIMEOUT_US + 1));
        assert!(!expired.valid);
        assert!(expired.timed_out);

        let first_after_timeout =
            link.observe_healthy_frame(start.wrapping_add(RC_LINK_TIMEOUT_US + 2), true);
        assert!(!first_after_timeout.valid);
        assert_eq!(
            first_after_timeout.last_invalidation,
            RcLinkInvalidation::Timeout
        );
    }

    #[test]
    fn rc_frame_flags_reject_failsafe_and_lost_frames() {
        assert_eq!(classify_rc_frame_flags(false, false), Ok(()));
        assert_eq!(
            classify_rc_frame_flags(false, true),
            Err(RcLinkInvalidation::SbusFrameLost)
        );
        assert_eq!(
            classify_rc_frame_flags(true, false),
            Err(RcLinkInvalidation::SbusFailsafe)
        );
        assert_eq!(
            classify_rc_frame_flags(true, true),
            Err(RcLinkInvalidation::SbusFailsafe)
        );
    }

    #[test]
    fn arm_qualifier_debounces_arm_and_emits_disarm_on_falling_edge() {
        let mut qualifier = ArmQualifier::default();

        assert_eq!(qualifier.update(true, 1_000), None);
        assert_eq!(qualifier.update(true, 1_000 + ARM_HOLD_US as u32 - 1), None);
        assert_eq!(
            qualifier.update(true, 1_000 + ARM_HOLD_US as u32),
            Some(SafetyEvent::ArmRequested)
        );
        assert_eq!(
            qualifier.update(true, 1_000 + ARM_HOLD_US as u32 + 10_000),
            None
        );
        assert_eq!(
            qualifier.update(false, 500_000),
            Some(SafetyEvent::DisarmRequested)
        );
        assert_eq!(qualifier.update(false, 501_000), None);
    }

    #[test]
    fn arming_guard_requires_permission_link_arm_switch_and_low_throttle() {
        assert_eq!(
            validate_arming_guard(true, true, true, ARMING_MAX_THROTTLE),
            Ok(())
        );
        assert_eq!(
            validate_arming_guard(false, true, true, 0),
            Err(ArmingAbortReason::PermitRevoked)
        );
        assert_eq!(
            validate_arming_guard(true, false, true, 0),
            Err(ArmingAbortReason::RcLinkInvalid)
        );
        assert_eq!(
            validate_arming_guard(true, true, false, 0),
            Err(ArmingAbortReason::ArmSwitchLow)
        );
        assert_eq!(
            validate_arming_guard(true, true, true, ARMING_MAX_THROTTLE + 1),
            Err(ArmingAbortReason::ThrottleHigh)
        );
    }

    #[test]
    fn prearm_health_requires_ready_calibrated_fresh_imu() {
        let healthy = PreArmHealth {
            imu_ready: true,
            imu_bias_calibrated: true,
            imu_fresh: true,
        };
        assert_eq!(validate_prearm_health(healthy), Ok(()));

        assert_eq!(
            validate_prearm_health(PreArmHealth {
                imu_ready: false,
                ..healthy
            }),
            Err(ArmingAbortReason::ImuUnavailable)
        );
        assert_eq!(
            validate_prearm_health(PreArmHealth {
                imu_bias_calibrated: false,
                ..healthy
            }),
            Err(ArmingAbortReason::ImuBiasUncalibrated)
        );
        assert_eq!(
            validate_prearm_health(PreArmHealth {
                imu_fresh: false,
                ..healthy
            }),
            Err(ArmingAbortReason::ImuStale)
        );
    }

    #[test]
    fn motor_command_reader_takes_latest_and_drops_stale_queue_entries() {
        let queue = std::boxed::Box::leak(std::boxed::Box::new(signals::MotorCmdQueue::new()));
        let (mut writer, mut reader) = signals::split_motor_cmd_queue(queue);

        writer
            .enqueue(MotorCmd {
                motors: [1.0; 4],
                seq: 1,
                issued_at_ms: 10,
            })
            .unwrap();
        writer
            .enqueue(MotorCmd {
                motors: [2.0; 4],
                seq: 2,
                issued_at_ms: 20,
            })
            .unwrap();
        writer
            .enqueue(MotorCmd {
                motors: [3.0; 4],
                seq: 3,
                issued_at_ms: 30,
            })
            .unwrap();

        let latest = reader.take_latest().unwrap();
        assert_eq!(latest.seq, 3);
        assert_eq!(latest.motors, [3.0; 4]);
        assert!(reader.dequeue().is_none());

        writer
            .enqueue(MotorCmd {
                motors: [4.0; 4],
                seq: 4,
                issued_at_ms: 40,
            })
            .unwrap();
        reader.discard_all();
        assert!(reader.dequeue().is_none());
    }

    #[test]
    fn motor_command_reader_rejects_missing_and_expired_latest_commands() {
        let queue = std::boxed::Box::leak(std::boxed::Box::new(signals::MotorCmdQueue::new()));
        let (mut writer, mut reader) = signals::split_motor_cmd_queue(queue);

        assert!(matches!(
            reader.take_latest_fresh(100, MOTOR_CMD_MAX_AGE_MS),
            Err(MotorCmdReadError::Missing)
        ));

        writer
            .enqueue(MotorCmd {
                motors: [100.0; 4],
                seq: 9,
                issued_at_ms: 100,
            })
            .unwrap();

        assert!(matches!(
            reader.take_latest_fresh(121, MOTOR_CMD_MAX_AGE_MS),
            Err(MotorCmdReadError::Stale { seq: 9, age_ms: 21 })
        ));
        assert!(reader.dequeue().is_none());
    }

    #[test]
    fn active_motor_output_validation_floors_and_clamps_finite_values() {
        assert_eq!(
            validate_active_motor_outputs([0.0, ESC_IDLE_THROTTLE, 1000.0, 3000.0]).unwrap(),
            [
                ESC_IDLE_THROTTLE,
                ESC_IDLE_THROTTLE,
                1000.0,
                ESC_MAX_THROTTLE
            ]
        );
    }

    #[test]
    fn active_motor_output_validation_rejects_non_finite_values() {
        assert_eq!(
            validate_active_motor_outputs([1.0, f32::NAN, 2.0, 3.0]),
            Err(MotorOutputValidationError::NonFinite)
        );
    }

    #[test]
    fn active_motor_output_validation_accepts_a_protocol_specific_idle() {
        assert_eq!(
            validate_active_motor_outputs_with_idle([0.0, 65.0, 1000.0, 3000.0], 80.0).unwrap(),
            [80.0, 80.0, 1000.0, ESC_MAX_THROTTLE]
        );
    }

    #[test]
    fn active_motor_output_validation_rejects_invalid_idle_configuration() {
        for invalid in [f32::NAN, ESC_LOW_THROTTLE, -1.0, ESC_MAX_THROTTLE + 1.0] {
            assert_eq!(
                validate_active_motor_outputs_with_idle([100.0; 4], invalid),
                Err(MotorOutputValidationError::InvalidIdleThrottle)
            );
        }
    }
}
