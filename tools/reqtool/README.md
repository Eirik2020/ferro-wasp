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
- `record` writes one run of a test or procedure: a result, at the current
  fingerprint, for every requirement entry that names that method. `mcu`,
  `bench` and `flight` evidence needs `--board`. `--failed REQ` fails one
  requirement in a run that otherwise passed; `--dry-run` prints the record.

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
scope. Records are JSON, schema 3, in `project_meta/testing/evidence/runs/`.

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
