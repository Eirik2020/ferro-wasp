import unittest

from tools.usb_bench import (
    BOARDS,
    config_value,
    control_rate,
    dfu_serial,
    fallback_state,
    idle_failures,
    parse_args,
    parse_status,
    port_table,
    split_lines,
)


def status(**overrides: str) -> dict[str, str]:
    fields = {
        "ms": "10000",
        "ready": "1",
        "seq": "20000",
        "stale": "0",
        "ctl": "20000",
        "ctl_hz": "2000",
        "armed": "0",
        "vbat_stale": "0",
    }
    return fields | overrides


def later(**overrides: str) -> dict[str, str]:
    return status(ms="12000", seq="24000", ctl="24000") | overrides


class ReadingTheBoard(unittest.TestCase):
    def test_a_status_line_inside_an_answer_is_split_out(self) -> None:
        self.assertEqual(
            split_lines("OK u=0 n=1 t=9 w=1FWDBG1 ms=44 armed=0\r\n"),
            ["OK u=0 n=1 t=9 w=1", "FWDBG1 ms=44 armed=0"],
        )

    def test_a_status_line_parses_and_an_answer_does_not(self) -> None:
        parsed = parse_status("FWDBG1 ms=44 imu=icm42688p armed=0 vbat_stale=1")
        self.assertEqual(parsed, {"ms": "44", "imu": "icm42688p", "armed": "0", "vbat_stale": "1"})
        self.assertIsNone(parse_status("OK build rev=abc"))

    def test_answers_read_back(self) -> None:
        self.assertEqual(fallback_state("OK config fallback=damaged; a saved slot is torn"), "damaged")
        self.assertEqual(fallback_state("OK config fallback=none"), "none")
        self.assertIsNone(fallback_state("ERR storage unavailable"))
        self.assertEqual(config_value("OK roll_p=2.5000", "roll_p"), 2.5)
        self.assertIsNone(config_value("OK pitch_p=2.5000", "roll_p"))

    def test_a_dfu_device_is_taken_only_by_its_own_flash_layout(self) -> None:
        listing = (
            'Found DFU: [0483:df11] ver=2200, devnum=4, cfg=1, intf=0, path="3-1", alt=0, '
            'name="@Internal Flash  /0x08000000/04*016Kg,01*064Kg,07*128Kg", serial="FOXEER1"\n'
            'Found DFU: [0483:df11] ver=0200, devnum=5, cfg=1, intf=0, path="3-2", alt=0, '
            'name="@Internal Flash   /0x08000000/16*128Kg", serial="LUCID1"\n'
        )
        self.assertEqual(dfu_serial(listing, BOARDS["foxeer-f405-v2"].dfu_layout), "FOXEER1")
        self.assertEqual(dfu_serial(listing, BOARDS["tbs-lucid-h7"].dfu_layout), "LUCID1")
        self.assertIsNone(dfu_serial("", BOARDS["foxeer-f405-v2"].dfu_layout))

    def test_the_port_table_reads_from_the_configurators_json(self) -> None:
        document = {"result": {"ports": [{"port": "uart1", "function": "esc_telemetry"}]}}
        self.assertEqual(port_table(document), {"uart1": "esc_telemetry"})


class JudgingIdle(unittest.TestCase):
    def test_a_healthy_idle_passes_and_measures_the_loop(self) -> None:
        self.assertEqual(idle_failures([status(), later()]), [])
        self.assertEqual(control_rate([status(), later()]), 2000.0)

    def test_each_fault_is_named(self) -> None:
        for fault, expected in (
            ({"armed": "1"}, "armed"),
            ({"ready": "0"}, "IMU was not ready"),
            ({"stale": "1"}, "IMU reported stale"),
            ({"vbat_stale": "1"}, "battery reading"),
            ({"seq": "20000"}, "IMU sequence"),
            ({"ctl": "22000"}, "control loop at 1000 Hz"),
            ({"ms": "9000"}, "uptime went backwards"),
        ):
            with self.subTest(fault=fault):
                self.assertTrue(
                    any(expected in problem for problem in idle_failures([status(), later(**fault)])),
                    idle_failures([status(), later(**fault)]),
                )

    def test_a_firmware_without_the_battery_field_does_not_pass(self) -> None:
        old = later()
        del old["vbat_stale"]
        self.assertTrue(idle_failures([status(), old]))

    def test_too_few_lines_do_not_pass(self) -> None:
        self.assertTrue(idle_failures([status()]))


class TheCommandLine(unittest.TestCase):
    def test_a_board_is_required_and_known(self) -> None:
        self.assertEqual(parse_args(["session", "--board", "tbs-lucid-h7"]).board, "tbs-lucid-h7")
        with self.assertRaises(SystemExit):
            parse_args(["session"])


if __name__ == "__main__":
    unittest.main()
