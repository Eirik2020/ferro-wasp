# Requirements Design: Safety-Critical Architecture

Sections 8 to 11 of [the requirements design](design.md), which
maps every section to its chapter.

## 8. Safety-critical and non-critical code

**Two axes.** The user's example path:

```
UART ──▶ command_input ──▶ control_loop ──▶ actuator_authority ──▶ PWM / DShot
 HW   │  ──────────── safety-critical core, HAL-free ─────────────  │   HW
```

UART and DShot contain hardware-specific implementations and are separate
from the flight core. The whole path is certified, but a change in the UART
or DShot DMA code must not force re-certification of the core flight group.

|  | software core (HAL-free, host-testable) | hardware layer (per chip family) |
|---|---|---|
| safety-critical | Command Input, control law, actuator authority, safety master and arming | UART RX DMA, SPI IMU, DShot/PWM DMA, control timer, watchdog |
| non-critical | OSD, blackbox, RPC, CLI, MSP | SPI NOR flash, USB, LEDs |

The core already turned out nearly HAL-free. Counting hardware references in
the safety-critical task definitions: `arming.rs` 0; `control.rs` 2 (the timer
tick acknowledgement); `actuator.rs` 2 (the `DshotMotors` type); `rc.rs` 2 (UART
buffer size, owned writer type); against `spi1.rs` 26 and `uart_port.rs` 12,
which are genuinely hardware. FerroForge tasks can be chip-agnostic: they cover
both software and hardware tasks, and a hardware task becomes chip-independent
through a trait implemented per chip or HAL.

**The direction rule.** A safety-critical crate never depends on a
non-critical crate; a non-critical crate may depend on a safety-critical one.
Cargo enforces it and `reqtool` checks it from `cargo metadata`. With it, "an
OSD change cannot affect safety evidence" holds by construction, not by
analysis. Criticality cannot drift: when safety-critical code needs something
non-critical, the check fails until that thing is deliberately reclassified,
moved, and given requirements.

**Mandatory linking.** In a safety-critical crate, linking flips from opt-in
to mandatory: `reqtool` fails if an item has no requirement. Elsewhere it
reports tasks without requirements, tests without requirements (as with
Command Input's tests) and requirements without a method.

**Naming the class.** The word appears in crate names, the schema, tool
output and the thesis, so collisions were checked across both repositories:

| Candidate | Collision | Verdict |
|---|---|---|
| `critical` | critical sections: `critical_section::Mutex`, `CriticalSectionSpiExecutor`, 48 uses, and the embedded-Rust `critical-section` crates | chosen: the term every outside reader (certifier, SORA, thesis examiner) knows; the overload is contextual |
| `assured` | 2 uses; DO-178C's "design assurance" | runner-up: no collisions, but needs defining for every reader and sounds like a finished status |
| `safe` | Rust's safe/unsafe | rejected |
| `safety` | the `ferrowasp-safety` crate; 576 uses | rejected |
| `flight` | the `ferrowasp-flight` crate, the `flight` level; 1,391 uses | rejected |
| `certified` | states a status nothing has | rejected |
| `trusted` | the plan's `trust: external`, nearly the opposite meaning | rejected |

The class is `critical`, always written "safety-critical" in prose. No new
code uses bare "critical" for anything else.

**Marking a crate.** The user reversed an initial proposal that marked the
non-critical side (`-periph`): the critical crate carries the marker, and
unmarked crates are the default. Three conventions were weighed:

| Option | Shape |
|---|---|
| A. suffix only on split pairs, criticality declared in `Cargo.toml` metadata (chosen) | `ferrowasp-stm32f4-critical` beside `ferrowasp-stm32f4`, but `ferrowasp-safety` keeps its name; `[package.metadata.ferrowasp] critical = true` is the source of truth; the tool checks names and metadata agree |
| B. suffix on every critical crate | `ferrowasp-safety-critical`; long names |
| C. the name as the only source | renaming a crate would silently reclassify it |

**The priority rule.** Every non-critical task runs below the border roles
(the coordinator, the observer and the Health Manager), which run below the
control path and the safety master (section 19); the parser checks
it. `esc_manager_task` is core (Actuator Feedback) and ran at 4, below
`usb_fs` (5); it runs at 8, above the border band (6 and 7) and below the
I/O watchdog (9). Each of its 2 ms runs is bounded by the bytes one period
brings, so it delays USB and the OSD by at most that run.

**Global critical sections.** `interrupt::free` and `critical_section` in
non-critical code block safety-critical tasks whatever the priorities. A lint
flags them in non-critical crates.

**The bus rule.** A bus carrying a safety-critical device carries no
non-critical device; board-data tests enforce it, and an exception needs the
user's approval. The Foxeer (IMU on SPI1, flash on SPI2) and the Lucid (IMU on
SPI1, SD card on SDMMC) comply. Some flight controllers on the market share
buses, which is why the rule is explicit.

