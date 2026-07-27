# Target Verification

This bounded entry point owns the common unpowered boot and idle gate. Use
`project_docs/testing/README.md` and `project_docs/testing/TEST_CATALOG.json`
to select every other software, target, bench, preflight, or flight test.

An active test definition is not evidence that a candidate passed and does not
clear an aircraft for flight. The user operates target hardware.

## Boot and Idle State

Purpose: establish common boot, idle, and actuator-inhibit behavior without
actuator power.

Prerequisite:

- complete `SW-COMMON-001` from the test catalog;
- identify the exact board, firmware image hash, and enabled feature set;
- remove propellers and disconnect actuator power;
- arrange observation of boot, panic, health, and arming state.

Checks:

- [ ] Confirm the board boots reliably from cold power and reset.
- [ ] Confirm all motor outputs remain at low/stop value during boot.
- [ ] Confirm no motor output pulses occur before the actuator-output task is initialized.
- [ ] Confirm boot RTT logs and the red/green LED heartbeat show the scheduler is alive.
- [ ] Confirm the firmware remains disarmed if the RC receiver is disconnected at boot.
- [ ] Confirm brownout or manual reset returns the system to disarmed low-output state.

Stop immediately on:

- unexpected motor output or actuator activity;
- inability to observe boot, panic, health, or arming state;
- uncertainty about the exact target or feature set.

Retain the board identity, firmware image hash, exact feature set, power and
propeller state, and boot/idle log. A result applies only to the recorded
candidate.

## Target Procedures

- FCU3: `project_docs/testing/targets/fcu3.md`
- Foxeer F405 V2: `project_docs/testing/targets/foxeer-f405-v2.md`

Start with the catalog so prerequisites and validation tiers remain ordered.
Do not substitute historical evidence for rerunning the selected gate on a new
candidate.

## Historical Evidence

The bounded map is `project_docs/testing/EVIDENCE_INDEX.md`.

The byte-preserved checklist and dated results that previously occupied this
path are retained at
`project_docs/archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md`.
Unchecked items in that snapshot are historical checklist state, not
automatically active test definitions or current flight obligations.
