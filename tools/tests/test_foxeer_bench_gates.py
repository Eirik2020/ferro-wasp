import unittest
from pathlib import Path

from tools.test_evidence import ARTIFACT_ID
from tools.foxeer_bench_gates import (
    REVIEWED_BASELINE,
    TEMPORARY_ROLL,
    BootIdleEvidence,
    REPO_ROOT,
    artifact_id,
    check_usb_snapshots,
    meaningful_note,
    repository_relative,
    parse_snapshot,
)

BASELINE_SHOW = """schema_version = 2
imu_lpf_hz = 50.8346
log_rate_divisor = 2
rc_deadband = 8
roll_center_rate = 70.0
roll_max_rate = 300.0
roll_expo = 0.5
pitch_center_rate = 70.0
pitch_max_rate = 300.0
pitch_expo = 0.5
yaw_center_rate = 70.0
yaw_max_rate = 200.0
yaw_expo = 0.5

[roll]
p = 2.5
i = 0.0
d = 0.0

[pitch]
p = 2.5
i = 0.0
d = 0.0

[yaw]
p = 2.0
i = 0.0
d = 0.0
# imu_lpf_hz 50.8346 is a one-pole alpha of 0.148 at the controller's 2000 Hz loop rate.
"""

CANDIDATE_ELF = "cd873d6b5bb3f88a4388e1e7ebd8a2ab10d721a13454c58b551c1f96f1c2bb6a"

HEALTHY_BOOT = [
    f"HOST: firmware SHA-256 {CANDIDATE_ELF.upper()}",
    "DRONE: [INFO ] Foxeer F405 V2 system init successful",
    "DRONE: [INFO ] Foxeer ICM42688-P ready; WHO_AM_I 71",
    "DRONE: [WARN ] IMU stale in control loop: seq 0, stale ticks 1",
    "DRONE: [INFO ] Running heartbeat!",
    "DRONE: [INFO ] Foxeer DShot values [0, 0, 0, 0], sets 999/1000, lanes [999, 999, 999, 999], "
    "busy 0, expired 0, timeouts 0, faults 0, at 1999 ms",
    "DRONE: [WARN ] Foxeer ESC telemetry manager latched fault after response timeout for physical "
    "output 1 (logical M1), request 0",
    "DRONE: [INFO ] Foxeer DShot values [0, 0, 0, 0], sets 1999/2000, lanes [1999, 1999, 1999, 1999], "
    "busy 0, expired 0, timeouts 0, faults 0, at 3999 ms",
    "DRONE:  WARN probe_rs::probe::stlink: send_jtag_command 242 failed: SwdDpError",
]


def snapshot(text: str, exit_code: int | None = None):
    prefix = "" if exit_code is None else f"exit {exit_code}\n"
    return parse_snapshot(prefix + text)


def with_values(text: str, **values: float) -> str:
    lines = []
    for line in text.splitlines():
        key = line.split(" = ")[0]
        lines.append(f"{key} = {values[key]}" if key in values else line)
    return "\n".join(lines) + "\n"


def usb_run(**overrides):
    temporary = with_values(BASELINE_SHOW, **TEMPORARY_ROLL)
    snapshots = {
        "initial": snapshot(BASELINE_SHOW),
        "reject": snapshot(BASELINE_SHOW, exit_code=2),
        "same_boot": snapshot(temporary),
        "cold_boot_temporary": snapshot(temporary),
        "restored": snapshot(BASELINE_SHOW),
        "restored_cold_boot": snapshot(BASELINE_SHOW),
    }
    snapshots.update(overrides)
    return snapshots


class UsbConfigGateTests(unittest.TestCase):
    def test_snapshots_flatten_to_firmware_key_names(self) -> None:
        exit_code, values = snapshot(BASELINE_SHOW, exit_code=2)
        self.assertEqual(exit_code, 2)
        self.assertEqual(values["roll_p"], 2.5)
        self.assertEqual(values["log_rate_divisor"], 2)
        self.assertTrue(REVIEWED_BASELINE.keys() <= values.keys())

    def test_a_complete_healthy_run_passes(self) -> None:
        self.assertEqual(check_usb_snapshots(usb_run()), [])

    def test_an_accepted_invalid_value_fails(self) -> None:
        failures = check_usb_snapshots(usb_run(reject=snapshot(BASELINE_SHOW, exit_code=0)))
        self.assertIn("reject: the out-of-range value was accepted", failures)

    def test_a_profile_lost_across_the_cold_boot_fails(self) -> None:
        failures = check_usb_snapshots(usb_run(cold_boot_temporary=snapshot(BASELINE_SHOW)))
        self.assertTrue(any(f.startswith("cold_boot_temporary: roll_expo") for f in failures), failures)

    def test_an_initial_profile_off_the_reviewed_baseline_fails(self) -> None:
        withdrawn = with_values(BASELINE_SHOW, **{"p": 3.0})
        failures = check_usb_snapshots(usb_run(initial=snapshot(withdrawn)))
        self.assertTrue(any(f.startswith("initial: roll_p") for f in failures), failures)

    def test_a_changed_value_outside_the_baseline_still_fails(self) -> None:
        drifted = with_values(BASELINE_SHOW, imu_lpf_hz=80.0)
        failures = check_usb_snapshots(usb_run(restored_cold_boot=snapshot(drifted)))
        self.assertTrue(any("imu_lpf_hz" in f for f in failures), failures)

    def test_a_missing_snapshot_fails(self) -> None:
        run = usb_run()
        del run["restored"]
        self.assertIn("restored: snapshot missing", check_usb_snapshots(run))


