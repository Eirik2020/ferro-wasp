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

| ID | Decision | Status |
|---|---|---|
| [ADR-0001](decisions/ADR-0001.md) | Refactor Branch Boundary | retired: the refactor branch it governed no longer exists |
| [ADR-0002](decisions/ADR-0002.md) | FerroWasp FCU3 Pin Map Is Frozen | retired with FCU3, removed 2026-10-07 |
| [ADR-0003](decisions/ADR-0003.md) | Safety Authority Is Preserved During Refactor | retired: the refactor ended; governing requirement W1 holds the rule |
| [ADR-0004](decisions/ADR-0004.md) | PWM DMA And DShot Timer DMA Are Deferred | superseded by ADR-0039 |
| [ADR-0005](decisions/ADR-0005.md) | Workspace Split Starts With Contracts | retired: the workspace split it began is complete |
| [ADR-0006](decisions/ADR-0006.md) | STM32F4 Backend Starts As Compile-Shaped Metadata | superseded by ADR-0007 |
| [ADR-0007](decisions/ADR-0007.md) | BSP Owns Board Policy, MCU Backend Owns HAL Mechanisms | superseded by ADR-0035 |
| [ADR-0008](decisions/ADR-0008.md) | Control Scheduling Is Board-Selected And TIM2 Is The I/O Timebase | accepted; its control scheduler is superseded by ADR-0058, its TIM2 I/O timebase stands |
| [ADR-0009](decisions/ADR-0009.md) | TIM6 Watchdog Posts Recovery To The SPI Owner Priority | accepted |
| [ADR-0010](decisions/ADR-0010.md) | SPI1 Uses One Owned Async Mailbox Between Task And IRQ Owner | accepted; its mailbox becomes SPSC queues per ADR-0055 |
| [ADR-0011](decisions/ADR-0011.md) | Serial RX Uses Owned Chunks With Out-Of-Band Continuity | accepted; its RX mailbox becomes an SPSC link per ADR-0057 |
| [ADR-0012](decisions/ADR-0012.md) | Serial TX Separates Writer Capacity From Hardware Completion | accepted |
| [ADR-0013](decisions/ADR-0013.md) | USART2 RC Validity Is Separate From Parsed Setpoints | accepted |
| [ADR-0014](decisions/ADR-0014.md) | The Actuator Owner Guards Long Arming Holds | superseded by ADR-0040 |
| [ADR-0015](decisions/ADR-0015.md) | Active Motor Values Cross One SPSC Authority Boundary | accepted; generalized by ADR-0055, and the motor-output boundary moves to the output group per ADR-0057 |
| [ADR-0016](decisions/ADR-0016.md) | FCU3 Is An Explicit BSP Target Without An App Adapter | superseded by ADR-0035; historical board facts retained |
| [ADR-0017](decisions/ADR-0017.md) | The First Multi-Target Proof Is An Isolated Nucleo Bring-Up App | accepted |
| [ADR-0018](decisions/ADR-0018.md) | Deployable Firmware Uses Isolated App Packages | accepted |
| [ADR-0019](decisions/ADR-0019.md) | The F401 Bring-Up App Uses A Minimal RTIC 2 Shell | accepted |
| [ADR-0020](decisions/ADR-0020.md) | Foxeer F405 V2 Uses A Separate Inhibited Flight App | accepted in part: its RC PWM output and DShot deferral are superseded by ADR-0039, and its arming inhibit was lifted by target evidence |
| [ADR-0021](decisions/ADR-0021.md) | Foxeer USB CDC Debug Is Read-Only And Bounded | superseded by ADR-0042 |
| [ADR-0022](decisions/ADR-0022.md) | Foxeer Cargo Run Uses The STM32 ROM-DFU Bootloader | accepted; amended by ADR-0043 |
| [ADR-0023](decisions/ADR-0023.md) | FCU3 DShot Starts As An Opt-In Physical-M1 Actuator Backend | superseded by ADR-0039 |
| [ADR-0024](decisions/ADR-0024.md) | FCU3 Four-Motor DShot Is One Synchronized Fault Domain | superseded by ADR-0039 |
| [ADR-0025](decisions/ADR-0025.md) | IMU Orientation Uses Explicit Sensor, Board, And Drone Frames | accepted |
| [ADR-0026](decisions/ADR-0026.md) | DShot Logical-Motor Identification Precedes Mixed Control | superseded by ADR-0041 |
| [ADR-0027](decisions/ADR-0027.md) | DShot Unequal-Packet Validation Uses A Fixed Capped Vector | retired: the unequal-vector bench image was removed |
| [ADR-0028](decisions/ADR-0028.md) | DShot Arming Preparation Remains Stopped Until Safety Arms | superseded by ADR-0031 |
| [ADR-0029](decisions/ADR-0029.md) | Mixed-Control DShot Requires An Explicit Clean Candidate Feature | superseded by ADR-0030 |
| [ADR-0030](decisions/ADR-0030.md) | FCU3 Promotes DShot600 To The Standard Output | superseded by ADR-0039 |
| [ADR-0031](decisions/ADR-0031.md) | FCU3 DShot Arming Uses Telemetry-Qualified Guarded Idle | superseded by ADR-0040 |
| [ADR-0032](decisions/ADR-0032.md) | Foxeer Uses A Separate Capped Actuator-Validation Gate | superseded by ADR-0041 |
| [ADR-0033](decisions/ADR-0033.md) | Foxeer IMU Sampling Is Triggered By PC4/EXTI4 | accepted; validated on target by the Foxeer flights |
| [ADR-0034](decisions/ADR-0034.md) | Foxeer Onboard Flash Uses A Low-Priority CPU-Driven SPI2 Owner | accepted; validated on target by the Foxeer flights; the manager is shared by ADR-0037 |
| [ADR-0035](decisions/ADR-0035.md) | Board Support Lives With Each Isolated App | accepted; location amended by ADR-0036 |
| [ADR-0036](decisions/ADR-0036.md) | Each Board's Facts Live In A Host-Tested Data Crate | accepted; amends the location in ADR-0035 |
| [ADR-0037](decisions/ADR-0037.md) | One Flash Manager Serves Every Board Through NorStorage | accepted, pending target validation |
| [ADR-0038](decisions/ADR-0038.md) | Test Evidence Is Tied To Component IDs, Not Image Hashes | superseded by ADR-0050; its tooling stays in use until reqtool replaces it |
| [ADR-0039](decisions/ADR-0039.md) | Flight Boards Drive Four DShot600 Lanes As One Fault Domain | accepted; cadence amended by ADR-0046 |
| [ADR-0040](decisions/ADR-0040.md) | DShot Arming Requires Telemetry-Qualified Idle On Every Motor | accepted; its eRPM source is superseded by ADR-0048 where bidirectional DShot runs |
| [ADR-0041](decisions/ADR-0041.md) | Motor Bench Actuation Is Capped And Explicitly Gated | accepted; motor-test routing amended by ADR-0056 |
| [ADR-0042](decisions/ADR-0042.md) | USB And The Configurator Port Carry One Bounded Command Line | accepted; request routing amended by ADR-0056 |
| [ADR-0043](decisions/ADR-0043.md) | The Command Line Reboots Into The STM32 ROM Bootloader | accepted; reboot routing amended by ADR-0056 |
| [ADR-0044](decisions/ADR-0044.md) | Accelerometer Level Is Calibrated On Command And Stored Per Craft | accepted; calibration owner amended by ADR-0059 |
| [ADR-0045](decisions/ADR-0045.md) | Bidirectional DShot Replies Return On Each Lane's Own Pin And Stream | accepted, implemented on the Foxeer behind a props-off bench feature, pending target validation |
| [ADR-0046](decisions/ADR-0046.md) | DShot Frames Follow The Control Loop | accepted, pending target validation; amends the cadence in ADR-0039 |
| [ADR-0047](decisions/ADR-0047.md) | The RPM Filter Follows Betaflight | accepted, not yet implemented |
| [ADR-0048](decisions/ADR-0048.md) | Bidirectional eRPM Supersedes UART Telemetry For Arming | accepted, not yet implemented; supersedes ADR-0040's eRPM source where bidirectional DShot runs; code placement set by ADR-0057 |
| [ADR-0049](decisions/ADR-0049.md) | Dynamic Idle Raises The Motor Floor From The Slowest Motor's eRPM | accepted, not yet implemented |
| [ADR-0050](decisions/ADR-0050.md) | Requirement Status Is Computed From Evidence, Never Recorded | accepted, not yet implemented; supersedes ADR-0038 |
| [ADR-0051](decisions/ADR-0051.md) | Verification Levels Are One Enum And Only Gates Block | accepted, not yet implemented |
| [ADR-0052](decisions/ADR-0052.md) | Fingerprints Are Derived, And Parents And Children Verify Each Other | accepted, not yet implemented |
| [ADR-0053](decisions/ADR-0053.md) | Requirements Are YAML Data Rooted In W1-W7 And Checked By reqtool | accepted, not yet implemented |
| [ADR-0054](decisions/ADR-0054.md) | Safety-Critical Code Is Separated By Crate And Never Depends On Non-Critical Code | accepted, not yet implemented |
| [ADR-0055](decisions/ADR-0055.md) | Tasks Communicate Over Declared SPSC Links And Single-Writer Flags | accepted, not yet implemented; generalizes ADR-0015, amends ADR-0010 |
| [ADR-0056](decisions/ADR-0056.md) | The Observer Is The Only Way Out And The Coordinator The Only Way In | accepted, not yet implemented; amends ADR-0041, ADR-0042, ADR-0043 |
| [ADR-0057](decisions/ADR-0057.md) | Hardware Meets The Core Through Message Contracts | accepted, not yet implemented; amends ADR-0011 and ADR-0015, places ADR-0048 |
| [ADR-0058](decisions/ADR-0058.md) | The Control Loop Is Gyro-Synced | accepted, not yet implemented; supersedes ADR-0008's control scheduler, its TIM2 I/O timebase stands |
| [ADR-0059](decisions/ADR-0059.md) | Stored Configuration Passes One Safety-Critical Gate | accepted, not yet implemented; amends ADR-0044's calibration owner |
| [ADR-0060](decisions/ADR-0060.md) | Crates Follow The Safety-Critical Split And Own Their Tasks | accepted, not yet implemented |

## Adding Or Changing A Decision

1. Add the next sequential `project_meta/decisions/ADR-NNNN.md` record with one
   matching H1 heading and one non-empty `Status:` line.
2. Add the matching index row and document-registry entry.
3. Record supersession explicitly in the affected status or decision body;
   never delete the earlier record.
4. Run `python tools/check_repository_context.py` and the repository-context
   tests before review.

Do not append full decision bodies to this index. Historical reconstruction
must use the immutable combined baseline.
