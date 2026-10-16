#!/usr/bin/env python3
"""Run FerroWasp's test suites the same way locally and in CI.

    python tools/run_tests.py              every suite
    python tools/run_tests.py rust gui     only those
    python tools/run_tests.py --list       what each suite runs

Each suite is a list of commands. Output streams as it runs; the summary
counts the tests each suite ran, so a suite that quietly runs fewer tests
shows as a smaller number rather than as a pass. The exit code is nonzero
when any suite fails, after every requested suite has run.
"""

from __future__ import annotations

import argparse
import functools
import re
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

CARGO_RESULT = re.compile(r"^test result: \w+\. (\d+) passed; (\d+) failed")
PYTHON_RESULT = re.compile(r"^Ran (\d+) tests? in")


# Replaced in a command by the host's target triple, looked up only when a
# selected suite needs it: asking the root's pinned toolchain would make a
# configurator-only run install it.
HOST = "{host}"


@functools.cache
def host_triple() -> str:
    output = subprocess.run(
        ["rustc", "-vV"], capture_output=True, text=True, check=True, cwd=ROOT
    ).stdout
    for line in output.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise RuntimeError("rustc -vV reported no host triple")


@dataclass(frozen=True)
class Step:
    command: list[str]
    cwd: str = "."


@dataclass(frozen=True)
class Suite:
    name: str
    why: str
    steps: list[Step] = field(default_factory=list)


def suites() -> list[Suite]:
    host_test = ["cargo", "test", "--locked", "--target", HOST]
    return [
        Suite(
            "rust",
            "every crate in the root workspace, with features unified across it",
            [Step([*host_test, "--workspace"])],
        ),
        Suite(
            "rust-features",
            "tests a workspace-wide run compiles out: the H7 backend turns on "
            "ferrowasp-stm32/dshot, replacing its non-DShot timer DMA test, and "
            "nothing in the workspace turns on mspv2_configurator's RPC test",
            [
                Step([*host_test, "-p", "ferrowasp-stm32"]),
                Step([*host_test, "-p", "ferrowasp-flight", "--features", "mspv2_configurator"]),
            ],
        ),
        Suite(
            "bringup",
            "the STM32F401 bring-up app's board manifest, its own workspace",
            [Step([*host_test, "--lib"], "firmware/stm32f401-bringup")],
        ),
        Suite(
            "configurator",
            "FerroConfigurator's core, bridge and CLI, its own workspace",
            [Step(["cargo", "test", "--locked", "--workspace"], "tools/ferro-configurator")],
        ),
        Suite(
            "reqtool",
            "the requirements tool, its own workspace, including a check that "
            "every requirement file in the repository is valid",
            [Step(["cargo", "test", "--locked"], "tools/reqtool")],
        ),
        Suite(
            "gui",
            "the configurator GUI: TypeScript types, the page build, and the "
            "Tauri shell, which has no other check",
            [
                Step(["npx", "tsc", "--noEmit"], "tools/ferro-configurator/gui"),
                Step(["npm", "run", "build"], "tools/ferro-configurator/gui"),
                Step(
                    ["cargo", "clippy", "--locked", "--", "-D", "warnings"],
                    "tools/ferro-configurator/gui/src-tauri",
                ),
            ],
        ),
        Suite(
            "python",
            "the host tools in tools/",
            [Step([sys.executable, "-m", "unittest", "discover", "-s", "tools/tests"])],
        ),
        Suite(
            "rtic-app-builder",
            "the isolated generator workspace, on its own pinned toolchain",
            [Step(["cargo", "test", "--locked", "--workspace"], "tools/rtic-app-builder")],
        ),
    ]


@dataclass
class Outcome:
    suite: str
    passed: bool = True
    tests: int = 0
    failed_tests: int = 0
    seconds: float = 0.0
    failed_command: str = ""


def run_step(step: Step, outcome: Outcome) -> bool:
    cwd = ROOT / step.cwd
    command = [host_triple() if part == HOST else part for part in step.command]
    print(f"\n$ (cd {step.cwd}) {' '.join(command)}", flush=True)
    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
    )
    assert process.stdout is not None
    for line in process.stdout:
        sys.stdout.write(line)
        if match := CARGO_RESULT.match(line):
            outcome.tests += int(match.group(1))
            outcome.failed_tests += int(match.group(2))
        elif match := PYTHON_RESULT.match(line):
            outcome.tests += int(match.group(1))
    return process.wait() == 0


def run_suite(suite: Suite) -> Outcome:
    outcome = Outcome(suite.name)
    print(f"\n==== {suite.name}: {suite.why}", flush=True)
    start = time.monotonic()
    for step in suite.steps:
        if not run_step(step, outcome):
            outcome.passed = False
            outcome.failed_command = " ".join(step.command)
            break
    outcome.seconds = time.monotonic() - start
    return outcome


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("suites", nargs="*", help="suites to run; all when omitted")
    parser.add_argument("--list", action="store_true", help="list the suites and stop")
    args = parser.parse_args()

    available = suites()
    by_name = {suite.name: suite for suite in available}
    if args.list:
        for suite in available:
            print(f"{suite.name}: {suite.why}")
            for step in suite.steps:
                print(f"    (cd {step.cwd}) {' '.join(step.command)}")
        return 0
    unknown = [name for name in args.suites if name not in by_name]
    if unknown:
        parser.error(f"unknown suite(s) {', '.join(unknown)}; choose from {', '.join(by_name)}")
    selected = [by_name[name] for name in args.suites] if args.suites else available

    outcomes = [run_suite(suite) for suite in selected]

    print("\n==== summary")
    for outcome in outcomes:
        status = "ok  " if outcome.passed else "FAIL"
        detail = f"  {outcome.failed_tests} failed" if outcome.failed_tests else ""
        print(f"{status} {outcome.suite:<17} {outcome.tests:>5} tests  {outcome.seconds:6.1f} s{detail}")
        if not outcome.passed:
            print(f"     failed: {outcome.failed_command}")
    return 0 if all(outcome.passed for outcome in outcomes) else 1


if __name__ == "__main__":
    sys.exit(main())
