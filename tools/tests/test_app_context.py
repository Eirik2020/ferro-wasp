import unittest

from tools.app_context import (
    MAX_OUTPUT_BYTES,
    RouteError,
    _index_symbols,
    _select_symbols,
    build_route,
    discover_symbols_from_text,
    validate_routes,
)


class AppContextTests(unittest.TestCase):
    def test_discovers_multiline_rtic_attributes_and_resources(self) -> None:
        source = """\
mod app {
    #[shared]
    struct Shared {
        state: u32,
    }

    #[local]
    struct Local {
        counter: u32,
        ready: bool,
    }

    #[task(
        binds = TIM4,

        priority = 14
    )]
    fn control_loop(_cx: control_loop::Context) {
    }

    #[cfg(feature = "usb_serial")]
    #[task(priority = 1)]
    async fn usb_fs(_cx: usb_fs::Context) {
    }

    fn helper() {
    }
}
"""

        symbols, resources = discover_symbols_from_text(source)
        indexed = _index_symbols(symbols)

        control = indexed["control_loop"][0]
        self.assertEqual(control.kind, "task")
        self.assertEqual(control.priority, 14)
        self.assertEqual(control.interrupt_binding, "TIM4")
        self.assertEqual(control.start_line, 13)

        usb = indexed["usb_fs"][0]
        self.assertEqual(usb.kind, "task")
        self.assertEqual(usb.features, ("usb_serial",))

        self.assertEqual(
            [(block.name, block.field_count) for block in resources],
            [("Shared", 1), ("Local", 2)],
        )

    def test_rejects_a_missing_routed_symbol(self) -> None:
        with self.assertRaisesRegex(RouteError, "routed symbol `missing`"):
            _select_symbols({}, ("missing",), label="fixture")

    def test_rejects_foxeer_only_topic_for_fcu3(self) -> None:
        with self.assertRaisesRegex(RouteError, "topic is not supported"):
            build_route("fcu3", "storage")

    def test_current_routes_have_no_drift(self) -> None:
        self.assertEqual(validate_routes(), [])

    def test_foxeer_arming_route_is_bounded_and_includes_golden_app(self) -> None:
        route = build_route("foxeer-f405-v2", "arming")

        self.assertIn("apps/foxeer-f405-v2/AGENTS.md", route)
        self.assertIn("actuator_output [task priority=15]", route)
        self.assertIn("FCU3 golden-app comparison anchors:", route)
        self.assertIn("apps/stm32f405-flight/src/main.rs", route)
        self.assertIn("BENCH-FOX-001", route)
        self.assertLessEqual(len(route.encode("utf-8")), MAX_OUTPUT_BYTES)

    def test_overview_reports_interrupt_and_resource_anchors(self) -> None:
        route = build_route("fcu3", "overview")

        self.assertIn("control_loop [task priority=14 binds=TIM4]", route)
        self.assertIn("Shared:", route)
        self.assertIn("Local:", route)
        self.assertNotIn("FCU3 golden-app comparison anchors:", route)
        self.assertLessEqual(len(route.encode("utf-8")), MAX_OUTPUT_BYTES)


if __name__ == "__main__":
    unittest.main()
