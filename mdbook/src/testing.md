# Testing

FerroWasp verification is staged so a successful software check is never
mistaken for target or flight evidence.

## Software Checks

From the repository root, contributors normally run:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --locked
cargo test --workspace --locked
python -m unittest discover -s tools/tests -v
python tools/check_repository_context.py
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
A result applies only to the recorded image, hardware, configuration, and
airframe.

## Foxeer User Workflow

The [Foxeer F405 V2 guide](user/foxeer_f405_v2.md) covers release-image
validation, USB DFU flashing, disarmed configuration changes, blackbox
download, and post-change props-off checks. It is an operating guide, not
flight authorization.

Machine-enforced test selection, retained run evidence, and historical
engineering records are maintained as internal repository metadata rather
than duplicated in the public documentation.