---

## 9. Links between tasks

**Every link is an SPSC queue.** Every link between safety-critical tasks,
and from a safety-critical task to the observer, is a single-producer,
single-consumer queue. The links are secure communication channels: one valid
producer handle and one valid consumer handle, each an RTIC local resource of
exactly one task, which RTIC and FerroForge enforce through local-resource
rules. "One producer, one consumer" is a compiler guarantee, not a convention.
`ferrowasp-stm32-tasks/src/channels.rs` already used this pattern for the ESC
and storage queues (`heapless` queues split once at init).

**The message rule.**
- Every message carries a timestamp and a sequence number
  (`TimestampMicros` exists).
- An empty queue is not an error: `heapless::spsc`'s `dequeue()` returns
  `None`. Empty means "nothing new since the last read", not "stale" or
  "failed". Staleness is the consumer's decision.
- Each link declares its kind:
  - `state`: the consumer drains the queue, keeps the newest message and
    checks its age against the link's limit; too old is stale, and stale
    fails closed (IMU → control, RC → safety master);
  - `event`: every message is processed in order; a full queue is a counted
    fault (ESC requests and acknowledgements, the blackbox stream).
- On a full queue `enqueue` hands back the new item and keeps the old ones;
  the producer counts the rejection and never blocks (W2). A stalled consumer
  later drains, takes the newest, and the age check catches anything too old.
- Kind, depth and age limit are requirements for each link, depth derived
  from the producer and consumer rates.

The user confirmed that every link is a queue; single-slot latest-value cells
were not adopted.

**Atomic flags.** Flags remain for one-writer, many-reader, single-word state.
SPSC cannot broadcast: `SAFETY_ARMED` was read by `esc`, `usb`, `flash` and
the app.

| Option | Meaning |
|---|---|
| A. flags allowed, declared like links (chosen) | the writer handle is a local resource of one task, each reader handle a local resource of its task; single-word values only |
| B. queues only | one `state` queue per reader; uniform, but the writer's cost grows per reader |
| C. as before | global `static` atomics anyone can touch; invisible to the tool |

Rules under A:
- one writer per flag, enforced by the type system (split once at init into a
  writer and copyable readers, as SPSC splits). `core::safety::signals`
  already had typed `…Reader` and `…Writer` handles (`SafetyArmReader`,
  `MotorCmdWriter`), so part of this existed;
- single-word state only. Multi-field data spread over several atomics can
  tear: in `snapshots.rs`, a reader of `IMU_LATEST_ROLL/PITCH/YAW` could get
  roll from one sample and pitch from the next;
- state, not data: a flag has no timestamp;
- a flag cannot tell a reader its writer died. If the safety master stalls,
  `armed` stays true forever. A safety-critical task reads a flag only when
  the writer's liveness is covered elsewhere, by the watchdog or a heartbeat
  link; otherwise the value travels over a `state` queue;
- non-critical code never writes a flag safety-critical code reads; that
  direction goes through the coordinator.

**Shared resources across the boundary are retired.** `tuning_profile` was
an RTIC shared resource locked by `control_loop` (14), `rc_input` (10),
`osd_refresh` (3) and `flash_manager_task` (1). A non-critical task could hold
a lock whose ceiling is 14 and stop the control loop for that time, and the
OSD (through `tuning_request_seq`) and the flash manager could write any
tuning into the control loop. Under this design `tuning_profile` is state
owned by the control loop; the OSD and flash manager send requests and read
the active profile from the ledger.

