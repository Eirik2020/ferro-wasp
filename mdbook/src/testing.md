# Testing

FerroWasp verification is staged so a successful software check is never
mistaken for target or flight evidence.

## Software Checks

Every test suite runs from the repository root with one command, the same one
CI runs:

```text
python tools/run_tests.py
```

It runs the root workspace, the tests a workspace-wide run compiles out through
feature unification, the bring-up app's board manifest, FerroConfigurator, its
GUI (TypeScript types, page build, and the Tauri shell), the Python tools, and
the RTIC app builder. `--list` shows each suite's commands; naming suites, as
in `python tools/run_tests.py rust python`, runs only those. The summary counts
the tests each suite ran, so a suite that quietly runs fewer reads as a smaller
number, not as a pass.

The remaining checks are separate:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python tools/check_repository_context.py
python tools/check_rtic_boundaries.py
mdbook build mdbook
```

Embedded applications are isolated Cargo workspaces. Check the selected app
from its own directory with the exact board features being reviewed. A build
proves source compatibility only; it does not prove pin routing, timing,
sensor orientation, or actuator behavior.

## Target Verification

Hardware work progresses through explicit gates:

1. unpowered boot and idle observation;
2. unpowered peripheral and signal checks;
3. powered props-off actuator checks;
4. injected fault and recovery checks;
5. bounded flight changes after the earlier gates pass.

Record the board, source revision, firmware hash, features, configuration,
power state, procedure, observations, and unresolved limitations. Stop on any
unexpected motor response, oscillation, heat, smoke, loss of RC or video, or
loss of confidence.

The user operates powered hardware and decides when to advance between gates.
A result applies to the recorded hardware, configuration, and airframe, and to
the source components the run exercised: it carries forward while those
components are unchanged and is repeated when one of them changes.

## Foxeer User Workflow

The [Foxeer F405 V2 guide](user/foxeer_f405_v2.md) covers release-image
validation, USB DFU flashing, disarmed configuration changes, blackbox
download, and post-change props-off checks. It is an operating guide, not
flight authorization.

Machine-enforced test selection, retained run evidence, and historical
engineering records are maintained as internal repository metadata rather
than duplicated in the public documentation.
