# FerroWasp Test System

This directory is the entry point for verifying FerroWasp: choosing what to
test, running it, and recording the evidence. Start here when the user asks to
test firmware, prepare a target, run a bench procedure, or perform a flight.

The design is [the requirements design](../requirements/design.md) and
ADR-0050 to ADR-0053; the tool is `tools/reqtool`, described in its README.

## Requirements, Procedures and Evidence

Keep these separate:

```text
Requirement = what shall be true, declared by the code it governs  project_meta/requirements/*.yaml
Procedure   = how a person verifies it on hardware                 project_meta/testing/targets/*.md
Evidence    = what a run showed, on which code, with what result   project_meta/testing/evidence/runs/
```

Each requirement lists its verification entries: a level, a target, a setup
and a method, which is a host test or a procedure (`path#heading`). Status is
computed by `reqtool` from the evidence records and identities derived from
the code; nobody writes it. A host pass stands while the files it exercised
and their direct imports are unchanged, a board pass while its
configuration is, and either goes `stale` when what it covered changes.

A procedure owns its own steps, stop conditions and required evidence.
Dated results, image hashes, logs, measurements and operator observations
belong in run records, never in a procedure or a requirement.

## Levels

Verification moves from lower-risk evidence toward flight:

1. `analysis` — tool checks on source and images: the image builds, and
   repository rules hold;
2. `unit` — host tests, including conformance suites;
3. `sitl` — software in the loop, once a harness exists;
4. `mcu` — on the flight controller, nothing actuating;
5. `bench` — the full drone, ESCs powered, propellers removed;
6. `flight` — in flight.

Host evidence does not establish target behaviour; props-off evidence does not
establish flight readiness (W7).

## Gates

Gates in `../requirements/gates.yaml` say what must be verified before the
next step, and only gates block:

- `mcu-session` — before running mcu procedures on a board;
- `bench-session` — before connecting ESC power with propellers removed;
- `preflight` — before the preflight checklist for a flight.

```text
reqtool gate bench-session --board foxeer-f405-v2
```

lists every entry still in the way. The flight itself follows the preflight
procedure's checklist, which is a per-flight check, not evidence.

## Invocation Workflow

When software changes:

1. `reqtool check <file>` on what you are changing shows the requirements
   it declares and their status;
2. run the software checks (`tools/run_tests.py`), then `reqtool test` to
   record the host evidence the requirements name;
3. add or update a requirement when behaviour, risk, or the required
   evidence changed;
4. report what is now stale or unverified at board level without claiming it
   passed.

When the user requests bench or flight testing:

1. establish the exact board, firmware revision and image hash, features,
   propeller state, actuator-power state, and available equipment;
2. check the gate for the step: `reqtool gate <gate> --board <board>`;
3. run the procedure under `targets/`, in order, and stop on any stop
   condition;
4. obtain explicit user confirmation before powered or flight activity;
5. record the run (below).

Who may run what on hardware is set by the Operating Rules in the root
`AGENTS.md`: an agent may run `mcu` procedures while disarmed with nothing
actuating; `bench` and `flight` procedures, and every physical step such as a
power cycle, are the user's.

## Recording Results

- Host tests: `reqtool test` runs the tests requirements name and records
  the session; a test that fails or does not run is never recorded as a pass.
- The boot and idle gate and the Foxeer USB configuration gate record
  themselves: `tools/bench_rig.py boot-idle` and
  `tools/foxeer_bench_gates.py usb-config` hand their run to `reqtool record`.
- Any other procedure: `reqtool record --procedure <path#heading> --board
  <board> --result pass|fail --details <file>`, where the details file says
  who ran it, on which image, in what conditions, with which artifacts and
  observations. `evidence/README.md` owns the record format.

Records are left uncommitted for review. Never change a result to make a gate
appear complete; a recorded fail stands until the code changes and a run
passes, or a record supersedes it on the user's explicit ruling.

## Safety Rules

W1 and W2 in [the governing requirements](../governing-requirements.md) apply
to every test. In addition:

- powered and flight tests fail closed when their target, image, feature set,
  prerequisites, or evidence capture cannot be established;
- stop conditions take priority over test completion or evidence collection;
- host, mcu, bench and flight evidence are reported as distinct levels.

## Adding a Test

A new host test proves a requirement once a requirement entry names it:
add the entry (`level: unit`, `method: test: <path>`) and run `reqtool
validate`. A new hardware procedure is a section under `targets/` with its
steps, stop conditions and required evidence; requirement entries name it by
`path#heading`, and a gate may require it. Then run:

```text
reqtool validate
python tools/check_repository_context.py
```
