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
| [ADR-0008](decisions/ADR-0008.md) | Control Scheduling Is Board-Selected And TIM2 Is The I/O Timebase | accepted |
| [ADR-0009](decisions/ADR-0009.md) | TIM6 Watchdog Posts Recovery To The SPI Owner Priority | accepted |
| [ADR-0010](decisions/ADR-0010.md) | SPI1 Uses One Owned Async Mailbox Between Task And IRQ Owner | accepted |
| [ADR-0011](decisions/ADR-0011.md) | Serial RX Uses Owned Chunks With Out-Of-Band Continuity | accepted |
| [ADR-0012](decisions/ADR-0012.md) | Serial TX Separates Writer Capacity From Hardware Completion | accepted |
| [ADR-0013](decisions/ADR-0013.md) | USART2 RC Validity Is Separate From Parsed Setpoints | accepted |
| [ADR-0014](decisions/ADR-0014.md) | The Actuator Owner Guards Long Arming Holds | superseded by ADR-0040 |
| [ADR-0015](decisions/ADR-0015.md) | Active Motor Values Cross One SPSC Authority Boundary | accepted |
| [ADR-0016](decisions/ADR-0016.md) | FCU3 Is An Explicit BSP Target Without An App Adapter | superseded by ADR-0035; historical board facts retained |
| [ADR-0017](decisions/ADR-0017.md) | The First Multi-Target Proof Is An Isolated Nucleo Bring-Up App | accepted |
| [ADR-0018](decisions/ADR-0018.md) | Deployable Firmware Uses Isolated App Packages | accepted |
| [ADR-0019](decisions/ADR-0019.md) | The F401 Bring-Up App Uses A Minimal RTIC 2 Shell | accepted |
| [ADR-0020](decisions/ADR-0020.md) | Foxeer F405 V2 Uses A Separate Inhibited Flight App | accepted in part: its RC PWM output and DShot deferral are superseded by ADR-0039, and its arming inhibit was lifted by target evidence |
| [ADR-0021](decisions/ADR-0021.md) | Foxeer USB CDC Debug Is Read-Only And Bounded | superseded by ADR-0042 |
| [ADR-0022](decisions/ADR-0022.md) | Foxeer Cargo Run Uses The STM32 ROM-DFU Bootloader | accepted |
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
| [ADR-0038](decisions/ADR-0038.md) | Test Evidence Is Tied To Component IDs, Not Image Hashes | accepted |
| [ADR-0039](decisions/ADR-0039.md) | Flight Boards Drive Four DShot600 Lanes As One Fault Domain | accepted |
| [ADR-0040](decisions/ADR-0040.md) | DShot Arming Requires Telemetry-Qualified Idle On Every Motor | accepted |
| [ADR-0041](decisions/ADR-0041.md) | Motor Bench Actuation Is Capped And Explicitly Gated | accepted |
| [ADR-0042](decisions/ADR-0042.md) | USB And The Configurator Port Carry One Bounded Command Line | accepted |

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
