# Requirements Design: Fault Handling and the Border Roles

Section 19 of [the requirements design](design.md), which maps every section
to its chapter. ADR-0061 to ADR-0064 are the binding records; this chapter
carries the reasoning behind them. The starting point was the user's
"FerroWasp Safety Core Architecture" document, which describes the roles
around the safety-critical core and the arm and disarm sequencing. It was
taken case by case against ADR-0054 to ADR-0060; where the two differed, the
outcome is recorded here.

## 19. Fault handling and the border roles

### The roles

```
Unprivileged tasks        Non-critical readers         Sensor tasks
       │ requests                ▲ read-only ledger          │ raw data
       ▼                         │                           ▼
  Coordinator                Observer                 Health Manager   ← border roles
       │ requests                ▲ state copy, one-way       │ classified faults
       ▼                         │                           ▼
  ┌───────────────────────── Safety Master ───────────────────────────┐
  │                              │ gates                               │
  │  Command Input ──SPSC──▶ Control Loop ──SPSC──▶ Actuator Output     │  privileged core
  │                              ▲ measurements                        │
  │                              └──────────── Actuator Feedback ───────┘
  └─ HAL border: messages in (RxChunk), messages out (MotorCommand) ─────┘
```

Already consistent with the earlier decisions and the code: the observer as a
one-way diode to a read-only ledger (ADR-0056); the safety master at the
highest priority, already 16 on the Foxeer above `actuator_output` (15) and
`control_loop` (14); the arming round trip, which today is `ArmRequested`,
guarded idle and eRPM qualification, then `SafetyEvent::ActuatorIdling`;
disarm immediate and unconditional (W2); SPSC links with compile-time
ownership (ADR-0055); and a narrow safety-critical HAL beside the broad one,
unverified by default in `reqtool`, which is the `-critical` backend crates
with mandatory linking (ADR-0054).

### The coordinator

The document called the coordinator a dumb pipe that forwards requests
without judging them, with every request going to the safety master.
ADR-0056's coordinator validates content, arbitrates between requesters and
runs the arm-inhibit, stage and commit transaction. The user kept ADR-0056's,
and stated its main purpose: to stop unprivileged tasks from stalling the
system by spamming or stalling the safety master. "Does not judge" therefore
means "never judges flight state", which ADR-0056 already said; routing every
request through the safety master was the option ADR-0056 rejected, because
the safety master would grow and re-owe its evidence with every request type.

### Two fault paths

The user's statement of the Health Manager's job: it aggregates data that is
not a hard fault and turns it into actionable items. A dead IMU or a dead RC
link causes a high-priority fault at once, so those are excluded from it, and
it is not the Health Manager's job to determine an IMU fault.

- **Direct.** A consuming task's gate detects its own input's failure and
  reports it straight to the safety master: a dead or stale IMU, RC loss, a
  stopped control loop, an arm-state mismatch, a DShot bank fault. ADR-0055
  already made staleness the consumer's decision.
- **Aggregated.** Everything below that goes to the Health Manager: ESC
  temperature and voltage, reply-loss rate, RC link quality and CRC error
  rate, isolated stale IMU ticks, gyro saturation counts, loop overruns. It
  reads them from the observer's ledger rather than from links of its own,
  and may use the same data the gates use, for an aggregated fault.

The Health Manager can decide that a combination of data is a hard fault and
relay it to the safety master, which handles it like any other hard fault. A
proposal to limit it to warnings and arming refusal was rejected. Each
escalation rule is a requirement with its own evidence, since a false hard
fault ends a flight.

The document left open whether the Health Manager should be one task or a
fast bounded-check tier plus a slower classifier. With primary detection at
the gates, the gates are the fast tier, nothing time-critical waits on the
Health Manager, and the question disappears.

### Fault classes

The safety master acts on the class, not on the source:

| Class | While armed | While disarmed |
|---|---|---|
| hard fault | disarm at once (W2) | refuse arming |
| failsafe-level | the failsafe | refuse arming |
| land immediately | no automatic action; an urgent warning | refuse arming while present |
| fault | no flight effect; reported | refuse arming until it clears |
| fault-imminent | reported | reported; arming allowed |

Hard faults latch and only a reboot releases them: after a hard fault the
board is not trusted until it restarts and passes its boot checks, as with
ADR-0056's config-transaction latch. Disarm-and-rearm and an explicit
acknowledgement were the alternatives. Failsafe-level and lower faults do not
latch.