The SPI1 mailbox between the IMU tasks and the SPI owner
(`critical-section::Mutex<RefCell<_>>`) becomes SPSC request and response
queues. The motor-command queue between the control loop and Actuator Output
was the first instance of this rule.

---

## 10. The observer and the ledger

The observer reads data from the safety-critical tasks and publishes it as a
uORB-like, read-only ledger. The OSD, telemetry and other non-critical tasks
read the ledger as a shared resource. Communication between safety-critical
tasks, and to the observer, uses SPSC links.

```
critical tasks ──SPSC──▶ critical tasks
      │
      └──SPSC──▶ observer ──publishes──▶ ledger (read-only topics) ◀──reads── OSD, telemetry, CLI, …
```

Why it fits the evidence model: safety-critical tasks never share a resource
with a non-critical task, only the producer end of a queue. The ledger's lock
ceiling is set only by the observer and its readers, which all run below
the core tasks, so no reader can block the control path or the safety
master, and path fingerprints can omit non-critical tasks by construction.

- **The producer side never blocks.** A safety-critical push never blocks,
  and drops and counts on a full queue, so a buggy or slow observer makes
  safety-critical tasks drop data, never stall. The policy is a contract
  requirement on the channel type. On that ground the observer was first
  non-critical; it became safety-critical when the Health Manager came to read
  the ledger (section 19).
- **Two contracts.** The SPSC message types (what critical tasks send) change
  the critical producers; ledger topics (what the observer publishes) change
  only the observer and its readers. The observer translates, so a new
  OSD value derived from data already sent changes nothing critical.
- **Two kinds of topic.** Latest-value topics serve the OSD and telemetry. A
  latest-value ledger loses samples, and the blackbox logs at 2 kHz, so queued
  stream topics serve the blackbox (uORB has queued topics for the same
  reason). Previously `control_loop` pushed straight into
  `flash_record_producer`, a critical-to-non-critical link that bypassed any
  observer. If the observer falls behind, the drop counter appears in the
  log: honest evidence, and the critical side still never stalls.
- **Topics carry a per-topic sequence**, which fixes the torn reads of
  separate atomics.

The ledger replaces `snapshots.rs`: about 30 global atomics that
safety-critical tasks wrote directly and anyone read.

**The mechanism**, decided by the user before implementation:

- **What `snapshots.rs` carried goes four ways.** Values leaving the core for
  display go to the ledger. Values between core tasks become links, flags or
  init values, because the core never reads a border role: the IMU sample
  to the control loop, a `state` link with gyro and accelerometer in one
  message (the control loop read its gyro as three atomics the parser
  writes at a lower priority, so a tick could mix two samples); the control
  loop's IMU status to the pre-arm check, a `state` link with an age limit,
  since nothing yet covers the control loop's liveness; the motor map and
  the DShot release time, flags; IMU readiness and kind, init values.
  Values non-critical tasks share among themselves also go through the
  observer. Inbound values (the accelerometer level, the host-connected
  flags, `tuning_profile`) wait for the coordinator.
- **The link type** and the `Stamped` envelope (sequence, timestamp) live in
  `ferrowasp-io-contracts`, under every link, including the hardware ones
  section 12 plans. A refused message still takes a sequence number, so the
  consumer reads the loss as a gap.
- **The ledger** is one shared resource. Its fields are private, getters
  return copies and only the observer module publishes, so a reader cannot
  write it. A pass-through topic is its message; a derived topic gets its
  own type, so a new derived value changes no producer.
- **The observer runs periodically**, every millisecond at priority 7.
  Producers only enqueue and never wake it; a run takes at most each link's
  capacity, so its cost is bounded. The control tick is an `event` link
  sized for 8 ms; every other link is a `state` link.
