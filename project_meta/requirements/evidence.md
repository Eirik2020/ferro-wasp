# Requirements Design: The Evidence Model

Sections 2 to 5 of [the requirements design](design.md), which
maps every section to its chapter. The ADRs it names are the binding
records; this chapter carries the reasoning behind them.

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

ADR-0052. Replaced by [section 20](tracing.md#20-tracing-per-source-file)
once `reqtool` implements it; until then this is how evidence is
fingerprinted.

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

ADR-0052. The verification modes are dropped by
[section 20](tracing.md#20-tracing-per-source-file) once `reqtool`
implements it.

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
