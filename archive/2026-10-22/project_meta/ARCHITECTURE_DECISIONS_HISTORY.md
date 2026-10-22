> Archived 2026-10-22 from `project_meta/ARCHITECTURE_DECISIONS_HISTORY.md`, verbatim below this note. Its binding
> rules were promoted first: into the requirements design
> (`project_meta/requirements/`), the requirements, the public book and
> `project_meta/faq.md`. This copy is not active authority.

# Architecture Decision History

Retired and superseded decisions, kept for provenance. A superseded ADR does
not override its named successor, and a retired one no longer binds; the
active decisions are in [the decision index](ARCHITECTURE_DECISIONS.md).

| ID | Decision | Status |
|---|---|---|
| [ADR-0001](decisions/ADR-0001.md) | Refactor Branch Boundary | retired: the refactor branch it governed no longer exists |
| [ADR-0002](decisions/ADR-0002.md) | FerroWasp FCU3 Pin Map Is Frozen | retired with FCU3, removed 2026-10-07 |
| [ADR-0003](decisions/ADR-0003.md) | Safety Authority Is Preserved During Refactor | retired: the refactor ended; governing requirement W1 holds the rule |
| [ADR-0004](decisions/ADR-0004.md) | PWM DMA And DShot Timer DMA Are Deferred | superseded by ADR-0039 |
| [ADR-0005](decisions/ADR-0005.md) | Workspace Split Starts With Contracts | retired: the workspace split it began is complete |
| [ADR-0006](decisions/ADR-0006.md) | STM32F4 Backend Starts As Compile-Shaped Metadata | superseded by ADR-0007 |
| [ADR-0007](decisions/ADR-0007.md) | BSP Owns Board Policy, MCU Backend Owns HAL Mechanisms | superseded by ADR-0035 |
| [ADR-0014](decisions/ADR-0014.md) | The Actuator Owner Guards Long Arming Holds | superseded by ADR-0040 |
| [ADR-0016](decisions/ADR-0016.md) | FCU3 Is An Explicit BSP Target Without An App Adapter | superseded by ADR-0035; historical board facts retained |
| [ADR-0021](decisions/ADR-0021.md) | Foxeer USB CDC Debug Is Read-Only And Bounded | superseded by ADR-0042 |
| [ADR-0023](decisions/ADR-0023.md) | FCU3 DShot Starts As An Opt-In Physical-M1 Actuator Backend | superseded by ADR-0039 |
| [ADR-0024](decisions/ADR-0024.md) | FCU3 Four-Motor DShot Is One Synchronized Fault Domain | superseded by ADR-0039 |
| [ADR-0026](decisions/ADR-0026.md) | DShot Logical-Motor Identification Precedes Mixed Control | superseded by ADR-0041 |
| [ADR-0027](decisions/ADR-0027.md) | DShot Unequal-Packet Validation Uses A Fixed Capped Vector | retired: the unequal-vector bench image was removed |
| [ADR-0028](decisions/ADR-0028.md) | DShot Arming Preparation Remains Stopped Until Safety Arms | superseded by ADR-0031 |
| [ADR-0029](decisions/ADR-0029.md) | Mixed-Control DShot Requires An Explicit Clean Candidate Feature | superseded by ADR-0030 |
| [ADR-0030](decisions/ADR-0030.md) | FCU3 Promotes DShot600 To The Standard Output | superseded by ADR-0039 |
| [ADR-0031](decisions/ADR-0031.md) | FCU3 DShot Arming Uses Telemetry-Qualified Guarded Idle | superseded by ADR-0040 |
| [ADR-0032](decisions/ADR-0032.md) | Foxeer Uses A Separate Capped Actuator-Validation Gate | superseded by ADR-0041 |
| [ADR-0038](decisions/ADR-0038.md) | Test Evidence Is Tied To Component IDs, Not Image Hashes | superseded by ADR-0050 |
