# Requirements Design: Schema, Tooling and Governing Requirements

Sections 6 to 7 of [the requirements design](design.md), which
maps every section to its chapter.

## 6. Schema, tooling and the parser

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
- group members are flat `pub use` re-exports of definitions in their own
  crates (`ferrowasp-stm32-app/src/groups.rs`), so a group holds each task
  name once and "crate plus task name" is unique: a duplicate is a compile
  error. The plan's claim that Rust's namespace rules guarantee
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
- The code that implements a requirement declares it, in the module doc
  comment of each source file ([section 20](tracing.md#20-tracing-per-source-file));
  the tool reports a declaration of an unknown ID.
- Requirement files live centrally, in
  `project_meta/requirements/<subsystem>.yaml`. Beside each crate was the
  alternative; it travels with a crate move, but every requirement edit would
  then change the crate directory.
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

**What requirement text holds.** A requirement says what must hold and why
it matters, nothing else:

| Field | Holds | Never holds |
|---|---|---|
| `statement` | one testable claim ("shall"), saying what must hold, in at most 60 words | how it is built, a path into the code, history, a second claim |
| `rationale` | why it matters: the hazard, or the governing requirement it serves, in at most 50 words | alternatives, who decided or when, evidence, plans |
| `verification` | how it is verified: a test or a procedure | results, which are evidence records |

A statement that needs two "shall" is two requirements. Where the rest goes:
the current structure in the spec section that owns the topic; why a
tempting alternative lost in [the FAQ](../faq.md); what is left in
remaining work; results in the evidence records; who decided, and when, in
git and the PR. `reqtool validate` lints requirement text and the FAQ
against these rules and reports what it finds; `--strict` fails on it.

The governance Markdown mirrors a requirement as prose: header, "shall"
statement, rationale, verification method, derived-from. A requirement:

```yaml
id: INPUT-002
title: Stale RC input invalidates the link
statement: >-
  The RC link shall become invalid when no healthy frame has arrived for
  more than RC_LINK_TIMEOUT_US (100 ms), and shall become valid again only
  after RC_LINK_RECOVERY_FRAMES (3) consecutive healthy frames.
rationale: Stale input must never be acted on as a live command.
derived_from: [INPUT-001]
verification:
  entries:
    - level: unit
      target: host
      setup: none
      method:
        test: ferrowasp_safety::tests::rc_link_timeout_holds_past_the_old_u32_wrap_and_forces_requalification
```

Each file that implements it lists it, with any other requirements it
implements, in its module doc comment:

```rust
//! Requirements: INPUT-002
//! Safety-critical.
```

**`reqtool`.**
- `check`: a compact verdict for a requirement or a source file — declared
  requirements, status per level and board, what is owed. Agents call it
  before editing.
- `record`: writes evidence.
- `report`: maturity for one configuration (TRACE-006).
- `validate` checks the per-file direction rule and that safety-critical
  files declare requirements (TRACE-001, TRACE-002).
- Later: `graph`, a view for people (a dependency graph, e.g. through
  Graphviz), and the other `analysis`-level checks (priority rule, names
  versus metadata, bus rule) once the parser lands.

**Evidence as implemented.** These choices were made while building
`record`, within the design above:
- A record is one run of one method. It carries a result for every
  requirement entry that names that method, each with its own claim and
  coverage, so one run can pass one requirement and fail another. An entry is identified by
  requirement, level, target, setup and method, not by its position in the
  file.
- A record also carries the git revision and whether tracked files had
  uncommitted changes; its identities cover the files as they were, so a
  dirty run is still exact.
- Coverage is derived as section 20 describes: host evidence from the files a
  run exercised and their direct imports, with the toolchain; board evidence
  from the configuration `cargo check` reports for the board and its
  features.
- A procedure contributes the text of its own section, so editing another
  section of the same file leaves the evidence standing.
- Board evidence is reported per board, and an entry takes the worst board
  that has evidence, until gates name the boards they require.
- `reqtool` reads only schema 4 records. Each schema 3 record was derived
  into a schema 4 one at its own revision, named `__retraced`, which lists
  what could not be derived; the schema 1, 2 and 3 records stay beside them
  untouched.
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
  does not run is never recorded as a pass, and passes already on record
  with the same coverage are not recorded again.

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
Deriving identities from git also covers the flown candidate: its record stays
tag `foxeer-candidate-16f6e8ed` plus the preserved binaries, never a rebuild.

---

## 7. Governing requirements

Two readings of "update the governing document to the new format" were
offered: (a) only reword W7, or (b) make W1-W7 the root requirements of the
new system. The user chose (b). W1-W7 stay prose Markdown in
`governing-requirements.md`, loaded into every session, with stable IDs; YAML
requirements reference them through `derived_from` (`CMD-INPUT-001`
`derived_from: [W2]`) and never copy them, which keeps one owner per topic.

W7 names no mechanism:

> 7. **W7 - Evidence.** A compile does not establish target behaviour, a
> target boot does not establish actuator behaviour, and props-off operation
> does not establish flight readiness; each claim needs evidence at its own
> level. Evidence verifies the requirements in `project_meta/requirements/`,
> each derived from W1-W7 or from another requirement. A record stands while
> the code and configuration it covered are unchanged, and status is computed
> from evidence, never recorded by hand.

How a record's coverage is established belongs to the requirements under it
(section 4, and section 20 once `reqtool` implements it), so a change of
method leaves W7 standing. W4 states criticality as a layering dimension
(section 8) the same way: "Code is safety-critical or not, and
safety-critical code never depends on non-critical code." `reqtool` reads W1-W7's IDs and titles from the governing
document itself, so it stays their one owner.
