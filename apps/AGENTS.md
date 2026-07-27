# Firmware Application Instructions

These instructions apply under `apps/` in addition to the repository root
rules.

## Required context

Read the selected app's `README.md`, `Cargo.toml`, and relevant code before
changing it. Read `../project_docs/CODEX_ACTIVE_WORK.md` before altering live
motor, logging, bench, or debug behavior. For test work, start at
`../project_docs/testing/README.md` and load only the catalog-selected target
procedure.

For localized work, route source context from the repository root before
opening an entire flight-app shell:

```text
python tools/app_context.py --board fcu3 --topic control
python tools/app_context.py --board foxeer-f405-v2 --topic arming
```

Open the returned source anchors and companion files, then broaden inspection
only when the affected ownership or behavior crosses the route. The reported
test chain is routing information, not authorization or a pass result.

`stm32f405-flight` on the FerroWasp FCU3 BSP is the golden flight app for
established runtime behavior, safety policy, task sequencing, and supported
feature integration. `stm32f401-bringup` is a minimal non-actuator validation
app, not a flight-behavior reference.

## RTIC ownership

- App shells own `#[rtic::app]`, task declarations, priorities, local/shared
  resources, initialization, and scheduling.
- Keep reusable task implementation outside the shell where practical.
- Prefer static ownership and bounded queues. Add shared resources and
  priority changes only with an explicit timing and contention rationale.
- A new task must update the applicable task/resource/policy manifest or
  equivalent documentation.
- Safety-related tasks must state their priority and timing assumptions.

## Golden-app drift prevention

Before implementing a feature in a secondary app, inspect the current FCU3
implementation and identify the invariants to preserve. Compare:

- arming preparation and final guard ordering;
- disarm, RC-loss, failsafe, watchdog, and fault paths;
- actuator permits, output leases, and stale-command containment;
- task priorities and completion ordering;
- logging identities and exact feature gates.

Implement shared protocol, driver, safety, and task logic in reusable crates
when practical. App shells should contain only RTIC wiring and genuine
hardware adaptation.

Do not copy an old FCU3 branch as current behavior. Before a secondary-board
bench test, compare its exact enabled feature set with the current FCU3 app and
reject stale fallbacks or obsolete safety sequences.

Board differences in pins, timers, DMA, sensors, orientation, electrical
behavior, or unavailable hardware require an explicit deviation and
board-specific verification. Golden-app status never authorizes copying FCU3
hardware facts to another board.

If a feature does not yet exist in FCU3, integrate it there first or in the
same change when practical. Otherwise document why the secondary app leads and
record the reconciliation work. Any intentional behavioral departure requires
a rationale, review, and verification note; silent drift is a defect.

## Firmware verification

- Run host tests for reusable logic before relying on a target build.
- Use the exact app directory, target, feature set, profile, and locked
  dependencies required by its README or catalog entry.
- Compile evidence does not establish hardware behavior.
- The user performs all target-hardware, powered, props-off, preflight, and
  flight procedures.
- For motor-capable images, record the commit/working tree, exact features,
  artifact hash, propeller state, actuator-power state, and applicable target
  evidence.
- Never claim a bench or flight gate passed from historical evidence or a
  successful build alone.
