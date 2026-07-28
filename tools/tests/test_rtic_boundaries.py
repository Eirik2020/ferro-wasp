import tempfile
import unittest
from pathlib import Path

from tools.check_rtic_boundaries import validate_rtic_boundaries


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]


class RticBoundaryTests(unittest.TestCase):
    def test_current_repository_passes(self) -> None:
        self.assertEqual(validate_rtic_boundaries(REPOSITORY_ROOT), [])

    def test_rejects_non_rtic_main_declarations_and_external_imports(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            main = root / "apps" / "demo" / "src" / "main.rs"
            main.parent.mkdir(parents=True)
            main.write_text(
                """
use external_crate::Thing;

const LOCAL_LIMIT: usize = 4;
struct Helper;

fn helper() {}

#[rtic::app(device = pac)]
mod app {
    use super::*;

    struct Shared {}
    struct Local {}

    #[init]
    fn init(_: init::Context) -> (Shared, Local) {
        (Shared {}, Local {})
    }
}
""",
                encoding="utf-8",
            )

            errors = validate_rtic_boundaries(root)

        self.assertTrue(any("may import only" in error for error in errors))
        self.assertTrue(any("const declaration 'LOCAL_LIMIT'" in error for error in errors))
        self.assertTrue(any("struct 'Helper'" in error for error in errors))
        self.assertTrue(any("function 'helper'" in error for error in errors))

    def test_rejects_shared_board_types_register_logic_and_copies(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = root / "apps" / "one" / "src" / "board" / "routes.rs"
            second = root / "apps" / "two" / "src" / "board" / "routes.rs"
            first.parent.mkdir(parents=True)
            second.parent.mkdir(parents=True)
            copied = """
pub struct DmaRoute {
    pub controller: u8,
    pub stream: u8,
    pub channel: u8,
}

pub fn configure(timer: &Timer) {
    timer.bdtr().modify(|_, w| w.moe().set_bit());
}

pub const ROUTES: [DmaRoute; 2] = [
    DmaRoute { controller: 1, stream: 2, channel: 3 },
    DmaRoute { controller: 2, stream: 3, channel: 4 },
];
"""
            first.write_text(copied, encoding="utf-8")
            second.write_text(copied, encoding="utf-8")

            errors = validate_rtic_boundaries(root)

        self.assertTrue(any("shared declaration 'DmaRoute'" in error for error in errors))
        self.assertTrue(any("timer register sequencing" in error for error in errors))
        self.assertTrue(any("cross-board source files are identical" in error for error in errors))


    def test_rejects_optional_or_inline_foxeer_usb(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "apps" / "foxeer-f405-v2"
            source = app / "src"
            source.mkdir(parents=True)
            (app / "Cargo.toml").write_text(
                "[features]\nusb_serial = []\n", encoding="utf-8"
            )
            (source / "main.rs").write_text(
                """
#[cfg(feature = "usb_serial")]
fn init_usb() {
    let _usb = USB::new(resources, pins, clocks);
}
""",
                encoding="utf-8",
            )
            (source / "lib.rs").write_text("", encoding="utf-8")

            errors = validate_rtic_boundaries(root)

        self.assertTrue(any("must not be a Cargo feature" in error for error in errors))
        self.assertTrue(any("must not be feature-gated" in error for error in errors))
        self.assertTrue(any("USB construction belongs" in error for error in errors))
        self.assertTrue(any("must initialize mandatory USB" in error for error in errors))

    def test_rejects_optional_standard_flight_services_and_pwm_esc_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "apps" / "foxeer-f405-v2"
            source = app / "src"
            board = source / "board"
            board.mkdir(parents=True)
            (app / "Cargo.toml").write_text(
                "[features]\n"
                "board-foxeer-f405-v2 = []\n"
                "dshot = []\n"
                "esc_telemetry = []\n"
                "flash_storage = []\n"
                "flash_blackbox = []\n",
                encoding="utf-8",
            )
            (source / "main.rs").write_text(
                "#[cfg(feature = \"dshot\")]\n"
                "let _motors = init_esc_pwm(resources, clocks);\n",
                encoding="utf-8",
            )
            (source / "lib.rs").write_text("", encoding="utf-8")
            (board / "routes.rs").write_text(
                "pub const ACTIVE_STATIC_PWM_ROUTES: &[u8] = &[];\n",
                encoding="utf-8",
            )

            errors = validate_rtic_boundaries(root)

        self.assertTrue(any("dshot is board-standard" in error for error in errors))
        self.assertTrue(any("esc_telemetry is board-standard" in error for error in errors))
        self.assertTrue(any("flash_blackbox is board-standard" in error for error in errors))
        self.assertTrue(any("board feature must enable" in error for error in errors))
        self.assertTrue(any("flight ESC output must use DShot" in error for error in errors))
        self.assertTrue(any("stale flight-output route" in error for error in errors))



if __name__ == "__main__":
    unittest.main()
