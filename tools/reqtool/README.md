# reqtool

FerroWasp's requirements and verification tool. Requirements are YAML in
`project_meta/requirements/`, derived from the governing requirements W1-W7.
Source files declare the requirements they implement. Status is computed,
never written. The design is
[`project_meta/requirements/design.md`](../../project_meta/requirements/design.md);
[section 20](../../project_meta/requirements/tracing.md#20-tracing-per-source-file)
is the tracing model.

Requirements are defined, validated, declared by the code, checked, and
verified by evidence records that carry derived identities. Gates say what must be
verified before each step toward flight. The testing chapter,
[`project_meta/testing/README.md`](../../project_meta/testing/README.md), owns
the workflow.

## Commands

Run from anywhere inside the repository, or pass `--repository`:

```text
cargo run --release -- validate
cargo run --release -- validate --strict
cargo run --release -- check
cargo run --release -- check INPUT-001
cargo run --release -- check ferrowasp_input::rc_receiver
cargo run --release -- check crates/ferrowasp-input/src/rc_receiver.rs
cargo run --release -- report --board foxeer-f405-v2
```

- `validate` exits nonzero on any problem: schema, IDs, references, tests
  that do not resolve or are not `#[test]`, missing procedure headings, and
  declarations of unknown requirements. It also checks every
  safety-critical file: one that defines code declares a requirement
  (TRACE-001), and it imports only safety-critical files and external crates
  its manifest pins to one version (TRACE-002). It lints requirement text and `project_meta/faq.md` against
  [what requirement text holds](../../project_meta/requirements/schema-and-tooling.md#6-schema-tooling-and-the-parser)
  and prints each finding as a warning; `validate --strict` fails on them.
- `check` with no target summarizes every requirement. With an ID it shows
  that requirement and the files that declare it; with `W1`-`W7` the
  requirements derived from it; with a file, by path or by module path, the
  requirements it declares. Agents run `check` on a file before changing it.
- `report --board <board> [--features a,b]` reports maturity for one
  configuration (TRACE-006): every requirement the safety-critical files it
  compiles in declare, with its status there, and the safety-critical files
  that declare nothing. It runs `cargo check` for the board to find what is
  compiled in.
- `test` runs the host tests that requirements name (all, or those of the
  requirements given) with cargo, in each package's own workspace, and
  records the session. Tests that fail or do not run exit nonzero; passes
  already on record with the same coverage are not recorded again.
- `gate` lists the gates in `project_meta/requirements/gates.yaml`;
  `gate <name> --board <board>` says whether one is open and lists every
  entry in its way. It exits zero only when the gate is open. A gate names
  requirements (an ID, a prefix ending in `*`, or `*`) and the levels they
  must be verified at, and may include other gates.
- `record` writes one run of a test or procedure: a result, at the current
  code, for every requirement entry that names that method. `mcu`,
  `bench` and `flight` evidence needs `--board`. `--failed REQ` fails one
  requirement in a run that otherwise passed; `--dry-run` prints the record.
  `--details FILE` adds how the run was done (JSON): `execution`
  (`performed_by`, `user_confirmed`), `candidate` (`firmware_sha256`,
  `features`, ...), `conditions` (`propellers`, `actuator_power`,
  `equipment`, `environment`), `commands`, `artifacts` (paths under `logs/`;
  `reqtool` hashes them), `measurements`, `observations`, `limitations` and
  `stop_conditions_triggered`. Board evidence must have it: user-confirmed,
  not agent-only, with the image's SHA-256 and a known power and propeller
  state; a props-off pass needs propellers removed and a flight pass needs
  them installed; and no pass may include a triggered stop condition.

```text
cargo run --release -- record --test ferrowasp_input::rc_receiver::tests::name --result pass
cargo run --release -- record --procedure "project_meta/testing/targets/foxeer-f405-v2.md#Heading" --board foxeer-f405-v2 --result pass
```

## Status

| Status | Meaning |
|---|---|
| `verified` | a pass whose coverage matches the current code |
| `stale` | a pass, but what it covered has changed since |
| `unverified` | no pass at all |
| `failed` | a fail whose coverage matches; a later pass with nothing changed does not clear it |

A fail clears only when the code changes and a run passes, or when a record
`--supersedes` it with the user's `--ruling`. Board evidence is reported per
board; an entry takes the worst board with evidence.

Every result carries its claim: the requirement's ID and statement
(whitespace-insensitive), the entry, and a procedure's own section.

- **Host evidence** covers the modules a run exercised: a test's file and the
  files it imports directly, each with its own direct imports, at their
  identities, and the toolchain. A source file's identity ignores whitespace
  and comments; an external crate's is its locked version. A host entry
  counts for a file that declares its requirement while the claim, the
  file, its direct imports and their identities are unchanged; a requirement
  no file declares takes what its runs exercised.
- **Board evidence** carries the configuration it ran in: every repository
  file `cargo check` reports compiled in (dep-info and build-script reruns),
  the app's manifest and `.cargo/config.toml`, external crate versions,
  compiler-read environment variables other than those a build script sets,
  and the toolchain. It counts for a configuration whose identity is the
  same.

Records are JSON, schema 4, in `project_meta/testing/evidence/runs/`: one
record per session, each result naming its own method. Schema 4 records
ending `__retraced` were derived from schema 3 ones at their own revisions,
and list what could not be derived.

## A requirement

```yaml
schema_version: 2
requirements:
  - id: INPUT-001                 # <AREA>-<NNN>, stable, never reused
    title: Reject stale RC input
    statement: >-
      Command Input shall treat RC frames older than the link's limit as
      invalid and signal loss of link to the safety master.
    rationale: Stale input must never be acted on as a live command.
    derived_from: [W2]            # W1-W7 or parent requirement IDs
    verification:
      entries:
        - level: unit             # analysis | unit | sitl | mcu | bench | flight
          target: host            # host | fcu | devboard
          setup: none             # none | sim | props-off
          method:
            test: ferrowasp_input::rc_receiver::tests::name
        - level: bench
          target: fcu
          setup: props-off
          method:
            procedure: project_meta/testing/targets/foxeer-f405-v2.md#Heading
```

A requirement is verified by its own entries only; a parent is never proved
through its children. Unknown fields are refused, and there is no `status`
field. A requirement with no entries is valid: nothing verifies it yet, and
`check` reports it as a gap.

The code that implements it says so in its module doc comment, and a
safety-critical file says that too:

```rust
//! Requirements: INPUT-001
//! Safety-critical.
```

Paths resolve through modules, `use` re-exports and globs, plus one segment
after a type for an enum variant or a method in an `impl` in the type's own
module. An import that does not resolve to one file widens: a glob to every
file it re-exports, a macro to the files defining it, a type or trait to the
files implementing it. Paths inside macro invocations are read from their
tokens. Items made by macros are invisible; `#[cfg]` is ignored.
