import unittest
from pathlib import Path
from unittest import mock

from tools.foxeer_bench_gates import (
    REVIEWED_BASELINE,
    TEMPORARY_ROLL,
    REPO_ROOT,
    check_usb_snapshots,
    operator_notes,
    repository_relative,
    parse_snapshot,
)
from tools.reqtool_bridge import ARTIFACT_ID, artifact_id

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


class RecordTests(unittest.TestCase):
    def test_artifact_ids_satisfy_reqtool(self) -> None:
        for name in ("1-initial.txt", "20260930_005150_rtt.log", "boot-2.log", "foxeer-baseline.toml"):
            identifier = artifact_id(Path(name))
            self.assertRegex(identifier, ARTIFACT_ID, name)
        self.assertEqual(artifact_id(Path("1-initial.txt")), "capture-1-initial")

    def test_commands_are_recorded_without_the_checkout_path(self) -> None:
        inside = f"{REPO_ROOT}/logs/bench/run/foxeer-baseline.toml"
        self.assertEqual(repository_relative(inside), "logs/bench/run/foxeer-baseline.toml")
        self.assertEqual(repository_relative("roll-expo"), "roll-expo")

    def notes_for(self, *answers: str) -> str:
        with mock.patch("sys.stdin.isatty", return_value=True), mock.patch(
            "builtins.input", side_effect=answers
        ), mock.patch("builtins.print"):
            return operator_notes()

    def test_no_means_nothing_to_report(self) -> None:
        self.assertEqual(self.notes_for("no"), "")
        self.assertEqual(self.notes_for("N"), "")

    def test_yes_asks_for_a_description(self) -> None:
        self.assertEqual(self.notes_for("yes", "warm ESC 3"), "warm ESC 3")

    def test_an_unclear_answer_is_asked_again(self) -> None:
        self.assertEqual(self.notes_for("none", "", "no"), "")
        self.assertEqual(self.notes_for("maybe", "y", "", "smell near M2"), "smell near M2")

if __name__ == "__main__":
    unittest.main()