- **The blackbox stream carries control ticks.** The record format lives in
  non-critical `ferrowasp-protocol`, which a critical observer cannot depend
  on, so the observer forwards each tick with its stamp and the flash
  manager builds the record and applies the log rate divisor: the divisor
  is the blackbox's setting and needs no way in. The log's dropped count is
  the sum of both stages, so the format does not change.
- **Gates read flags, not the ledger.** The flash manager's disarmed-only
  checks and the ESC manager keep their arm-state flag readers, so a
  stalled observer can never show a gate "disarmed".
- **Priorities.** `esc_manager_task` is core and ran below `usb_fs` (5); it
  moves to 8, so the border band is 6 to 7 and no border role or
  non-critical task sits above core code.
- The heartbeat to the safety master comes with the fault classes, which
  it needs.

---

## 11. The coordinator

**Purpose.** The observer is the single way out for data; the coordinator is
the single way in. It is the trust boundary for everything non-critical code
can ask the safety-critical side to do, the role Command Input plays for RC.
Its five jobs:

1. **One place lists the inbound surface.** It is one typed `Request` enum in
   one task. "What can the OSD, the USB command line or the configurator make
   the flight side do?" is answered by reading one file. For certification:
   "non-critical code can influence critical state only through these N
   requests, each validated as specified."
2. **Owners receive only validated, typed values.** The USB command-line
   parser, OSD menu and configurator protocol are non-critical code. The
   coordinator checks well-formedness, ranges (`value_spec().accepts`), motor
   indices and profile consistency, so owner code never defends against a
   malformed request, and owner evidence does not depend on any requester.
3. **A misbehaving requester floods only its own queue.** Each requester has
   its own SPSC queue; the coordinator takes a bounded number per run, in
   turn, so USB cannot starve the OSD menu and the coordinator's timing cost
   is known.
4. **It arbitrates between requesters.** If USB and the UART command line both
   request a motor test, one bench session exists and the coordinator decides
   who holds it. The safety master sees one session and its lease.
5. **Requesters and owners change independently.** A new requester (MSP,
   Bluetooth, a second command line) adds one queue, and owners are
   unchanged. A new request type changes the coordinator and one owner.

**What it does not do.** It never judges flight state, never applies
anything, and never owns leases. It asks the owner of the state.

**Where it lives.** Three placements were weighed:

| Option | Shape | Verdict |
|---|---|---|
| a separate task that validates and routes, with owners applying (chosen) | requesters → coordinator → owner over `event` links | the safety master stays small; a new request type changes the coordinator and one owner, so the safety master's evidence stands |
| inside the safety master | it receives, validates, checks state and applies | no extra hop, but the safety master grows with every request type, and each new one re-owes it |
| no central entry point | each owner has its own queue from each requester | no single list of the inbound surface; requesters × owners queues |

**The transaction.** The user's model:

```
OSD ──▶ coordinator: SetTuning(id 17)
        coordinator: validate content
        coordinator ──▶ safety master: request arm inhibit
                        safety master: disarmed? grant : refuse(Armed)   ← the one atomic check
        coordinator ──▶ control loop: apply tuning
                        control loop ──▶ coordinator: ack
        coordinator ──▶ safety master: release inhibit
        coordinator ──▶ observer: accepted(17)
```

An earlier proposal had each owner check "disarmed" itself. That works for a
change with one owner and breaks for changes touching several (a motor map
touches mixer and actuator; an RC protocol touches RC input and the safety
master), where separate checks cannot make the whole change atomic. It also
had a check-then-act gap: a gate checking "disarmed" and then applying while
the safety master arms in between. The safety master's grant, held for the
whole change, is the one atomic check.

Refinements, all accepted (rules may be loosened later if the benefits
outweigh the risks):
1. "disable arm" means inhibit arming, never disarm: granted only while
   disarmed and not arming. A bug in the OSD menu can never disarm in flight;
2. one transaction at a time;
3. an inhibit carries a lease, as W1's motor test does;
4. changes with several owners stage, ack, then commit; any failure aborts
   with nothing applied;
5. owners still check "disarmed" when they apply, as cheap defence in depth.

**Two kinds of inhibit.**

