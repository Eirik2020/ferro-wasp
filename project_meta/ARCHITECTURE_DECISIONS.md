# FerroWasp Architecture Decisions

This is the bounded index for FerroWasp architecture decision records (ADRs).
Open only the decisions relevant to the task. The standalone records preserve
the wording and status from the former combined ledger.

## Authority And Interpretation

- Current code, manifests, and tests remain authoritative for implemented
  behavior.
- An accepted, non-superseded ADR records durable architecture direction.
- A superseded ADR is retained for provenance but does not override its named
  successor.
- A pending-validation status is not evidence that target validation passed.
- When statuses or implementation appear inconsistent, inspect the referenced
  decisions and current target evidence rather than inferring a resolution.

The byte-for-byte ledger before this split is retained as
[`archive/project_meta/ARCHITECTURE_DECISIONS_FULL_BASELINE_2026-07-27.md`](../archive/project_meta/ARCHITECTURE_DECISIONS_FULL_BASELINE_2026-07-27.md).
Its immutable size and SHA-256 are enforced through
`DOCUMENT_REGISTRY.json`.

## Decision Index

Decisions in force. Decisions accepted but not yet implemented are indexed in
[the planned decisions](ARCHITECTURE_DECISIONS_PLANNED.md), and retired and
superseded ones in [the decision history](ARCHITECTURE_DECISIONS_HISTORY.md).

| ID | Decision | Status |
|---|---|---|
| [ADR-0008](decisions/ADR-0008.md) | Control Scheduling Is Board-Selected And TIM2 Is The I/O Timebase | accepted; its control scheduler is superseded by ADR-0058, its TIM2 I/O timebase stands |
| [ADR-0009](decisions/ADR-0009.md) | TIM6 Watchdog Posts Recovery To The SPI Owner Priority | accepted |
| [ADR-0010](decisions/ADR-0010.md) | SPI1 Uses One Owned Async Mailbox Between Task And IRQ Owner | accepted; its mailbox becomes SPSC queues per ADR-0055 |
| [ADR-0011](decisions/ADR-0011.md) | Serial RX Uses Owned Chunks With Out-Of-Band Continuity | accepted; its RX mailbox becomes an SPSC link per ADR-0057 |
| [ADR-0012](decisions/ADR-0012.md) | Serial TX Separates Writer Capacity From Hardware Completion | accepted |
| [ADR-0013](decisions/ADR-0013.md) | USART2 RC Validity Is Separate From Parsed Setpoints | accepted |
| [ADR-0015](decisions/ADR-0015.md) | Active Motor Values Cross One SPSC Authority Boundary | accepted; generalized by ADR-0055, and the motor-output boundary moves to the output group per ADR-0057 |
| [ADR-0017](decisions/ADR-0017.md) | The First Multi-Target Proof Is An Isolated Nucleo Bring-Up App | accepted |
| [ADR-0018](decisions/ADR-0018.md) | Deployable Firmware Uses Isolated App Packages | accepted |
| [ADR-0019](decisions/ADR-0019.md) | The F401 Bring-Up App Uses A Minimal RTIC 2 Shell | accepted |
| [ADR-0020](decisions/ADR-0020.md) | Foxeer F405 V2 Uses A Separate Inhibited Flight App | accepted in part: its RC PWM output and DShot deferral are superseded by ADR-0039, and its arming inhibit was lifted by target evidence |
| [ADR-0022](decisions/ADR-0022.md) | Foxeer Cargo Run Uses The STM32 ROM-DFU Bootloader | accepted; amended by ADR-0043 |
| [ADR-0025](decisions/ADR-0025.md) | IMU Orientation Uses Explicit Sensor, Board, And Drone Frames | accepted |
| [ADR-0033](decisions/ADR-0033.md) | Foxeer IMU Sampling Is Triggered By PC4/EXTI4 | accepted; validated on target by the Foxeer flights |
| [ADR-0034](decisions/ADR-0034.md) | Foxeer Onboard Flash Uses A Low-Priority CPU-Driven SPI2 Owner | accepted; validated on target by the Foxeer flights; the manager is shared by ADR-0037 |
| [ADR-0035](decisions/ADR-0035.md) | Board Support Lives With Each Isolated App | accepted; location amended by ADR-0036 |
| [ADR-0036](decisions/ADR-0036.md) | Each Board's Facts Live In A Host-Tested Data Crate | accepted; amends the location in ADR-0035 |
| [ADR-0037](decisions/ADR-0037.md) | One Flash Manager Serves Every Board Through NorStorage | accepted, pending target validation |
| [ADR-0039](decisions/ADR-0039.md) | Flight Boards Drive Four DShot600 Lanes As One Fault Domain | accepted; cadence amended by ADR-0046 |
| [ADR-0040](decisions/ADR-0040.md) | DShot Arming Requires Telemetry-Qualified Idle On Every Motor | accepted; its eRPM source is superseded by ADR-0048 where bidirectional DShot runs |
| [ADR-0041](decisions/ADR-0041.md) | Motor Bench Actuation Is Capped And Explicitly Gated | accepted; motor-test routing amended by ADR-0056 |
| [ADR-0042](decisions/ADR-0042.md) | USB And The Configurator Port Carry One Bounded Command Line | accepted; request routing amended by ADR-0056 |
| [ADR-0043](decisions/ADR-0043.md) | The Command Line Reboots Into The STM32 ROM Bootloader | accepted; reboot routing amended by ADR-0056 |
| [ADR-0044](decisions/ADR-0044.md) | Accelerometer Level Is Calibrated On Command And Stored Per Craft | accepted; calibration owner amended by ADR-0059 |
| [ADR-0045](decisions/ADR-0045.md) | Bidirectional DShot Replies Return On Each Lane's Own Pin And Stream | accepted, implemented on the Foxeer behind a props-off bench feature, pending target validation |
| [ADR-0046](decisions/ADR-0046.md) | DShot Frames Follow The Control Loop | accepted, pending target validation; amends the cadence in ADR-0039 |
| [ADR-0050](decisions/ADR-0050.md) | Requirement Status Is Computed From Evidence, Never Recorded | accepted; implemented in tools/reqtool |
| [ADR-0051](decisions/ADR-0051.md) | Verification Levels Are One Enum And Only Gates Block | accepted; implemented in tools/reqtool except the analysis-level checks |
| [ADR-0052](decisions/ADR-0052.md) | Fingerprints Are Derived, And Parents And Children Verify Each Other | accepted; implemented in tools/reqtool except variants and the path and hardware-conformance scopes, which wait on the app! parser and the -critical crates |
| [ADR-0053](decisions/ADR-0053.md) | Requirements Are YAML Data Rooted In W1-W7 And Checked By reqtool | accepted; implemented in tools/reqtool except the app! parser and the analysis-level checks |

## Adding Or Changing A Decision

1. Add the next sequential `project_meta/decisions/ADR-NNNN.md` record with one
   matching H1 heading and one non-empty `Status:` line.
2. Add the matching index row and document-registry entry.
   A decision not yet implemented is indexed in
   [the planned decisions](ARCHITECTURE_DECISIONS_PLANNED.md); when it is
   implemented, move its row here. When a decision is retired or superseded,
   move its row to [the decision history](ARCHITECTURE_DECISIONS_HISTORY.md).
3. Record supersession explicitly in the affected status or decision body;
   never delete the earlier record.
4. Run `python tools/check_repository_context.py` and the repository-context
   tests before review.

Do not append full decision bodies to this index. Historical reconstruction
must use the immutable combined baseline.
