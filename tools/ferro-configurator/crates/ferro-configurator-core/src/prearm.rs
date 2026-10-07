//! Why the controller will not arm, in words a pilot can act on.
//!
//! The firmware decides arming; this module only explains the status line it
//! already reports. It mirrors the order of `safety::validate_arming_guard`
//! and `validate_prearm_health` so the first failing row is the reason the
//! firmware would give. Conditions the status line does not carry (gyro bias
//! calibration, ESC idle eRPM qualification) are reported as `Unknown` rather
//! than guessed, so a fully green list never claims more than was observed.

use ferrowasp_core::safety::ARMING_MAX_THROTTLE;
use serde::Serialize;

use crate::client::StatusSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Pass,
    Fail,
    /// The firmware does not report this condition.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrearmCheck {
    /// Stable identifier for front-ends.
    pub id: &'static str,
    pub label: &'static str,
    pub state: CheckState,
    /// What to do about a failure, or why the state is unknown.
    pub hint: &'static str,
}

fn check(
    id: &'static str,
    label: &'static str,
    pass: Option<bool>,
    hint: &'static str,
) -> PrearmCheck {
    let state = match pass {
        Some(true) => CheckState::Pass,
        Some(false) => CheckState::Fail,
        None => CheckState::Unknown,
    };
    PrearmCheck {
        id,
        label,
        state,
        hint,
    }
}

/// The arming preconditions, in the order the firmware evaluates them:
/// `validate_arming_guard` (link, throttle), then `validate_prearm_health`
/// (IMU ready, bias calibrated, fresh), then the ESC idle qualification.
///
/// The arm switch itself is not a row: it is the pilot's request, not a
/// precondition, and a front-end shows it separately.
pub fn prearm_checks(status: &StatusSnapshot) -> Vec<PrearmCheck> {
    vec![
        check(
            "rc_link",
            "Receiver link",
            Some(status.rc_valid),
            "No valid receiver frames. Turn the radio on, check it is bound, and check the receiver wiring.",
        ),
        check(
            "rc_armable",
            "Arm switch reset",
            Some(status.armable),
            "After the link connects the arm switch must be seen off once. Flip it off, then on.",
        ),
        check(
            "throttle_low",
            "Throttle low",
            Some(status.throttle <= ARMING_MAX_THROTTLE),
            "Move the throttle stick fully down.",
        ),
        check(
            "imu_detected",
            "Gyro detected",
            Some(status.imu != "none"),
            "No IMU answered at boot. Check the board, or reflash the matching firmware.",
        ),
        check(
            "imu_ready",
            "Gyro running",
            Some(status.imu_ready),
            "The IMU was found but is not streaming. Power-cycle the controller.",
        ),
        check(
            "gyro_calibrated",
            "Gyro calibrated",
            None,
            "Not reported over USB yet. Keep the quad still for a few seconds after plugging in.",
        ),
        check(
            "imu_fresh",
            "Gyro data fresh",
            status.imu_stale.map(|stale| !stale),
            "IMU samples stopped arriving. Power-cycle; if it repeats, the board needs checking.",
        ),
        check(
            "battery",
            "Flight battery connected",
            Some(status.battery_decivolts > 0),
            "Running on USB power only. Connect a battery: arming waits for all four ESCs to report idle.",
        ),
        check(
            "esc_idle",
            "ESCs report idle",
            None,
            "Not reported over USB yet. The firmware checks idle eRPM from all four motors when you arm.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> StatusSnapshot {
        StatusSnapshot {
            uptime_ms: 1_000,
            control_sequence: 0,
            control_loop_hz: 1_000,
            imu: "icm42688p".to_owned(),
            imu_ready: true,
            rc_valid: true,
            armable: true,
            throttle: 0,
            arm_switch: false,
            armed: false,
            battery_decivolts: 168,
            imu_stale: Some(false),
            channels: None,
        }
    }

    fn state(checks: &[PrearmCheck], id: &str) -> CheckState {
        checks.iter().find(|c| c.id == id).expect("known id").state
    }

    #[test]
    fn checks_follow_the_firmware_guard_order() {
        let ids: Vec<_> = prearm_checks(&ready()).iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            [
                "rc_link",
                "rc_armable",
                "throttle_low",
                "imu_detected",
                "imu_ready",
                "gyro_calibrated",
                "imu_fresh",
                "battery",
                "esc_idle",
            ]
        );
    }

    #[test]
    fn a_ready_controller_fails_nothing() {
        let checks = prearm_checks(&ready());
        assert!(checks.iter().all(|c| c.state != CheckState::Fail));
    }

    #[test]
    fn unreported_conditions_are_unknown_not_passed() {
        let mut status = ready();
        status.imu_stale = None;
        let checks = prearm_checks(&status);
        assert_eq!(state(&checks, "imu_fresh"), CheckState::Unknown);
        assert_eq!(state(&checks, "gyro_calibrated"), CheckState::Unknown);
        assert_eq!(state(&checks, "esc_idle"), CheckState::Unknown);
    }

    #[test]
    fn throttle_limit_matches_the_firmware_guard() {
        let mut status = ready();
        status.throttle = ARMING_MAX_THROTTLE;
        assert_eq!(
            state(&prearm_checks(&status), "throttle_low"),
            CheckState::Pass
        );
        status.throttle = ARMING_MAX_THROTTLE + 1;
        assert_eq!(
            state(&prearm_checks(&status), "throttle_low"),
            CheckState::Fail
        );
    }

    #[test]
    fn each_failure_is_named() {
        let mut status = ready();
        status.imu = "none".to_owned();
        status.rc_valid = false;
        status.battery_decivolts = 0;
        let checks = prearm_checks(&status);
        assert_eq!(state(&checks, "imu_detected"), CheckState::Fail);
        assert_eq!(state(&checks, "rc_link"), CheckState::Fail);
        assert_eq!(state(&checks, "battery"), CheckState::Fail);
    }
}
