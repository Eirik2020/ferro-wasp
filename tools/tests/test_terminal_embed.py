import unittest
from pathlib import Path

from tools.terminal_embed import (
    FIRMWARE_TARGETS,
    FoxeerSmokeEvidence,
    add_required_feature,
    commands_from_args,
    is_probe_flash_activity_line,
    is_probe_programming_complete_line,
    is_probe_run_command,
)


class FoxeerSmokeEvidenceTests(unittest.TestCase):
    def test_smoke_diagnostic_feature_is_added_once(self) -> None:
        self.assertEqual(add_required_feature("", "imu_transport_rtt"), "imu_transport_rtt")
        self.assertEqual(
            add_required_feature("usb_serial,imu_transport_rtt", "imu_transport_rtt"),
            "usb_serial imu_transport_rtt",
        )

    def test_probe_flash_milestones_are_classified(self) -> None:
        self.assertTrue(is_probe_run_command(["probe-rs", "run", "firmware.elf"]))
        self.assertTrue(is_probe_run_command(["probe-rs.exe", "run", "firmware.elf"]))
        self.assertFalse(is_probe_run_command(["probe-rs", "info"]))
        self.assertTrue(is_probe_flash_activity_line("Programming 100%"))
        self.assertTrue(is_probe_flash_activity_line("Erasing sectors"))
        self.assertFalse(is_probe_flash_activity_line("Connecting to target"))
        self.assertTrue(is_probe_programming_complete_line("Finished in 6.34s"))
        self.assertFalse(is_probe_programming_complete_line("Finished release profile"))

    def test_probe_command_keeps_flags_and_elf_as_separate_arguments(self) -> None:
        _, commands = commands_from_args(
            [],
            target=FIRMWARE_TARGETS["foxeer-f405-v2"],
            release=True,
            locked=True,
            features="",
            no_default_features=False,
            probe_speed_khz=100,
            connect_under_reset=True,
        )

        command = commands[0]
        timestamps = command.index("--no-timestamps")
        self.assertEqual(Path(command[timestamps + 1]).name, "FerroWaspFoxeerF405V2")
        self.assertIn("--connect-under-reset", command)
        self.assertEqual(command[command.index("--speed") + 1], "100")

    def test_accepts_boot_inhibit_imu_and_two_one_khz_intervals(self) -> None:
        evidence = FoxeerSmokeEvidence()
        for line in (
            "[INFO ] Foxeer F405 V2 system init successful",
            "[INFO ] FerroWasp RTT hello from Foxeer",
            "[INFO ] Foxeer ICM42688-P ready; WHO_AM_I 71",
            "[WARN ] Flight arming inhibited: Foxeer smoke-test actuator lockout is active",
            "[INFO ] IMU DRDY IRQ 2040, delta 2001, rejected 0, delta 0, last 1 us",
            "[INFO ] IMU DRDY IRQ 4040, delta 2000, rejected 0, delta 0, last 1 us",
        ):
            evidence.observe(line)

        self.assertEqual(evidence.failures(), [])

    def test_reports_missing_progress_and_fatal_output(self) -> None:
        evidence = FoxeerSmokeEvidence()
        evidence.observe("[ERROR ] panicked at SPI initialization")

        failures = evidence.failures()
        self.assertIn("supported IMU did not become ready", failures)
        self.assertTrue(any("fatal firmware output" in failure for failure in failures))


if __name__ == "__main__":
    unittest.main()