| Inhibit | Lease lapses | Why |
|---|---|---|
| bench session (motor test) | motors stop, inhibit released, arming possible | nothing persistent changed; lapsing is the normal end of a test |
| config transaction | arming refusal latched until reboot | a partial config may exist; a stuck change must leave the craft unable to arm |

The operational cost of the latch: a failed config change in the field means
a power cycle.

**Results** reach the ledger through the observer with the request id. A
refusal at validation comes from the coordinator; an accept or refusal at
apply time from the owner. A requester never assumes success; with no result
within its limit it shows a timeout. A full requester queue drops the request,
and the requester times out.

**Host leases.** Arming is refused while a computer is attached (ARM-003).
Each link a computer can be attached by renews its own lease through the
coordinator: USB every 100 ms while a host has the board configured, for
500 ms, and the configurator link with each command, for 2 s. The safety
master holds the leases and publishes when the last one ends, for Actuator
Output's guard. A link that stops renewing releases arming by itself, so no
non-critical task writes what the arming guard reads; the Foxeer has no VBUS
sense pin, so a USB supply that only gives power does not count.

**Worked example: the motor test.** Before, `usb_fs` (non-critical) put
`SafetyEvent::BenchMotor` straight into the safety master's own event input,
beside `ArmRequested`, `DisarmRequested` and `RcLinkInvalid`, and
`validate_bench_motor_request` in `core/safety.rs` mixed content checks (motor
1-4) with state checks (armed, arming, arm switch, output enabled). Some of it
already matched: `Stop` was refused while armed ("the configurator has no
authority over a flying aircraft"), the actuator re-checked disarmed, and the
lease was firmware-held. Under the design:

```
configurator ─USB─▶ usb_fs ─SPSC─▶ coordinator ─event─▶ safety master ─event─▶ actuator
   (host)        (requester)        (content,          (owner: state,       (re-checks
                                     session)           lease, inhibit)      disarmed)
```

The link task translates the wire protocol into
`Request::MotorTest(Spin { motor })`; the host never sees the coordinator.
The safety master opens the bench session as an arm inhibit with a lease, each
repeated `Spin` renews it, and the motor stops when renewals stop. The safety
master's event input then comes only from safety-critical tasks and the
coordinator.

**Worked example: reboot.** Before, the flash manager (non-critical,
priority 1) handled `reboot` and `reboot dfu`, read `SAFETY_ARMED`, and called
`backend::system_bootloader::reboot` itself (`stm32-tasks/src/flash.rs`).
That is check-then-act by non-critical code, and it checked only armed, not
arming in progress, when motors already spin for idle qualification. Under
the design, the storage owner, which alone knows the store is settled, is
the requester:

```
CLI ─▶ flash manager: reboot dfu                (non-critical ↔ non-critical)
       flash manager: refuse while armed, maintaining or logging;
                      settle the store; answer; wait for the answer to leave
       flash manager ─▶ coordinator: Request::Reboot(RomBootloader)
         coordinator: from the storage owner? → forward, ticketed
         coordinator ─▶ safety master (participant):
             disarmed and not arming? → reset
             otherwise               → refused(Armed | ArmingInProgress)
```

Only the safety master can end the flight firmware; non-critical crates
cannot even name `reboot()`. No inhibit is needed: the check and the reset
run in one safety-master run, and arming is that task's alone. A write cut by a reboot is covered by the
two-slot scheme and boot validation (section 14).

**Naming.** The task was first called "the gate". Candidates were checked for
collisions:

| Name | Finding |
|---|---|
| gate | 140 uses: test gates, actuator gating in W1, arming guard order |
| dispatcher | RTIC's word for software-task interrupts; `dispatchers = [...]` in every `app!` |
| commander | PX4's arming state machine; a PX4-literate reader takes it for the safety master |
| broker | suggests two-way routing and no validation |
| intake | plain; says nothing of transactions or arbitration |
| warden | suggests it holds authority, which the safety master does |
| **coordinator** | chosen: the two-phase-commit term for what runs a transaction across participants, which is exactly the adopted protocol |

The owners it works with are its **participants**.