**RC loss is failsafe-level.** The user had first grouped a dead RC with the
dead IMU as a high-priority fault; it stays high-priority and direct, from
Command Input's gate, but it does not latch, so a receiver glitch on the
ground does not need a reboot. Once the link is valid again, arming is allowed
after the normal checks.

**The failsafe mirrors Betaflight**: a stage 1 guard period, then the stage 2
procedure. Today RC loss revokes the arm permit and disarms on the first
invalid frame, with no gentler failsafe, because no angle mode exists to
descend with. Until angle mode is flight-verified, stage 2 can only be a drop.
Betaflight's timings, stage 1 channel values and recovery rules are taken from
its source at implementation, not from memory.

**Land immediately** means the user must get the system into a safe state as
quickly as possible, like a low-battery warning: the drone will crash unless
they land immediately. It exists for the border roles (below). Their failure
was first set at failsafe-level, but until angle mode exists the failsafe ends
in a drop, so a hung observer would have brought the craft down; the user
lowered the severity to this new class.

### The border roles

The coordinator, the observer and the Health Manager live on the border of
the core. All three are safety-critical, and all three run below the control
path and the safety master and above every non-critical task, so a border
role that hangs or faults cannot bring the system down. Its failure raises
land immediately, never a hard fault.

The observer was non-critical under ADR-0056, because the producers'
never-block rule was the safety-critical part. With the Health Manager
reading the ledger, a non-critical observer would break the direction rule,
so it becomes safety-critical. A third, "border" class was considered and not
needed: the border roles are safety-critical crates outside the core control
path, core tasks never depend on them for primary safety, and unprivileged
tasks depend on them.

**Heartbeats.** Each border role sends a heartbeat (sequence, timestamp) to
the safety master over a `state` link, beating even when idle; one older than
its role's limit raises land immediately with the role's id. The ages are
checked in the safety master's periodic check (ADR-0063). The control loop's
tick was proposed first and rejected: it stops when the IMU does.

**How the observer fails.** The user asked how likely the observer is to fail
without taking the OSD with it.

| Observer failure | The OSD | Why |
|---|---|---|
| panic | stops with everything | `panic-probe` stops the whole MCU |
| busy hang | stops | the OSD (priority 3) runs below the border roles and is starved |
| silent stall: stuck waiting, or a bug that stops updating topics | keeps running, showing frozen values | the observer uses no CPU, but the ledger stops advancing |

Only the silent stall leaves the OSD running, and the ledger's per-topic
sequence already shows it. So the OSD and telemetry show a warning when the
topics they read stop advancing. That also catches an observer that still
beats but stopped publishing. The user chose the OSD's form: a blinking
"DATA STALE" row while the observer's own topic or the control loop's has
held for 250 ms, and dashes for the battery values while the battery's topic
has held for 500 ms, twice its period, rather than one warning row alone or
a blanked overlay (OSD-001). The radio's warning over CRSF waits. A pilot-alert flag written by the safety master
and read by the OSD directly, bypassing the observer, was proposed and dropped
as unnecessary; land immediately from a hung coordinator or Health Manager
reaches the pilot through the observer.

**One crate.** The border roles share one safety-critical `ferrowasp-border`
crate, one module each. Giving each its own crate raised the question whether
crates would balloon into hundreds of single-task ones. They do not if crates
follow evidence: a new crate only when its bench or flight evidence must
survive changes to its neighbour's code, or for a standalone driver or
protocol; everything else is a module. The boundary that matters is the
border roles versus the safety master. Between the border roles, evidence is
mostly unit and SITL, which costs nothing to re-run. The ledger's topic types
live in `ferrowasp-boundary-contracts`, so the OSD and telemetry depend on the
contracts, not on the border crate. The §16 crate map has about 31 crates for
about 40 tasks; growth comes from new sensors and protocols, not tasks.

### The independent watchdog

The safety master is the only disarm authority. If it hangs in flight while
the control loop runs, the craft flies on and neither an RC disarm nor the
RC-loss failsafe can stop it: Actuator Output's freshness check needs the
control loop to stop, and the arm-state cross-check reads a flag a hung
safety master leaves at "armed". A reset, which stops the motors, is the only
way out; losing disarm authority is worse than a drop.

No independent watchdog exists: `ferrowasp-stm32f4::watchdog` is a
timer-driven I/O watchdog whose task (`io_watchdog`) expires overdue SPI1
transactions. The design first assumed a hardware watchdog backed the core;
checking the code showed none, so ADR-0063 adds the IWDG. The user delegated
its details.

- Only the safety master's own periodic check feeds it, on the monotonic at a
  fixed rate. Tying the feed to the control loop as well was rejected: a dead
  IMU would then reboot the board in a loop on the bench instead of being
  reported, and a hung control loop is already the safety master's hard
  fault.
