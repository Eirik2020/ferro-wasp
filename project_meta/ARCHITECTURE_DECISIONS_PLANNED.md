# Planned Architecture Decisions

Decisions accepted but not yet implemented. They record agreed direction;
until a decision is implemented, current code, manifests and tests remain
authoritative for what the firmware does. When one is implemented, its row
moves to [the decision index](ARCHITECTURE_DECISIONS.md); retired and
superseded decisions are in [the decision history](ARCHITECTURE_DECISIONS_HISTORY.md).

| ID | Decision | Status |
|---|---|---|
| [ADR-0047](decisions/ADR-0047.md) | The RPM Filter Follows Betaflight | accepted, not yet implemented |
| [ADR-0048](decisions/ADR-0048.md) | Bidirectional eRPM Supersedes UART Telemetry For Arming | accepted, not yet implemented; supersedes ADR-0040's eRPM source where bidirectional DShot runs; code placement set by ADR-0057 |
| [ADR-0049](decisions/ADR-0049.md) | Dynamic Idle Raises The Motor Floor From The Slowest Motor's eRPM | accepted, not yet implemented |
| [ADR-0054](decisions/ADR-0054.md) | Safety-Critical Code Is Separated By Crate And Never Depends On Non-Critical Code | accepted, not yet implemented; priority rule amended by ADR-0061 |
| [ADR-0055](decisions/ADR-0055.md) | Tasks Communicate Over Declared SPSC Links And Single-Writer Flags | accepted, not yet implemented; generalizes ADR-0015, amends ADR-0010 |
| [ADR-0056](decisions/ADR-0056.md) | The Observer Is The Only Way Out And The Coordinator The Only Way In | accepted, not yet implemented; amends ADR-0041, ADR-0042, ADR-0043; observer class and border priority amended by ADR-0061 |
| [ADR-0057](decisions/ADR-0057.md) | Hardware Meets The Core Through Message Contracts | accepted, not yet implemented; amends ADR-0011 and ADR-0015, places ADR-0048 |
| [ADR-0058](decisions/ADR-0058.md) | The Control Loop Is Gyro-Synced | accepted, not yet implemented; supersedes ADR-0008's control scheduler, its TIM2 I/O timebase stands |
| [ADR-0059](decisions/ADR-0059.md) | Stored Configuration Passes One Safety-Critical Gate | accepted, not yet implemented; amends ADR-0044's calibration owner |
| [ADR-0060](decisions/ADR-0060.md) | Crates Follow The Safety-Critical Split And Own Their Tasks | accepted, not yet implemented; crate map amended by ADR-0061 |
| [ADR-0061](decisions/ADR-0061.md) | The Border Roles Are Safety-Critical And Run Below The Core | accepted, not yet implemented; amends ADR-0054, ADR-0056, ADR-0060 |
| [ADR-0062](decisions/ADR-0062.md) | Faults Have Classes, Hard Faults Latch, And The Failsafe Follows Betaflight | accepted, not yet implemented; changes the RC-loss response (W6) |
| [ADR-0063](decisions/ADR-0063.md) | An Independent Watchdog Backs Up The Safety Master | accepted, not yet implemented; a new watchdog behaviour (W6) |
| [ADR-0064](decisions/ADR-0064.md) | Actuator Output Confirms Arming And Cross-Checks Disarming | accepted, not yet implemented |