class BootIdleGateTests(unittest.TestCase):
    def evidence(self, lines):
        evidence = BootIdleEvidence()
        for line in lines:
            evidence.observe(line)
        return evidence

    def test_a_healthy_boot_passes_and_probe_warnings_are_ignored(self) -> None:
        evidence = self.evidence(HEALTHY_BOOT)
        self.assertEqual(evidence.failures(CANDIDATE_ELF, minimum_reports=2), [])
        self.assertEqual(evidence.expected_warnings, 2)

    def test_another_image_fails(self) -> None:
        failures = self.evidence(HEALTHY_BOOT).failures("0" * 64, minimum_reports=2)
        self.assertTrue(any("is not the candidate's" in f for f in failures))

    def test_any_output_off_stop_fails(self) -> None:
        lines = HEALTHY_BOOT + [
            "DRONE: [INFO ] Foxeer DShot values [0, 48, 0, 0], sets 2999/3000, lanes [2999, 2999, 2999, "
            "2999], busy 0, expired 0, timeouts 0, faults 0, at 5999 ms"
        ]
        failures = self.evidence(lines).failures(CANDIDATE_ELF, minimum_reports=2)
        self.assertTrue(any(f.startswith("DShot not at stop") for f in failures))

    def test_a_dshot_fault_fails(self) -> None:
        lines = HEALTHY_BOOT + [
            "DRONE: [INFO ] Foxeer DShot values [0, 0, 0, 0], sets 2999/3000, lanes [2999, 2999, 2999, "
            "2999], busy 0, expired 0, timeouts 0, faults 1, at 5999 ms"
        ]
        failures = self.evidence(lines).failures(CANDIDATE_ELF, minimum_reports=2)
        self.assertTrue(any(f.startswith("DShot not at stop") for f in failures))

    def test_arming_fails(self) -> None:
        failures = self.evidence(HEALTHY_BOOT + ["DRONE: [INFO ] SYSTEM ARMED"]).failures(CANDIDATE_ELF, 2)
        self.assertIn("the system armed", failures)

    def test_an_unexpected_warning_fails(self) -> None:
        lines = HEALTHY_BOOT + ["DRONE: [WARN ] UART4 RX free-buffer pool exhausted on IDLE"]
        failures = self.evidence(lines).failures(CANDIDATE_ELF, minimum_reports=2)
        self.assertIn("unexpected warning: UART4 RX free-buffer pool exhausted on IDLE", failures)

    def test_a_short_capture_fails(self) -> None:
        failures = self.evidence(HEALTHY_BOOT).failures(CANDIDATE_ELF, minimum_reports=5)
        self.assertIn("2 DShot reports, fewer than 5", failures)


class RecordTests(unittest.TestCase):
    def test_artifact_ids_satisfy_the_evidence_validator(self) -> None:
        for name in ("1-initial.txt", "20260930_005150_rtt.log", "boot-2.log", "foxeer-baseline.toml"):
            identifier = artifact_id(Path(name))
            self.assertRegex(identifier, ARTIFACT_ID, name)
        self.assertEqual(artifact_id(Path("1-initial.txt")), "capture-1-initial")

    def test_commands_are_recorded_without_the_checkout_path(self) -> None:
        inside = f"{REPO_ROOT}/logs/bench/run/foxeer-baseline.toml"
        self.assertEqual(repository_relative(inside), "logs/bench/run/foxeer-baseline.toml")
        self.assertEqual(repository_relative("roll-expo"), "roll-expo")

    def test_saying_there_is_nothing_is_not_a_note(self) -> None:
        for answer in ("", "none", "None.", "no", "nothing abnormal", "ok"):
            self.assertEqual(meaningful_note(answer), "", answer)
        self.assertEqual(meaningful_note("warm ESC 3"), "warm ESC 3")


if __name__ == "__main__":
    unittest.main()
