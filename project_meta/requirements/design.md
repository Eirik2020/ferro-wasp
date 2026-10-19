# Requirements, Evidence and Safety Architecture: Design

This document records the design of FerroWasp's requirements and verification
system, and the firmware architecture that system depends on: why each
decision was made, which alternatives were rejected and why, and what in the
code motivated it. The binding records are ADR-0050 to ADR-0060
([index](../ARCHITECTURE_DECISIONS.md)); this document carries the reasoning
behind them. Where the two differ, the ADR governs and this document is
corrected.

Nothing described here is implemented yet. The code findings quoted below
describe the code the design was made against, as motivation; they are not a
description of the target.

## Contents

1. [Goals and approach](#1-goals-and-approach)
2. [The requirements and evidence model](#2-the-requirements-and-evidence-model)
3. [Verification levels and gates](#3-verification-levels-and-gates)
4. [Fingerprints](#4-fingerprints)
5. [Verification modes and timing](#5-verification-modes-and-timing)
6. [Schema, tooling and the parser](#6-schema-tooling-and-the-parser)
7. [Governing requirements](#7-governing-requirements)
8. [Safety-critical and non-critical code](#8-safety-critical-and-non-critical-code)
9. [Links between tasks](#9-links-between-tasks)
10. [The observer and the ledger](#10-the-observer-and-the-ledger)
11. [The coordinator](#11-the-coordinator)
12. [Hardware contracts](#12-hardware-contracts)
13. [The gyro-synced control loop](#13-the-gyro-synced-control-loop)
14. [Stored configuration](#14-stored-configuration)
15. [Classifications](#15-classifications)
16. [Crate layout](#16-crate-layout)
17. [Order of work](#17-order-of-work)
18. [Open items](#18-open-items)

---

## 1. Goals and approach

The system must give machine-checkable traceability between requirements,
code and test evidence. It must be cheap for agents to query, with no silent
drift, and reviewable by people. It is kept deliberately loose until the
master's thesis and the FerroWasp 1.0 milestone, and then solidified.

From the starting plan (`ferrowasp-requirements-plan.md`, in the workspace
root), these points carried through unchanged:

- three documentation layers: user documentation and governance rules are
  Markdown prose; requirements and verification data are YAML (deep nesting:
  function → task → task group → groups of groups, with per-chip variation),
  with CSV for flat data such as bring-up checklists;
- agents edit raw files, and people review diffs against the original
  wording, so drift in paraphrase is caught;
- schema enforcement through Rust `serde` structs with `deny_unknown_fields`
  and enums for constrained fields, run in CI, so no field can be invented;
- third-party crates are recorded as `trust: external`, pinned to an exact
  version, and contract-tested at their interface;
- the tool is Rust (fast startup for frequent agent calls, one binary, shared
  types and toolchain), a library with a thin CLI, in this repository;
- the first three goals: a minimal `reqtool` and schema; the Command Input
  requirement, tagging its existing tests and exposing its validation gap;
  the Foxeer F405 V2 bring-up as the first board in the bring-up matrix;
- later: an MCP server (via `rmcp`) as separate, composable requirements,
  developer and user servers; a YAML-to-ReqIF converter if a certifier needs
  exchange; an optional web view. ReqIF is never the working format.

**Clean sheet.** The design is new, not an extension of the system it
replaces: `COMPONENTS.json`, `TEST_CATALOG.json`, the schema 2 run records and
`tools/components.py` (ADR-0038). Old parts are reused only where they earn it
on their own merits. What survives: W7's principle that evidence stands while
what it covered is unchanged; procedures as Markdown, because people run them;
raw artifacts kept in `logs/` with records holding only path and hash.

The discussion that produced this design turned, from its first case, into a
redesign of the firmware's structure, because a requirement has to be
allocated to something, and the structure it would be allocated to was the
obstacle. The two tracks are kept apart in the work that follows (section 17).

---

## 2. The requirements and evidence model

ADR-0050.

**Primitives.**

| Primitive | What it is | Who produces it |
|---|---|---|
| Requirement | what shall be true | written, in YAML |
| Item | a task definition, function, module or board data | found by parsing |
| Method | how a requirement is checked | unit tests found by parsing; SITL, bench and flight procedures written |
| Evidence | one run of a method, with a result per requirement | written only by `reqtool record` |
| Fingerprint | the identity of what the evidence covered | derived by the tool, never listed by hand |

A run can pass some requirements and fail others; old records carried one
result per test.

**Status is computed, never stored.** `reqtool` computes status for each
requirement, verification entry and variant from the links, the evidence
records, the current fingerprints and the verification mode.

| Status | Meaning |
|---|---|
| `verified` | a pass exists whose fingerprint matches the current code |
| `stale` | a pass exists, but its fingerprint no longer matches |
| `unverified` | no evidence at all |
| `failed` | the newest evidence at the current fingerprint is a fail |

`stale` and `unverified` are kept apart because "re-run this, it passed
before" is different work from "this has never been tested".

Why computed:
- the starting plan stored `status: unverified` in the YAML, which needed an
  `invalidate` command to cascade changes. A stored status is wrong the moment
  someone forgets to invalidate. A computed status cannot drift, and there is
  no `invalidate` step to forget;
- agents never edit status, so the plan's rule that agents edit YAML only with
  explicit per-instance permission no longer needs to cover status fields.

**How a failure clears.** A fail at the current fingerprint is cleared only by
a pass at a new fingerprint, or by a superseding record on the user's explicit
ruling that cites the fail. A pass after a fail with nothing changed does not
clear it: a flaky result counts as failed. This keeps the existing evidence
rule (never change a result to make a gate appear complete), and matches how
the BENCH-FOX-001 scope change was handled: a superseding record on the user's
ruling, not an edited result.

**No expiry.** Evidence does not age out. Unchanged code needs no re-testing.

**Who writes what.** People and agents write requirements. `reqtool record`
writes evidence. Nobody writes status. Gates read status.

**Why ADR-0038's registry was replaced.** It tied evidence to hand-listed
components, and the person writing a run record listed which components the
run exercised. That is unsound: a record for `driver-mpu6000` that does not
also list `drivers/lib.rs` keeps passing after a change to `lib.rs`. Its goal
stands — unchanged code is never re-owed, on any board — and the mechanism
moves to derived fingerprints (section 4).

---

## 3. Verification levels and gates

ADR-0051.

The plan's levels (unit, SITL, MCU, bench, flight) did not match the old six
tiers (software, embedded-build, bench-unpowered, bench-powered-props-off,
preflight, flight). They merge into one enum, ordered by cost and risk:

| Level | Meaning | Replaces |
|---|---|---|
| `analysis` | tool checks on source and images: dependency direction, the priority rule, stack, RAM and flash, the image building for its board and features | `embedded-build`, part of `software` |
| `unit` | host tests, including conformance suites against mocks and golden fixtures | `software` |
| `sitl` | software-in-the-loop on the host, once a harness exists | none |
| `mcu` | on the flight controller, nothing actuating (the plan's MCU: the FCU itself, not a generic devboard) | `bench-unpowered` |
| `bench` | the full drone, ESCs powered, propellers removed (not peripheral simulators) | `bench-powered-props-off` |
| `flight` | in flight | `flight` |

A level is a field on each verification entry, not a separate document
hierarchy. One requirement can need several levels: arming-timeout logic at
`unit` and `sitl`, real ESC timing at `bench`.

`target` (`host`, `fcu`, later `devboard`) and `setup` (`none`, `sim`,
`props-off`, later rigs) exist from the start, even with few valid values, so
devboard and peripheral-simulator stages are added as new values, not as a
migration. With MCU meaning the FCU only, MCU evidence is per board, not per
chip, so "verify once per chip" savings do not apply until a devboard stage
exists.

**Preflight is a gate, not a level.** A level says how a requirement was
verified. Preflight says whether this exact image and configuration may fly
now: the readback matches, the required path evidence is current, the
variants are covered. It consumes evidence; it does not verify requirements.

**Gates** are named checkpoints: merge, bench session, preflight, flight.
Each declares which requirements, at which levels and for which variants,
must be `verified` before it opens. **Only gates block.** Missing evidence
never blocks development; this is what keeps evidence that needs equipment
not yet available (a timing devboard) from stopping work.

W7 still applies at every level: a compile does not establish target
behaviour, a target boot does not establish actuator behaviour, props-off
operation does not establish flight readiness.

---

## 4. Fingerprints

ADR-0052.

A fingerprint answers: which code has to stay the same for this evidence to
still count? The tool derives it from what the evidence claims to verify:
evidence verifies requirements, requirements are allocated to items, and the
fingerprint covers those items and what they depend on. A bench run proving
"stale RC rejected" then survives an OSD change without anyone listing
anything.

**How the dependency scope was chosen.** Three ways to compute "what an item
depends on" were weighed:

| Option | Soundness | Granularity |
|---|---|---|
| crate closure: the item's crate and every crate it depends on, from `cargo metadata` | cannot miss a change | coarse |
| item call graph via rust-analyzer or rustc | precise | heavy and fragile |
| hand-listed files (the old registry) | can miss a change | fine |

In the code as it stood, crate closure was effectively the whole image.
`safety_master` lived in `ferrowasp-stm32-tasks`, which also held OSD, RC,
USB, flash, DShot and ESC code; it depended on `ferrowasp-flight`, which held
OSD, the log writer and RPC beside arming; both depended on
`ferrowasp-drivers`, which held every driver. An edit to `osd.rs` or a CRSF
fix would have made the safety master's bench evidence stale.

Module-level tracking was considered and found blocked by
`ferrowasp-stm32-tasks/src/prelude.rs`: every task, including `arming.rs`,
`actuator.rs`, `control.rs` and `dshot.rs`, imported `crate::prelude`, which
re-exported `osd`, `flash_storage`, `esc_manager`, `mspv1`, every IMU driver
and the chip backend. On paper the safety master depended on the OSD.

The finding was that the dependency problem was mostly poor crate structure:
safety-critical code was intertwined with non-critical code. The fix is
structural (section 8): once safety-critical crates depend only on
safety-critical crates, crate closure becomes both sound and fine-grained for
exactly the code where it matters.

**Why crates and not modules carry the boundary.** A module split was
reconsidered. It can work, but its guarantees then rest on tools FerroWasp
writes itself:

| Need | Crate split | Module split |
|---|---|---|
| critical code cannot use non-critical code | guaranteed by Cargo: a crate can only use what its `Cargo.toml` lists | nothing in Rust stops `use crate::usb_serial`; `pub(crate)` cannot express the rule; only a custom lint |
| the fingerprint covers what the evidence depends on | crate closure, sound by construction | module analysis with conservative fallbacks for glob imports, macros, trait impls and shared `lib.rs` items |
| churn | higher | lower |

A concrete risk in the existing backend: `ferrowasp-stm32/src/memory.rs`
holds DMA buffer definitions that UART, SPI and storage may all use. In one
crate nothing forces anyone to notice a critical and a non-critical module
starting to share one; across crates it needs a wrong-direction dependency,
which fails the check. Because fingerprints are derived from source, a
granularity change can be recomputed from git for every old record, so the
choice is reversible without losing evidence.

**Scopes.**

| Scope | Fingerprint |
|---|---|
| unit or core | the item's crate and dependency closure, the toolchain, and the locked external versions |
| hardware conformance, per MCU family | the `-critical` backend crate, `ferrowasp-io-contracts`, and the chip |
| path | every segment crate on the path, the board data crate, the safety-critical part of the RTIC scheduling table (priorities, bindings, link graph, read from `app!`), and the variant values |

The path scope exists because some evidence depends on timing and
scheduling, not only on code. On the 2026-10-09 bench no IMU code changed: a
UART priority change starved `spi1_parser`, a stale IMU tick disarmed the
craft, and no code-dependency analysis would have caught it. The scheduling
table is in the path fingerprint for that reason. RTIC's scheduling model
narrows it: a task's timing depends only on tasks that can preempt it and on
resources whose ceiling is at or above its priority. With the priority rule
(section 8), no non-critical task can preempt a safety-critical one, so path
fingerprints never include non-critical tasks.

**Requirement text is part of the fingerprint.** Editing a requirement's
statement or its verification entry makes its evidence `stale`: the claim
changed, so the old evidence no longer proves it.

**Image-level checks.** Some properties come from linking everything
together, so no crate fingerprint covers them. RTIC tasks share one stack:
deeper stack use in a non-critical task adds to the worst case when
safety-critical tasks preempt it, and an overflow faults the whole firmware.
Stack, RAM and flash usage are therefore checked for each board image in CI,
at the `analysis` level. They are cheap and automatic and never need bench
evidence. Tool choice is left to implementation.

**Variants.** Runtime-selected options change which code runs: the RC
protocol, the actuator-feedback source, serial bindings, the loop rate. A
bench pass with UART telemetry does not count for bidirectional DShot.
Variants are declared once (`rc_protocol: [crsf, sbus]`,
`telemetry_source: [dshot, blheli_uart]`). A verification entry lists the
variants it covers, omitted meaning variant-independent, and coverage is
reported for each variant a board can run.

---

## 5. Verification modes and timing

ADR-0052.

The plan's cascade rule stands: a safety-critical parent is satisfied by its
own requirement and test, or by full coverage from its children, never both
restating the same thing. Restatement (the same "-60 to +60" at ten nested
levels) is an anti-pattern. Aggregation (a car at 100 km/h proven through
independently tested wheels, engine and frame) is legitimate decomposition.
The plan's node rule also stands: a child gets its own tracked entry only if
more than one parent reuses it; a one-off child rolls up into its parent.

The user added the opposite direction: when the parent is verified, it can
verify its children. Four modes result:

| Mode | Direction | Requires |
|---|---|---|
| `direct` | the requirement's own evidence | none |
| `via_children` | up: the children are jointly sufficient | a stated reason |
| `covered_by_parent` | down: the parent's run covers the child | a named observable and a reason |
| `either` | either of the above | none |

`via_children` needs someone to assert that the children are jointly
sufficient, which covers interaction and integration risk. The tool cannot
judge that; it is a systems-engineering judgement, recorded as the reason.

`covered_by_parent` covers only what the parent's run actually exercised and
observed. A timing budget is observed, because an overrun counter would show
a failure. "Stale RC is rejected" is not, because a normal flight never sends
stale RC. Fault-path children always need their own evidence. The tool
cannot judge observation either, so the link names the observable:

```yaml
verification:
  mode: covered_by_parent
  observed_by: loop_overrun_counter
  reason: path bench run fails on any overrun
```

**Timing.** The first proposal was a measured execution-time budget per
safety-critical task (DWT cycle counter), so path timing could be
compositional: a change to a preempting task would re-owe only its own budget
measurement. The user's constraint was that this must not deadlock
development, because timing measurement needs a dedicated devboard. The
resolution is `either`: prove the parent path runs as expected, or prove each
child's budget is within tolerance.

Until a devboard exists, timing is covered from the parent by counters every
bench and flight run records: queue drops, stale samples and, with gyro-sync,
loop overruns (a sample arriving before the previous loop finished). A
non-zero count on a safety-critical link fails the run. The cost is that
timing is not compositional while covered from the parent: a change to any
safety-critical task on or above a path re-owes that path's timing evidence.
When the devboard stage arrives, budgets get direct evidence and
compositionality returns without a redesign. (The DWT counter also works on
the flight controller itself over SWD, whose pins stay reserved on the
Foxeer; that is an option if a dedicated devboard is not otherwise wanted.)

An explicit deferral marker (`deferred: { until: devboard }`) was considered
and dropped: `covered_by_parent` made it unnecessary.

**Two rules found while implementing.**
- A child covered by its parent takes its status from that parent, so it never
  counts towards the parent's `via_children` or `either` status; otherwise each
  would wait on the other. A `via_children` or `either` requirement therefore
  needs at least one child that verifies itself.
- Under `either`, a failing own entry is never outvoted by passing children: a
  failure stands until it is cleared as section 2 describes.

---

## 6. Schema, tooling and the parser

ADR-0053.

**Parsing the firmware.** `main.rs` is a single `ferroforge::app! { … }`
invocation. To `syn` it is one `ItemMacro` with opaque tokens, so plain `syn`
sees no tasks. The `app!` grammar parser lived in `ferroforge-macros`
(`app.rs`, `group.rs`), a proc-macro crate with `pub(crate)` types that no
host tool can link. `ferroforge-contracts` exists for "FerroForge macros and
host tools" but parsed only `#[ferroforge::task]` definitions.

| Option | Cost | Risk |
|---|---|---|
| A. move `app!` and `#[group]` declaration parsing into `ferroforge-contracts` (chosen) | needs a FerroForge release first | low: macros and tools share one parser |
| B. `reqtool` keeps its own `syn` copy of the grammar | medium | drifts silently when the grammar changes |
| C. keep extending the Python regexes in `app_context.py` and `check_rtic_boundaries.py` | cheap | fragile |

A follows W3: what FerroWasp needs from FerroForge is built in FerroForge. It
is released on crates.io before `reqtool` depends on it.

What the parse yields:
- every task names its definition with `from =`, for example
  `from = flight_tasks::uart1_rx_dma` or a group's
  `from = flight_tasks::groups::flight`;
- group members are flat `pub use crate::x` re-exports
  (`ferrowasp-stm32-tasks/src/groups.rs`), so task definitions share one
  namespace per crate, and "crate plus task name" is unique: a duplicate is a
  compile error. The plan's claim that Rust's namespace rules guarantee
  uniqueness holds for tasks, not for arbitrary functions;
- an instance and its definition can differ: `dshot_motor1_dma_complete` to
  `motor4` are four instances of `dshot_dma_complete` with different config.
  Requirements attach to the definition; the instance carries the board's
  binding;
- tasks exist only under some features (`bench_bidirectional_dshot`; `flight`
  itself under `dshot`), so the parse keeps `#[cfg]` information, or coverage
  would be reported against tasks a flight image does not contain;
- every link and flag handle is a declared local resource (section 9), so the
  parse yields the whole communication graph.

Plain functions are not visible from `main.rs`. The task list from `app!` is
mandatory scope; functions are opt-in, named by a requirement and checked for
existence.

**Schema.**

- IDs are `<AREA>-<NNN>` (`SAFE-001`, `INPUT-012`, `CTRL-004`), stable, never
  reused, and not tied to crate names, so moving a crate does not rename a
  requirement. W1-W7 are the roots.
- `allocated_to` uses crate-qualified paths: a task by its definition
  (`ferrowasp_safety::safety_master`), a function or module by its path
  (`ferrowasp_input::command_input::validate`). The tool resolves each by
  parsing and reports dangling ones. The plan's idea of finding a
  requirement by naming its file after the item was dropped: hundreds of tiny
  files, and renames would pass silently instead of showing as dangling.
- Requirement files live centrally, in
  `project_meta/requirements/<subsystem>.yaml`. Beside each crate was the
  alternative; it travels with a crate move, but every requirement edit would
  then change the crate directory, and the fingerprint calculation would have
  to exclude the file.
- Gates are declared in one file. Procedures stay Markdown, referenced as
  `path#heading`.
- Evidence is schema 3 records under
  `project_meta/testing/evidence/runs/YYYY/MM/`, written by `reqtool record`.
  Raw artifacts stay in `logs/`.
- Schema types are `serde` with `deny_unknown_fields`; `schemars` can emit a
  JSON Schema from the same types. `serde_yaml` is archived; the YAML crate is
  `serde-saphyr`: pure Rust, panic-free parsing, and errors that point into the
  file, which matters most for a tool agents call. `serde_yml` is deprecated,
  and `serde_norway` keeps `serde_yaml`'s C-translated `unsafe-libyaml`.
  Evidence records stay JSON, since only the tool writes them.
- A method is `test: <crate-qualified path>` (a `#[test]` function, found by
  parsing) or `procedure: <path>#<heading>` (a Markdown heading that must
  exist). Each level fixes its target and setup: `analysis` and `unit` run on
  the host with no setup, `sitl` on the host in simulation, `mcu` on the FCU
  or a devboard with no setup, `bench` on the FCU or a devboard props-off, and
  `flight` on the FCU. `unit` takes a test; `mcu`, `bench` and `flight` take a
  procedure.

The governance Markdown mirrors a requirement as prose: header, "shall"
statement, rationale, verification method, derived-from. The plan's
illustrative requirement:

```yaml
id: CMD-INPUT-001
title: Reject stale RC input
statement: >
  Command Input shall treat RC frames older than <N> ms as invalid
  and signal loss-of-link to the Safety Master.
rationale: Stale input must never be acted on as a live command.
derived_from: [W2]
allocated_to: [ferrowasp_input::command_input]
verification:
  mode: direct
  entries:
    - level: unit
      target: host
      setup: none
    - level: sitl
      target: host
      setup: sim
```

**`reqtool`.**
- `check`: a compact verdict for an item, component or board — allocated
  requirements, status per level and variant, what is owed. Agents call it
  before editing.
- `record`: writes evidence.
- `report` and `graph`: views for people (a YAML tree or dependency graph,
  e.g. through Graphviz).
- Later: `analysis`-level checks (direction rule, priority rule, names versus
  metadata, bus rule) once the parser lands.

**Evidence as implemented.** These choices were made while building
`record`, within the design above:
- A record is one run of one method. It carries a result for every
  requirement entry that names that method, each with its own fingerprint, so
  one run can pass one requirement and fail another. An entry is identified by
  requirement, level, target, setup and method, not by its position in the
  file.
- A record also carries the git revision and whether tracked files had
  uncommitted changes; the fingerprint covers the files as they were, so a
  dirty run is still exact.
- The fingerprint is taken from the governing `Cargo.lock`, which lists every
  crate in the closure, dev-dependencies included. Path crates contribute
  every file git tracks or would track under their directory, leaving out
  nested packages and build output; registry crates contribute their version
  and checksum. The nearest `rust-toolchain.toml` is included.
- Until the `app!` parser exists, board evidence takes the board's whole
  firmware app as its closure. That is coarse but cannot miss a change, and
  because fingerprints are derived, every such record can be re-fingerprinted
  with the narrower path scope once it exists.
- A procedure contributes the text of its own section, so editing another
  section of the same file leaves the evidence standing.
- Board evidence is reported per board, and an entry takes the worst board
  that has evidence, until gates name the boards they require.
- `reqtool` reads only schema 3 records and leaves the schema 1 and 2 ones
  beside them untouched.
- A record carries `details`, what each procedure's "Required evidence" asks
  for: execution, candidate image, conditions, commands, artifacts,
  measurements, observations, limitations and triggered stop conditions.
  The old run-record validator's hardware rules apply to it, checked when
  recording and when reading: no pass with a triggered stop condition; board
  evidence user-confirmed, not agent-only, with the image's SHA-256 and a
  known power and propeller state. Two rules follow from the setups: a
  props-off pass needs propellers removed, a flight pass installed.
  Artifacts live under `logs/`; `reqtool` hashes them and re-checks the hash
  whenever the file is present.
- `reqtool test` runs the host tests requirements name, per package in its
  own workspace and toolchain, and records one session; a test that fails or
  does not run is never recorded as a pass, and passes already on record at
  the same fingerprint are not recorded again.

**Tooling transition.**

| Tool | Fate |
|---|---|
| `components.py`, `COMPONENTS.json` | retired; fingerprints are derived, and `due` becomes `reqtool check` |
| `test_evidence.py` | retired; `reqtool record` writes and validates evidence |
| `TEST_CATALOG.json` | dissolved: stop conditions and required evidence stay in each procedure's Markdown, which they duplicated; who may run a procedure follows from its level under the AGENTS.md Operating Rules; prerequisites became the gates in `gates.yaml`; trigger paths are dropped because what is owed is computed |
| `RUN_RECORD_TEMPLATE.json` | replaced by schema 3 records and their `details` |
| schema 1 and 2 run records (16) | 13 imported once, re-fingerprinted at each record's own revision with their details, as `<run_id>__imported.json`; the software-suite and preflight records were not, by design; originals unchanged. The one-off `import` command was removed with the catalog it read |
| `bench_rig.py`, `foxeer_bench_gates.py` | kept; they judge their gates as before and record through `reqtool record` (`reqtool_bridge.py`) |
| `check_repository_context.py` | kept for document checks; its catalog and run-record checks removed |
| `check_rtic_boundaries.py` | kept until the parser lands; its mechanical checks then move into `reqtool` |
| `app_context.py` | kept; it routes agent context, a different job, names the gate chain in place of the catalog chain, and can adopt the parser later |

Evidence is never recorded under two systems at once: the old one was
retired as a whole, in one change, once `reqtool` could record, import and
gate.
Re-fingerprinting from git also covers the flown candidate: its record stays
tag `foxeer-candidate-16f6e8ed` plus the preserved binaries, never a rebuild.

---

## 7. Governing requirements

ADR-0053.

Two readings of "update the governing document to the new format" were
offered: (a) only reword W7, or (b) make W1-W7 the root requirements of the
new system. The user chose (b). W1-W7 stay prose Markdown in
`governing-requirements.md`, loaded into every session, with stable IDs; YAML
requirements reference them through `derived_from` (`CMD-INPUT-001`
`derived_from: [W2]`) and never copy them, which keeps one owner per topic.

W7 is reworded, because "evidence is tied to component IDs" named the retired
mechanism. As landed with the first schema:

> 7. **W7 - Evidence.** A compile does not establish target behaviour, a
> target boot does not establish actuator behaviour, and props-off operation
> does not establish flight readiness; each claim needs evidence at its own
> level. Evidence verifies the requirements in `project_meta/requirements/`,
> which derive from W1-W7, and carries a fingerprint the tooling derives from
> the code they are allocated to, or from the path where behaviour depends on
> timing or scheduling. A record stands while its fingerprint is unchanged,
> and status is computed from evidence, never recorded by hand.

W4 gains criticality as a layering dimension (section 8): "Each crate is
safety-critical or not, and a safety-critical crate never depends on a
non-critical one." `reqtool` reads W1-W7's IDs and titles from the governing
document itself, so it stays their one owner.

---

## 8. Safety-critical and non-critical code

ADR-0054.

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

**The priority rule.** The observer and every non-critical task run below the
lowest safety-critical task, checked by the parser. Moving `esc_manager_task`
(4) above `usb_fs` (5) is a priority change, which states a timing rationale
and is revalidated.

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

ADR-0055.

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

ADR-0010's SPI1 mailbox (`critical-section::Mutex<RefCell<_>>`) becomes SPSC
request and response queues. ADR-0015's motor-command SPSC boundary was the
first instance of this rule.

---

## 10. The observer and the ledger

ADR-0056. The design is the user's.

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
every safety-critical task, so no non-critical task can block a
safety-critical one, and path fingerprints can omit non-critical tasks by
construction.

- **The producer side is the safety-critical part.** A safety-critical push
  never blocks, and drops and counts on a full queue. With that guaranteed, the
  observer itself is non-critical: a buggy or slow observer makes
  safety-critical tasks drop data, never stall. The policy is a contract
  requirement on the channel type.
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

---

## 11. The coordinator

ADR-0056.

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
the design:

```
CLI ─▶ flash manager: quiesce storage          (non-critical ↔ non-critical)
CLI ─▶ coordinator: Request::Reboot(Firmware | RomBootloader)
         coordinator ─▶ safety master (participant):
             disarmed and not arming? → hold arm inhibit → reset
             otherwise               → refused(Armed | ArmingInProgress)
```

Only the safety master can end the flight firmware; non-critical crates
cannot even name `reboot()`. A write cut by a reboot is covered by the
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

---

## 12. Hardware contracts

ADR-0057.

**Principle.** Traits inside a task group let a device be swapped; messages
between a group and the core mean nothing has to be swapped.
`ferrowasp-io-contracts` holds only message types, and the core depends on no
hardware trait. A trait earns its place only where a task calls into a device
during its own run; since every link is a queue, hardware groups produce
messages and the core consumes them.

A contract is more than its types: behaviour such as "RX reports a
discontinuity on overrun" is part of it. Each contract therefore carries its
own requirements, verified by one shared conformance suite run against every
implementation. Porting to a new chip family means passing the conformance
suite on it, with no core re-test.

**Serial RX.** The user asked what serial RX is for. It is the UART box in
the path: it hands the core the received bytes plus what the core needs to
judge whether they can be trusted, which only the hardware layer can know.
`RxChunk` (ADR-0011) already carried it:

| Field | Why the core needs it |
|---|---|
| `bytes`, `len` | the data, in order |
| `timestamp` | arrival time, from which Command Input measures a frame's age |
| `discontinuity_before` and cause (`QueueOverflow`, `DmaError`, `FramingError`, `PortReconfigured`, `TransportReset`) | bytes were lost: discard a partial frame and resync, never join bytes across a gap |
| `uart_error_seen` | a noise or framing error, a corruption signal separate from the protocol CRC |
| `generation` | the port was reconfigured; partial data from before is invalid |

How chunks are produced is chip-specific (F4: DMA stream, IDLE-line interrupt,
error flags; H7: different DMA and cache maintenance). Every serial consumer
uses the same contract: RC (CRSF, SBUS), actuator feedback over BLHeli UART,
the CLI and configurator, GPS later. Serial RX became a message contract, not
a trait. The `SerialRxMailbox` (a `Deque` behind `critical_section::Mutex`,
shared between interrupt and reader task) becomes an SPSC link, which removes a
global critical section from the RC path.

**IMU.** The IMU has two kinds of hardware: the MCU (how bytes move: SPI, DMA,
EXTI) and the sensor (what the bytes mean: registers, scaling, FIFO, faults),
which vary independently. The user's chain:

```
IMU driver crate        FerroWasp IMU trait        IMU task group               core
(embedded-hal SPI) ───▶ (one impl per sensor) ───▶ (generic over I: Imu) ─SPSC─▶ consumes ImuSample
                                                    data-ready, scheduling,
                                                    timestamp, sequence
```

The MCU dependence sits only under the driver, in the HAL's
`embedded-hal(-async)` `SpiDevice` implementation, DMA-backed per family. The
data-ready pin can be `embedded_hal_async::digital::Wait`, so the group is
chip-agnostic too.

| Unit | Verified by | Re-owed by |
|---|---|---|
| each IMU's trait implementation | host tests against a mock `SpiDevice` (`embedded-hal-mock`) with golden register dumps; the trait's conformance suite (decode, faults, ID check); one MCU-level run on any board with that sensor | that driver or the trait |
| the IMU task group | unit tests with a mock `Imu`; path evidence | the group or the trait |
| MCU SPI and DMA | `SpiDevice` conformance per MCU family | that backend |
| core | unit and SITL on `ImuSample` | the core or `ImuSample` |

This delivers the user's 2026-10-12 aim: the MPU-6000 driver is proven once and
stays proven on every board while unchanged. An external driver crate is
`trust: external` and verified through the same conformance suite.

`ImuSample` (in `io-contracts`) carries gyro and accel in typed units in the
sensor's own frame plus a sensor id; `data_ready_at` (the edge time, not the
read-completion time, which is where clock drift shows); a sequence number
(gaps, separate from staleness); per-axis saturation (a clipped gyro reading
is not a valid rate); and a fault (not responding, ID mismatch, transfer
error). Rotation into the body frame stays an explicit core step using the
board's orientation data: AGENTS.md forbids a sign change hidden in a sensor
or mixer layer (ADR-0025). The trait and group live in `ferrowasp-imu`, so a
trait change re-owes drivers and group, never the core. IMU → core is a
`state` link.

**Actuator feedback** (renamed from ESC telemetry; safety-critical because
eRPM qualifies arming):

|  | BLHeli/KISS over UART | bidirectional DShot |
|---|---|---|
| transport | a separate wire | the DShot wire, reversed after each frame |
| rate | round robin, one motor per request over the telemetry bit | every frame, every motor |
| delivers | eRPM, temperature, voltage, current, consumption | eRPM; temperature, voltage, current only with EDT, interleaved |
| decoder | `drivers::blheli_telemetry` | `waveform::dshot_reply` (GCR) |

No trait: bidirectional decode must live in the DShot output group, which
owns the pin and turns it around; BLHeli decode lives in an ESC-telemetry
group over `RxChunk`. What they share is the output, `ActuatorFeedback`:
physical output index (logical mapping is the W6 motor map, applied
explicitly in the core), eRPM, observed-at, sequence, source, decode-fault
counts. Rules:
- one declared source per quantity, chosen at boot by ADR-0048's stored
  setting, fixed until reboot, never switched automatically. Switching
  between UART and DShot telemetry is possible by changing the setting and
  rebooting; boot validates the choice against the board and fails closed if
  invalid. This follows "never falling back" and "diagnostic features never
  replace [the motor path] silently";
- the producer splits by criticality: eRPM goes to the core; temperature,
  voltage and current go straight to the observer. Over-temperature or
  current protection would move them into the core message deliberately.

**Motor output.** Before, `actuator_output` (15) locked the shared
`dshot_motors` and wrote throttles through `apply_with_lease`. Under the
design actuator authority sends messages to the output group:

- `MotorCommand`: per physical output `Stop` or `Throttle(normalized)`, with
  issued-at and sequence. `Stop` is distinct from `Throttle(0)` (DShot 0 means
  disarmed, 48+ is throttle; PWM has its own minimum); the core says what it
  means and each protocol encodes it;
- `DshotCommand`: direction, beep, EDT enable, save; accepted only while every
  output is `Stop`, which the output group also checks;
- the output group makes the final freshness check: if its newest command is
  older than the maximum age, it outputs `Stop` on every output whatever
  happened upstream. It is a conformance requirement ("a stopped command
  stream stops the motors within N ms") and the last link of stale-command
  containment (W1, W2);
- the queue adds a hop; control-to-wire latency becomes a measured path
  requirement, so the hop is shown acceptable, not assumed.

**Protocol crates.** The user's requirement: CRSF, SBUS and BLHeli telemetry
are each their own crate following the embedded-hal / embedded-io standards,
so they work outside FerroWasp, behind traits like the IMU's (an RC protocol
trait in `ferrowasp-input`, an ESC-telemetry trait in `ferrowasp-feedback`).
Each has golden-frame tests plus the trait's conformance suite: valid frames,
the failsafe flag, frame lost, out of range, corrupted, and a split across a
discontinuity.

- The RC protocol is chosen at boot (`rc_protocol`), unlike the board-fixed
  IMU, so the group needs runtime dispatch without allocation. The existing
  `RcReceiver` enum (`ferrowasp-flight/src/rc_receiver.rs`:
  `Sbus(sbus_rs::StreamingParser)` or `Crsf { parser, .. }`, producing a
  common `RcFrame` of 16 channels and the failsafe and frame-lost flags) is
  that dispatch type.
- `sbus-rs 0.1.3` is external: pinned, `trust: external`, verified by the
  conformance suite. CRSF and BLHeli telemetry become standalone crates.
- Discontinuities: `embedded-io` has no concept of lost bytes. ADR-0011
  already carries them out-of-band (a `DiscontinuityReader`), because `Read`
  alone cannot prove nothing was lost before a valid-looking frame. Each
  parser therefore has `push` and `reset`; the RC group observes the
  out-of-band handle and calls `reset`, discarding any partial frame. Turning
  discontinuities into read errors was considered and not adopted, in favour
  of ADR-0011's design.

The SBUS pipeline: UART → frame assembly → SBUS parser → Command Input →
safety master / control loop / actuator authority. Command Input is the first
fully owned boundary and the place for defensive validation (range,
staleness, failsafe corroboration). It mostly trusted its input, with unit
tests that had no requirement behind them; goal 2 writes that requirement.

---

## 13. The gyro-synced control loop

ADR-0058.

Before, `control_loop` was bound to the TIM4 interrupt at
`control_loop_rate_hz` and began with `acknowledge_control_tick` through a
`TimerTick` trait bound, the one place a core task called into hardware during
its own run. The timer ran independently of the IMU's data-ready, which caused
the 2026-10-09 problem: the ICM-42688-P runs about 1% fast against the 2 kHz
timer, the two beat every ~89 samples, and a tick finds no new sample.
Tolerance versus gyro-sync had been an open user decision since then.

| Option | Driven by | Trade-off |
|---|---|---|
| A. timer (before) | TIM4 tick | fixed rate; the beat needs a stale-tick tolerance (`IMU_TOLERATED_STALE_TICKS`), and the core keeps a hardware trait |
| B. gyro-synced (chosen) | each new `ImuSample` | no beat, no tolerance; rate follows the sensor clock, so it is ~1% off nominal |

Why B:
- `io-contracts` becomes pure message types: the tick trait was the core's
  last hardware trait;
- fail-closed by construction: if the IMU stops, no samples arrive, the loop
  sends no `MotorCommand`, and the output group's freshness check stops the
  motors. Stale IMU data cannot produce motor output;
- a control-loop heartbeat over a `state` link lets the safety master disarm
  and report "control loop not running". This is also the writer-liveness
  cover section 9 requires before safety-critical tasks read flags.

The user's condition was that Betaflight uses gyro-sync. Its scheduler locks
to the gyro's data-ready interrupt and runs the PID loop at gyro rate ÷
`pid_process_denom`; this is from knowledge of its source and is confirmed in
the Betaflight code at implementation.

**Rates.** Gyro-sync gives the sensor's output rate divided by an integer.
Per the datasheets as recalled, not yet checked:

| Sensor | Output rates | 500 Hz | 1 kHz | 2 kHz | 3 kHz | 8 kHz |
|---|---|---|---|---|---|---|
| MPU-6000 / 6500 | 8 kHz internal (low-pass off) ÷ (1 + `SMPLRT_DIV`): 8k, 4k, 2.67k, 2k, 1.6k, 1.33k, 1k, … | yes | yes | yes | no | yes |
| ICM-42688-P | 32k, 16k, 8k, 4k, 2k, 1k, 500, 200, … | yes | yes | yes | no | yes |

Betaflight has the same constraint, which is why it offers 2.67 kHz, not
3 kHz. Running the sensor faster than the loop (8 kHz sampling, every fourth
sample for 2 kHz) lets the samples in between be filtered at full rate. A
timer-driven loop with the sensor at least twice the loop rate would also
avoid stale ticks at any rate, with sample-age jitter of up to one sensor
period, at the cost of keeping the tick trait and an explicit stale check;
since 500 Hz to 8 kHz in the divider set suffice, gyro-sync was chosen.

**The rate rule** (the user's refinement): the configured rate is the
sensor's output rate ÷ N, at or below the board's highest timing-tested rate
(execution time does not depend on the rate, so every lower rate inherits the
timing evidence), and at or above a minimum the core declares for its filters
and control. Rate-dependent logic is unit-tested at every rate the sensor
offers: filter coefficients and PID `dt` from measured sample intervals, not
`control_loop_rate_hz`; freshness limits that scale; the RPM filter's Nyquist
limit at low rates (at 500 Hz nothing above 250 Hz can be notched, so filtering
happens at the sensor rate before decimation). At 8 kHz the whole loop must
finish within 125 µs on the F405; that is timing evidence, not something a
config value can promise.

Costs: a behaviour change to the flight path and loop rate (W6), so props-off
checks and a flight re-gate; and the loop's priority stays above the IMU
group's work, stated as a path requirement.

---

## 14. Stored configuration

ADR-0059.

Storage is untrusted input; validation is the trust boundary, the same pattern
as the SBUS parser behind Command Input.

```
BOOT     flash driver ─raw page─▶ init: decode → validate(config, board) → select slot or default
         (non-critical)                (safety-critical, HAL-free, host-tested)

RUNTIME  CLI / configurator / OSD ─Request─▶ coordinator: validate(config', board)
                                               → live keys: arm inhibit → participants stage/commit → release
                                               → boot-only keys: stored as "pending until reboot"
         save ─▶ flash manager saves the config owner's next-boot image, read from the ledger
```

`StoredConfig` held tuning (gains, rates), `log_rate_divisor`,
`serial_bindings`, `rc_protocol` and `accel_level`. Tuning flies the craft and
the RC protocol decides whether RC works, so the values matter; the storage
mechanism need not be safety-critical.

| Safety-critical (core, HAL-free) | Non-critical |
|---|---|
| page decode and schema migration | SPI NOR driver |
| `validate(config, board)` | save sequencing (`config_save`) |
| slot selection at boot, and fallback | CLI, configurator and OSD front ends |
| the config owner (active and pending images) | |
| participants applying live keys | |

**One `validate()`** runs at boot and in the coordinator, so a change the
coordinator accepts can never be rejected at the next boot (the craft working
until its next power-up would be a nasty failure). It checks each value's spec
and the rules across fields and against the board (from the host-tested board
data crate): the motor map is a permutation; the loop-rate rule; the
actuator-feedback source is valid for the board; RC is bound and no UART is
bound twice; the RC protocol is supported on its port.

**Saving bypasses the coordinator.** Saving changes nothing safety-critical;
it copies validated state into storage. The config owner publishes the
next-boot image (live keys as active, boot-only keys as pending) as one
ledger topic with a sequence number. The command line asks the flash manager
directly (non-critical to non-critical); it writes that image with the
existing two-slot scheme (erase the spare slot, program, read back, adopt only
on a match) and refuses while armed (blackbox logging already writes flash in
flight, so this keeps config changes on the ground, not timing). A requester
asks for "save", never supplies bytes, so it cannot persist an unvalidated
config; if the save path corrupts the page, boot decode and `validate()`
reject it. The save path needs only ordinary non-critical tests (slot scheme,
readback, power loss mid-save).

**Boot fallback.**

| Case | Boot does | Arming |
|---|---|---|
| fresh board: both slots erased | board default | allowed, expected on first boot |
| newest slot damaged (CRC or decode), older valid | older slot, the pilot's previous save | refused until acknowledged |
| decodes but fails `validate()` (e.g. a firmware update lowered the tested loop rate) | older slot, then default; never a clamped value, which would be a guessed state (W2) | refused until acknowledged |
| older schema version | migrates; new keys take defaults | allowed, reported |

A silent fallback could fly different gains, or a different motor map (a flip
on takeoff). Of three options (refuse on any fallback; refuse on damage or
validation failure only; never refuse, Betaflight-like), the second was
chosen. The safety master owns the refusal; the pilot clears it with
`AcknowledgeConfigFallback` through the coordinator, disarmed only. It recurs
on every boot until a good save overwrites the damaged slot; the two-slot
scheme writes the slot not in use, which is the damaged one, so one save fixes
it.

**`ferrowasp-config`** is its own safety-critical, HAL-free crate: schema,
value specs, `validate()`, page encode and decode, migration, and the
motor-map type (used by the mixer, by actuator authority and the bench test,
and validated by config). Decoding untrusted bytes is the trust boundary, so
the page format cannot live in the non-critical protocol crate: a
safety-critical crate cannot depend on a non-critical one. The flash manager
and `ferrowasp-protocol` depend on it.

---

## 15. Classifications

ADR-0054.

| Item | Class | Reason |
|---|---|---|
| actuator feedback (formerly ESC telemetry) | safety-critical | eRPM qualifies arming; the RPM filter and dynamic idle feed control |
| stored config | split | safety-critical gate, non-critical store (section 14) |
| attitude estimator and `accel_level` | safety-critical | stabilized mode, which needs an angle estimate, is next |
| `system_reset`, `system_bootloader` | safety-critical | code that can end the flight firmware; only the safety master calls it |
| `adc` (battery voltage and current) | non-critical for now | display, RC telemetry and the blackbox only; a low-voltage failsafe or arming check would reclassify it |
| `pwm_config` (servo PWM) | non-critical for now | unused on a quad |

**The estimator.** `accel_level.rs` itself said "the rate loop never reads
the accelerometer, so this changes the estimated attitude, and nothing that
flies the craft today". The estimate (`imu_angle_integrator`) ran inside the
control loop, read only by the OSD horizon and blackbox. Making it
non-critical (calibration and estimator on the non-critical side, reclassified
when angle mode arrives) was proposed; the user chose safety-critical now,
because stabilized mode is coming soon. Consequences: the estimator stays in
the control loop with its own requirements; accelerometer calibration moves
from the flash manager into the safety-critical core as a coordinator
transaction (arm inhibit; the estimator's owner applies ADR-0044's stillness,
lean and magnitude checks to its own samples; stage the offsets; commit;
release); `ImuSample`'s accel fields become safety-critical; the OSD reads
attitude from the ledger. `gyro_bias_calibrator` was always safety-critical,
because the rate loop subtracts gyro bias.

The one estimator requirement now: in rate mode, an estimator fault must not
affect motor output. Angle-mode fault handling is decided when angle mode is
built; further detail now is scope creep.

---

## 16. Crate layout

ADR-0060.

**Splitting the core.** Bench evidence decides granularity, because unit
evidence costs nothing to re-run.

| Option | Shape | Verdict |
|---|---|---|
| A. one core crate | `flight-core` | any core change re-owes all safety-critical bench evidence |
| B. along bench-evidence lines (chosen) | `safety`, control, `input`, `feedback` | a tuning or filter change leaves the arming evidence standing, and the reverse |
| C. one crate per module | about a dozen | finest invalidation, little gain over B |

The user then split control into general control and flight-controller-
specific code:

| `ferrowasp-control` (general, reusable maths) | `ferrowasp-flight-control` (how FerroWasp flies) |
|---|---|
| PID | control loop task |
| biquad, low-pass and notch filters | rate profiles and expo, the tuning profile |
| notch-bank maths for the RPM filter | mixer and how the motor map is applied |
| attitude estimator algorithm | dynamic idle, throttle handling |
| gyro bias estimation, accel-level calibration maths | RPM filter wiring: which harmonics feed which notches |
| frame rotation maths | stale-sample handling, rate-dependent setup |

The general crate changes rarely and is proven almost entirely by unit tests;
flight and bench evidence attach to the flight-controller crate.

**Where tasks live.** Task code is target-only: `ferrowasp-stm32-tasks`
depended on `rtic` with `thumbv7-backend`, `cortex-m` and `defmt`, while the
logic crates are host-tested.

| Option | Shape | Verdict |
|---|---|---|
| A. tasks in their subsystem crate behind a `tasks` feature (chosen) | `#[cfg(feature = "tasks")] mod tasks` | fewest crates; a subsystem is one unit of evidence; tests cover both feature sets, as the one-entry-point runner already does |
| B. a paired `-tasks` crate per subsystem | `ferrowasp-safety` + `ferrowasp-safety-tasks` | twice the crates |
| C. one task crate per class | `ferrowasp-tasks-critical`, `ferrowasp-tasks` | an IMU-group wiring change would re-owe the arming path |

AGENTS.md's rule still holds as written: a task definition wires resources,
locks and spawns; each decision it makes is a host-tested function. The
prelude goes away, and each task imports only what its own crate depends on.

**Contract crates.** The count evolved. First two boundaries were seen
(hardware ↔ core, critical ↔ non-critical); then four (chip ↔ core;
critical → observer; requesters → coordinator; firmware ↔ host, found
because FerroConfigurator depended on `ferrowasp-core` by path for protocol
and blackbox types). Re-owing unit evidence is free, so crate boundaries follow
the expensive bench-evidence boundaries, giving three crates:

| Crate | Holds | Class |
|---|---|---|
| `ferrowasp-io-contracts` | hardware message types | safety-critical, minimal: a change costs bench runs on every chip |
| `ferrowasp-boundary-contracts` | `Request`, results, messages to the observer | safety-critical |
| `ferrowasp-protocol` | wire protocol, blackbox format | non-critical, W6; the configurator depends on this, not the core |

Combining the two safety-critical contract crates was the alternative; a new
telemetry field would then re-owe the hardware traits. Config keys first went
to `ferrowasp-protocol` and moved to `ferrowasp-config` (section 14).
`ferrowasp-core` mixed contract types with `safety.rs` (1,193 lines of logic),
`blackbox.rs` and `config.rs`; `io-core` held implementations (the serial
reader and writer, over 500 lines each, the SPI mailbox and async device), so
neither was a contract crate as it stood.

**Chip backends.** Each of `ferrowasp-stm32` (shared), `ferrowasp-stm32f4`
and `ferrowasp-stm32h7` mixed critical and non-critical hardware.

| Critical hardware | Non-critical hardware |
|---|---|
| motor output: `dshot_bank`, `dshot_lanes`, `dshot`, `timer_dma` | USB: `usb_serial` |
| SPI and IMU: `spi_common`, `spi`, `spi1_imu`, `spi_dma`, `exti` | status LED |
| UART: `uart_common`, `uart_port`, `uart_dma` | storage: `app_storage`, `sd_storage` |
| timing: `clocks`, `timebase`, `watchdog` | `adc` |
| routing: `board_routes`, `board_manifest`, `memory` | |
| `system_reset`, `system_bootloader` | |
| `scheduler`, `timer_tick` (deleted with gyro-sync) | |

Each splits into a `-critical` crate and the unmarked non-critical one. A
module split was the alternative (section 4). The expensive evidence is the
per-family conformance suite for UART, SPI and DShot, which a USB or SD change
must not invalidate.

**The map.**

Safety-critical (`critical = true`):

| Crate | Holds | From |
|---|---|---|
| `ferrowasp-io-contracts` | `RxChunk`, `ImuSample`, `ActuatorFeedback`, `MotorCommand`, `DshotCommand` | parts of `core`, `io-core` |
| `ferrowasp-boundary-contracts` | `Request`, results, outbound messages | new |
| `ferrowasp-config` | schema, `validate()`, page format, migration, motor map | `core::config`, part of `core::blackbox`, `flight::flash_storage` |
| `ferrowasp-safety` | safety master, coordinator, arming, actuator authority | `core::safety`, `flight::arming`, `flight::actuator` |
| `ferrowasp-control` | general control (above) | `pid`, parts of `flight::drone_toolbox`, `accel_level`, `rpm_filter`, `core::frames` |
| `ferrowasp-flight-control` | flight-controller-specific (above) | the rest of `drone_toolbox`, `dynamic_idle`, `core::actuator` |
| `ferrowasp-input` | Command Input, RC protocol trait, RC group | `flight::rc_receiver` |
| `ferrowasp-feedback` | actuator feedback core, ESC-telemetry trait and group | `flight::esc_manager` |
| `ferrowasp-imu` | IMU trait and group | `stm32-tasks::spi1`, `io-core::spi` |
| per-sensor IMU crates | one embedded-hal driver each | `drivers::mpu6000`, `mpu6500`, `icm42688p` |
| CRSF, SBUS, BLHeli-telemetry crates | standalone embedded-hal/io protocol crates | `drivers::crsf`, `drivers::blheli_telemetry`, `sbus-rs` (external) |
| `ferrowasp-output` | DShot and PWM output groups, waveform encode, bidirectional decode | `waveform`, `stm32-tasks::dshot` |
| `ferrowasp-serial` | UART transport group, RX reader, TX writer | `io-core::serial`, `stm32-tasks::uart_port` |
| `ferrowasp-stm32-critical`, `-stm32f4-critical`, `-stm32h7-critical` | the critical backend modules | the backends |

The serial TX writer is safety-critical because the OSD and telemetry write
through the same transport that may carry RC; a port's function is chosen at
boot from `serial_bindings`, so the transport code is safety-critical for
every port, and the observer separates the consumers (the OSD encoder feeds a
TX queue and never touches the transport).

Non-critical: `ferrowasp-observer` (observer, ledger topics; replaces
`stm32-tasks::snapshots`), `ferrowasp-protocol`, `ferrowasp-blackbox` (log
writer, blackbox storage, sync ledger, storage maintenance),
`ferrowasp-storage` (flash manager, config save, NOR storage, SPI NOR),
`ferrowasp-cli` (command link, RPC, storage commands, USB debug),
`ferrowasp-osd`, `ferrowasp-mspv1`, `ferrowasp-mspv2`, `rc-pwm`, and the
non-critical `ferrowasp-stm32`, `-stm32f4`, `-stm32h7`.

Dissolved: `ferrowasp-core`, `ferrowasp-io-core`, `ferrowasp-flight`,
`ferrowasp-drivers`, `ferrowasp-pid`, `ferrowasp-waveform`,
`ferrowasp-stm32-tasks`.

Names are working names until each crate is created.

---

## 17. Order of work

1. Record the decisions: ADR-0050 to ADR-0060 and this document, on the
   `requirements-system` branch. The governing-document edits (W1-W7 IDs, W7,
   W4) land with the first schema on that branch.
2. Goal 1: a minimal `reqtool` and schema — define a requirement, link it to
   an item, `check` it — coexisting with the old system.
3. The `app!` parser in `ferroforge-contracts`, and a FerroForge release.
   `reqtool` can start with function paths and add tasks when the release is
   out.
4. The architecture redesign, as its own stack of changes on `main`. It
   changes the flight image and re-gates the Foxeer, and it is kept apart from
   the requirements tooling.

Then goal 2 (the Command Input requirement and its tests) and goal 3 (the
Foxeer F405 V2 bring-up matrix and the first test of shared-driver
verification).

Deferred, from the plan and this design: the devboard and peripheral-simulator
stages; formal substitution and risk-tailoring rules (EASA SORA, AADL, SysML),
during flight-termination and flight-controller work; MCP servers; ReqIF
export; the web view; the Betaflight-target converter, its board trust tiers
(certified, manufacturer-supported, legacy; imported boards start unverified),
and a legal review of it (GPLv3 source: extract facts into an intermediate
structure and generate FerroWasp's own format, never copy text) before it
ships. The converter stays in the open fork; a professional or defence fork
keeps its own hardware list and never ingests imported targets.

---

## 18. Open items

- **USB-host arming refusal.** ADR-0042 refuses arming while a USB host is
  connected or within 2 s of a configurator-link command, by reading flags
  written by non-critical tasks (`USB_HOST_CONNECTED`,
  `CONFIGURATOR_LINK_ACTIVE`). That breaks the flag rule. It moves either to
  an inhibit lease each link holds through the coordinator or to a
  safety-critical VBUS sense; decided during the redesign.
- **Discontinuity handling in the RC group.** The RC group observes
  ADR-0011's out-of-band handle and calls the parser's `reset` (`rc_input`);
  its fault decisions move into host-tested functions on the
  `rc-input-fault-decisions` branch, which INPUT-006 links once merged.
- **Feature-gated tests.** `reqtool test` runs each package with its default
  features, so a test behind an opt-in feature cannot be evidence yet.
- **Datasheet rates.** The MPU-6000 and ICM-42688-P output rates above are
  recalled, not checked.
- **Betaflight's scheduler.** Gyro-locking is to be confirmed in its source.
- **Stack-analysis tooling** for the image-level check, chosen at
  implementation.
- **Document sizes.** On the requirements branch the user set the size
  limits aside to avoid lossy compression: the ADR index limit is disabled in
  `check_repository_context.py`, and this document, `SUMMARY.md` and
  `AGENTS.md` have raised budgets. The limits are restored once the documentation has moved
  into the requirements system.
