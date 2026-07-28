# FerroWasp Test System

This directory is the entry point for selecting software, target, preflight,
and flight tests. Start here when the user asks to test firmware, prepare a
target, run a bench procedure, or perform a flight.

Do not load every historical test record. `TEST_CATALOG.json` routes an agent
to the procedure relevant to the selected target and validation tier.

## Definitions And Evidence

Keep these separate:

```text
Test definition = what must be tested and what evidence is required
Test-run evidence = what was tested, on which image, when, and with what result
```

A prior pass does not remove a test requirement. Dated results, image hashes,
logs, measurements, and operator observations belong in the evidence system
or a historical handoff, not in the catalog.

Use `EVIDENCE_INDEX.md` to locate retained historical test evidence. Do not
load an immutable baseline unless a specific provenance question requires it.

## Validation Tiers

Tests progress from lower-risk evidence toward target operation:

1. `software` — formatting, lint, host unit tests, analysis-tool tests, and
   simulation where available;
2. `embedded-build` — exact target, feature, and release checks without
   claiming hardware behavior;
3. `bench-unpowered` — target observation without ESC or actuator power;
4. `bench-powered-props-off` — powered actuator testing with propellers
   removed;
5. `preflight` — exact-image configuration and safety gates immediately before
   a proposed flight;
6. `flight` — a bounded operator-executed flight procedure.

Software and build evidence do not establish target behavior. Bench evidence
does not establish flight readiness.

## Invocation Workflow

When software changes:

1. identify affected paths, boards, features, protocols, and safety behavior;
2. select matching catalog entries by target, tier, path trigger, and feature;
3. run applicable software/build tests;
4. add or update a test definition when the behavior, risk, or required
   evidence changed;
5. report remaining manual gates without claiming they passed.

When the user requests bench or flight testing:

1. establish the exact board, firmware revision/image hash, enabled features,
   propeller state, actuator-power state, and available equipment;
2. load only common entries plus the selected target's entries;
3. reject entries marked `needs-review` until their referenced procedure has
   been reconciled with `../CODEX_ACTIVE_WORK.md` and current code;
4. order tests by prerequisite and validation tier;
5. present the applicable checklist and stop conditions;
6. obtain explicit user confirmation before powered or flight activity;
7. record results separately with the required evidence fields.

The agent may run software-only checks. The user remains the operator for
hardware, powered-actuator, and flight procedures.

## Recording Results

For new runs, use `evidence/README.md` and create one bounded JSON record under
`evidence/runs/YYYY/MM/`. Keep raw artifacts under ignored `logs/` paths and
reference only their path, size, and SHA-256.

Do not append results to procedures, the catalog, current-work handoffs, or one
cumulative evidence file. Do not backfill historical runs merely to populate
the record directory.

Run-record validation is included in:

```text
python tools/check_repository_context.py
```

## Safety Rules

- No test may bypass arming, actuator gating, failsafe, watchdog, health, or
  command-freshness checks.
- Propellers remain removed unless an explicitly reviewed flight test requires
  them.
- Powered and flight tests fail closed when their target, image, feature set,
  prerequisites, or evidence capture cannot be established.
- A `needs-review` entry is routing information, not authorization to execute
  the referenced procedure.
- Stop conditions take priority over test completion or evidence collection.
- Host, compile, bench, and flight evidence must be reported as distinct tiers.

## Catalog Status

- `active` — the definition may be selected when its prerequisites are met.
- `needs-review` — an existing procedure is indexed, but it mixes dated state
  or has not yet been migrated into the test system.
- `retired` — retained only for identity or supersession; never selected for a
  new run.

Target procedures are promoted topic by topic after reconciliation with live
state and code. Promoted definitions remain prerequisites to run, not evidence
of a pass or authorization to operate hardware.

## Adding A Test

Add one catalog entry with:

- a stable ID;
- target and validation tier;
- path, feature, or invocation triggers;
- a procedure path and exact Markdown heading;
- prerequisites;
- whether user execution is required;
- stop conditions appropriate to its tier;
- evidence fields needed to evaluate the run.

Then run:

```text
python tools/check_repository_context.py
python -m unittest tools.tests.test_repository_context -v
python -m unittest tools.tests.test_test_evidence -v
```

The context checker validates catalog structure, references, and bounded
run-record metadata. It does not decide that hardware is safe or that a test
passed.
