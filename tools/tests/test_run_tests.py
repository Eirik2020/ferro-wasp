import contextlib
import io
import sys
import unittest

from tools.run_tests import (
    CARGO_RESULT,
    HOST,
    PYTHON_RESULT,
    ROOT,
    Step,
    Suite,
    run_suite,
    suites,
)


def python_step(code: str) -> Step:
    return Step([sys.executable, "-c", code])


def quietly(suite: Suite):
    with contextlib.redirect_stdout(io.StringIO()):
        return run_suite(suite)


class RunTestsTests(unittest.TestCase):
    def test_every_suite_is_named_once_and_runs_somewhere_that_exists(self) -> None:
        names = [suite.name for suite in suites()]
        self.assertEqual(len(names), len(set(names)))
        for suite in suites():
            self.assertTrue(suite.steps, suite.name)
            for step in suite.steps:
                self.assertTrue((ROOT / step.cwd).is_dir(), f"{suite.name}: {step.cwd}")

    def test_the_host_triple_is_filled_in_late(self) -> None:
        rust = next(suite for suite in suites() if suite.name == "rust")
        self.assertIn(HOST, rust.steps[0].command)

    def test_counts_cargo_and_python_results(self) -> None:
        match = CARGO_RESULT.match(
            "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
        )
        self.assertEqual((match.group(1), match.group(2)), ("12", "0"))
        self.assertEqual(PYTHON_RESULT.match("Ran 119 tests in 0.7s").group(1), "119")
        self.assertEqual(PYTHON_RESULT.match("Ran 1 test in 0.0s").group(1), "1")

    def test_a_suite_sums_its_steps_and_stops_at_the_first_failure(self) -> None:
        passing = quietly(
            Suite(
                "fake",
                "two passing steps",
                [
                    python_step("print('test result: ok. 3 passed; 0 failed; 0 ignored')"),
                    python_step("print('Ran 2 tests in 0.1s')"),
                ],
            )
        )
        self.assertTrue(passing.passed)
        self.assertEqual(passing.tests, 5)

        failing = quietly(
            Suite(
                "fake",
                "a failing step, then one that must not run",
                [
                    python_step(
                        "print('test result: FAILED. 1 passed; 2 failed; 0 ignored'); "
                        "raise SystemExit(101)"
                    ),
                    python_step("print('Ran 9 tests in 0.1s')"),
                ],
            )
        )
        self.assertFalse(failing.passed)
        self.assertEqual((failing.tests, failing.failed_tests), (1, 2))
        self.assertIn("SystemExit(101)", failing.failed_command)


if __name__ == "__main__":
    unittest.main()
