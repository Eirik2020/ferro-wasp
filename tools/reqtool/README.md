# reqtool

FerroWasp's requirements and verification tool. Requirements are YAML in
`project_meta/requirements/`, derived from the governing requirements W1-W7
and allocated to items in the code. Status is computed, never written. The
design is [`project_meta/requirements/design.md`](../../project_meta/requirements/design.md),
with ADR-0050 to ADR-0053.

Requirements are defined, validated, linked to code, checked, and verified by
evidence records that carry a derived fingerprint. The old evidence system
(`tools/components.py`, `TEST_CATALOG.json`) stays in use until `reqtool` can
also import its records; until that switch, nothing real is recorded here.

## Commands

Run from anywhere inside the repository, or pass `--repository`:

```text
cargo run --release -- validate
cargo run --release -- check
cargo run --release -- check INPUT-001
cargo run --release -- check ferrowasp_flight::rc_receiver
```

- `validate` exits nonzero on any problem: schema, IDs, references, items
  that do not resolve, tests that are not `#[test]`, missing procedure
  headings, and verification modes without their reason or relations.
- `check` with no target summarizes every requirement. With an ID it shows
  that requirement; with `W1`-`W7` the requirements derived from it; with an
  item path every requirement allocated to that item, inside it, or around
  it. Agents run `check` on an item before changing it.
- `test` runs the host tests that requirements name (all, or those of the
  requirements given) with cargo, in each package's own workspace, and
  records the session. Tests that fail or do not run exit nonzero; passes
  already on record at the same fingerprint are not recorded again.
- `gate` lists the gates in `project_meta/requirements/gates.yaml`;
  `gate <name> --board <board>` says whether one is open and lists every
  entry in its way. It exits zero only when the gate is open. A gate names
  requirements (an ID, a prefix ending in `*`, or `*`) and the levels they
  must be verified at, and may include other gates.
- `import` carries the old schema 1 and 2 run records over as schema 3
  evidence, beside the originals, which stay unchanged. Each requirement
  entry naming the old test's procedure gets a result fingerprinted in a
  temporary worktree of the record's own revision; a revision not in this
  repository, or one whose items did not exist yet, gets an unavailable
  fingerprint, so its pass shows as stale. The old record's execution,
  candidate, conditions, artifacts, observations and limitations come with
  it. Re-running it changes nothing.
- `record` writes one run of a test or procedure: a result, at the current
  fingerprint, for every requirement entry that names that method. `mcu`,
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
cargo run --release -- record --test ferrowasp_flight::rc_receiver::tests::name --result pass
cargo run --release -- record --procedure "project_meta/testing/targets/foxeer-f405-v2.md#Heading" --board foxeer-f405-v2 --result pass
```

## Status

| Status | Meaning |
|---|---|
| `verified` | a pass at the current fingerprint |
| `stale` | a pass, but the fingerprint has changed since |
| `unverified` | no pass at all |
| `failed` | a fail at the current fingerprint; a later pass with nothing changed does not clear it |

A fail clears only when the code changes and a run passes, or when a record
`--supersedes` it with the user's `--ruling`. Board evidence is reported per
board; an entry takes the worst board with evidence.

A fingerprint covers every source file of the path crates in the dependency
closure (from `Cargo.lock`, dev-dependencies included), each registry crate's
checksum, the toolchain file, the requirement's statement and verification
entry (whitespace-insensitive), and a procedure's own section. Host evidence
takes the closure of the allocated and test crates; board evidence takes the
board's whole firmware app, until the `app!` parser allows a narrower path
scope. Records are JSON, schema 3, in `project_meta/testing/evidence/runs/`:
one record per session, each result naming its own method.

## A requirement

```yaml
schema_version: 1
requirements:
  - id: INPUT-001                 # <AREA>-<NNN>, stable, never reused
    title: Reject stale RC input
    statement: >-
      Command Input shall treat RC frames older than the link's limit as
      invalid and signal loss of link to the safety master.
    rationale: Stale input must never be acted on as a live command.
    derived_from: [W2]            # W1-W7 or parent requirement IDs
    allocated_to:                 # crate-qualified paths
      - ferrowasp_flight::rc_receiver
    verification:
      mode: direct                # direct | via_children | covered_by_parent | either
      entries:
        - level: unit             # analysis | unit | sitl | mcu | bench | flight
          target: host            # host | fcu | devboard
          setup: none             # none | sim | props-off
          method:
            test: ferrowasp_flight::rc_receiver::tests::name
        - level: bench
          target: fcu
          setup: props-off
          method:
            procedure: project_meta/testing/targets/foxeer-f405-v2.md#Heading
```

`via_children`, `covered_by_parent` and `either` need a `reason`;
`covered_by_parent` also needs `observed_by`, naming what in the parent's run
would show a failure. A child covered by its parent never counts towards that
parent. Unknown fields are refused, and there is no `status` field. A
`direct` requirement with no entries is valid: nothing verifies it yet, and
`check` reports it as a gap.

Paths resolve through modules, `use` re-exports and globs, plus one segment
after a type for an enum variant or a method in an `impl` in the type's own
module. Items made by macros are invisible; `#[cfg]` is ignored.