- It covers whatever stops that check: a hung or livelocked safety master,
  starvation by higher interrupts or a global critical section, a dead
  monotonic, a panic or HardFault loop. Today a panic halts the MCU until a
  power cycle.
- About 100 ms nominal. The LSI frequency varies widely between parts and with
  temperature, so the nominal is chosen so the shortest possible timeout clears
  the safety master's worst legitimate gap by a wide margin, and the longest is
  a stated requirement. Started at the end of init, which takes longer than
  that.
- The debugger freeze bit is set, so a halt under `probe-rs` does not reset
  the board.
- After an IWDG reset the boot reports the cause and refuses arming until it
  is acknowledged, as with `AcknowledgeConfigFallback`: something hung the
  core, and the board should not fly again until someone has seen why.

### Arming and disarming

Arming is confirmed before it is declared: the safety master asks Actuator
Output to arm, and declares the system armed only on its success report.
Actuator Output judges its own success, fresh in-range eRPM on every motor,
from a link of its own from actuator feedback; letting the Health Manager
judge qualification was the alternative, but then Actuator Output could not
confirm its own physical state. Actuator Output also cross-checks its own
armed state against the system-armed flag (ADR-0064).

The document says Actuator Output retries disarm until it succeeds. DShot has
no acknowledgement: the firmware knows what it sent, not what an ESC did. Two
mechanisms already exist: while disarmed, ADR-0046's fallback keeps stop
frames going at no less than 500 Hz, and a bank fault forces every output low,
a signal loss on which an ESC stops its motor. So success means a stop frame
set completed or the lines were forced low; stop is re-asserted every period
while disarmed; and where bidirectional eRPM exists, a motor still spinning
past a spin-down limit is a hard fault. Spin-down is a cross-check, not the
success condition, since it depends on the prop and its inertia.

### Readings of the document

- **Byte streams.** The document puts the core's border at a byte stream in
  and out, enforced by traits. ADR-0057 stands: `RxChunk` in, which carries
  the discontinuities a bare byte stream would lose (ADR-0011), and
  `MotorCommand` out, so DShot or PWM encoding, bidirectional inversion and
  the turnaround stay in the output group. Traits live below the core, in the
  hardware groups over the narrow safety-critical HAL. The document's intent
  holds: the core never touches hardware, and moving it to another board
  proves only that board's contract implementation.
- **Atomic flags.** The document calls flags simple state without
  ownership semantics. ADR-0055 stands: no queue semantics, but one writer
  enforced by the type system, single-word values, and trusted by a
  safety-critical reader only while the writer's liveness is covered, which
  for the safety master is the IWDG.

### Deferred: a domain-agnostic safety master

The document's safety master does not know whether it flies a quad, a car or
a horse; the Health Manager translates domain signals into its vocabulary.
The user deferred this. The proposed sort of today's `safety.rs` (1,367
lines), kept as the starting point:

| Goes to | From today's `safety.rs` |
|---|---|
| Command Input | `ARM_THRESHOLD`, `ARM_HOLD_US`, `RC_LINK_TIMEOUT_US`, `RC_LINK_RECOVERY_FRAMES`, `RcLinkState`, `classify_rc_frame_flags`, `RcInputFault`, `rc_fault_response`, `neutralize_rc_outputs`, `RcRates`; the RC half of `validate_arming_guard` (arm switch high, throttle low); the failsafe stages |
| Actuator Output | the ESC and PWM throttle constants, the BLHeli arm holds, PWM calibration, `MotorCmd` and its freshness check, `validate_active_motor_outputs`, the arming sequence with eRPM qualification, its armed state and the cross-check |
| coordinator | the content half of `validate_bench_motor_request` (a motor index from 1 to 4) |
| safety master | the arming state machine, the response per fault class, latches, inhibits and leases, the arm handshake, heartbeat checks and the IWDG feed, the system-armed flag |
| deleted | `IMU_STALE_TICKS_TOLERATED` and `imu_stale_beyond_tolerance` (gyro-sync); `RcRatesWriter`, a `Mutex<RefCell>`, which becomes a `state` link |

Also proposed for then: arming prerequisites (`PreArmHealth`) become faults
raised by the gates; `ArmingAbortReason` becomes a class plus an opaque
source id and code, decoded by the observer and the configurator; and a check
that `ferrowasp-safety` depends on no RC, IMU or actuator crate. Splitting
`ferrowasp-actuator` out of `ferrowasp-safety` waits for the same step.
