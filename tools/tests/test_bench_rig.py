import tempfile
import unittest
from pathlib import Path

from tools.bench_rig import (
    BOARDS,
    BootEvidence,
    Leds,
    artifact_id,
    led_failures,
    read_capture,
    run_components,
)
from tools.test_evidence import ARTIFACT_ID


LUCID = BOARDS["tbs-lucid-h7"]


def dshot(at_ms: int, values: str = "0, 0, 0, 0", faults: int = 0) -> str:
    return (
        f"[INFO ] Foxeer DShot values [{values}], sets 999/1000, lanes [999, 999, 999, 999], "
        f"busy 0, expired 0, timeouts 0, faults {faults}, at {at_ms} ms"
    )


HEALTHY_RESET = [
    "[INFO ] Begin system init..",
    "[INFO ] TBS Lucid H7 system init successful",
    "[INFO ] TBS Lucid H7 MPU-6000 ready; WHO_AM_I 104",
    "[WARN ] Flight arming inhibited: TBS Lucid H7 IMU orientation, motor order, and ADC "
    "scale are not bench-verified",
    "[INFO ] Running heartbeat!",
    "[WARN ] IMU stale in control loop: seq 0, stale ticks 2",
    "[WARN ] Foxeer ESC telemetry stopped after response timeout for physical output 1 "
    "(logical M1), request 0; retrying while disarmed",
    *(dshot(1999 + 2000 * index) for index in range(6)),
]


def judge(lines: list[str], *, from_reset: bool = True, allow_no_storage: bool = False) -> list[str]:
    evidence = BootEvidence(LUCID, allow_no_storage=allow_no_storage)
    for line in lines:
        evidence.observe(line)
    return evidence.failures(minimum_reports=5, from_reset=from_reset)


class BootJudgementTests(unittest.TestCase):
    def test_a_healthy_boot_from_reset_passes(self) -> None:
        self.assertEqual(judge(HEALTHY_RESET), [])

    def test_a_boot_whose_imu_failed_fails(self) -> None:
        lines = [line for line in HEALTHY_RESET if "ready" not in line]
        lines.append("[WARN ] TBS Lucid H7 MPU-6000 configuration failed; WHO_AM_I 104, sampling disabled")

        problems = judge(lines)

        self.assertIn("the IMU never reported ready", problems)
        self.assertTrue(any("configuration failed" in problem for problem in problems))

    def test_any_output_off_stop_or_dshot_fault_fails(self) -> None:
        self.assertTrue(judge([*HEALTHY_RESET, dshot(15999, values="0, 48, 0, 0")]))
        self.assertTrue(judge([*HEALTHY_RESET, dshot(15999, faults=1)]))

    def test_arming_fails(self) -> None:
        self.assertIn("the system armed", judge([*HEALTHY_RESET, "[INFO ] SYSTEM ARMED"]))

    def test_missing_storage_needs_explicit_acceptance(self) -> None:
        lines = [
            *HEALTHY_RESET,
            "[WARN ] SD storage unavailable (Device(NotReady(Bus(Timeout)))); storage remains disabled",
        ]

        self.assertTrue(judge(lines))
        self.assertEqual(judge(lines, allow_no_storage=True), [])

    def test_a_power_cycle_capture_must_show_a_fresh_boot(self) -> None:
        fresh = [dshot(1999 + 2000 * index) for index in range(6)]
        stale = [dshot(174000 + 2000 * index) for index in range(6)]

        self.assertEqual(judge(fresh, from_reset=False), [])
        self.assertTrue(any("not a fresh boot" in problem for problem in judge(stale, from_reset=False)))

    def test_terminal_embed_prefixed_lines_are_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "boot.log"
            path.write_text("\n".join(f"DRONE: {line}" for line in HEALTHY_RESET) + "\n")

            evidence = read_capture(path, LUCID, allow_no_storage=False)

        self.assertEqual(evidence.imu, "MPU-6000")
        self.assertEqual(evidence.failures(minimum_reports=5, from_reset=True), [])


class LedTests(unittest.TestCase):
    LEDS = Leds(odr_address=0, heartbeat_bit=4, armed_bit=3, active_low=True)

    def test_a_toggling_heartbeat_with_a_dark_armed_led_passes(self) -> None:
        self.assertEqual(led_failures(self.LEDS, [0x18, 0x08, 0x18, 0x08]), [])

    def test_a_stuck_heartbeat_fails(self) -> None:
        self.assertTrue(led_failures(self.LEDS, [0x18, 0x18, 0x18]))

    def test_a_lit_armed_led_fails_while_disarmed(self) -> None:
        self.assertTrue(led_failures(self.LEDS, [0x10, 0x00, 0x10]))


class RecordTests(unittest.TestCase):
    def test_a_run_names_its_app_backend_and_the_imu_driver_it_loaded(self) -> None:
        names = run_components(LUCID, "MPU-6000")

        self.assertIn("app-tbs-lucid-h7", names)
        self.assertIn("stm32h7", names)
        self.assertIn("driver-mpu6000", names)
        self.assertNotIn("driver-icm42688p", names)

    def test_artifact_ids_satisfy_the_evidence_validator(self) -> None:
        for name in ("FerroWaspTbsLucidH7.elf", "boot-1-swd-reset.log", "boot-2-usb-power-cycle.log"):
            self.assertRegex(artifact_id(Path(name)), ARTIFACT_ID)


if __name__ == "__main__":
    unittest.main()
