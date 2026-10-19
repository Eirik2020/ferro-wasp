# Target Verification

This bounded entry point owns the common unpowered boot and idle gate. Use
`project_meta/testing/README.md` to select every other software, target,
bench, preflight, or flight test.

A current procedure is not evidence that a candidate passed and does not
clear an aircraft for flight. The user operates target hardware.

## Boot and Idle State

Purpose: establish common boot, idle, and actuator-inhibit behavior without
actuator power.

`python3 tools/bench_rig.py boot-idle --board <target>` runs this gate on any
flight board over SWD: it builds and flashes the image, captures a boot from
SWD reset, checks the LED pins where the board has LEDs, and captures a boot
after the one USB power cycle the operator performs, then records a pass with
reqtool for review. `check-boot-idle` judges existing captures to the same
standard.

Verifies: BOOT-001, BOOT-002. Former catalog ID `BENCH-COMMON-001`, kept by
its earlier run records.

Prerequisite:

- `reqtool gate mcu-session` is open: the host evidence is current and the
  image builds;
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
propeller state, and boot/idle log; the record's fingerprint covers the code
that ran.

## Target Procedures

- Foxeer F405 V2: `project_meta/testing/targets/foxeer-f405-v2.md`
- TBS Lucid H7: this common gate only; the board cannot arm yet.

The gates in `project_meta/requirements/gates.yaml` keep the steps in order.
A pass stands until the code it covered changes; then rerun the gate.

## Historical Evidence

The bounded map is `project_meta/testing/EVIDENCE_INDEX.md`.

The byte-preserved checklist and dated results that previously occupied this
path are retained at
`archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md`.
Unchecked items in that snapshot are historical checklist state, not
automatically active test definitions or current flight obligations.
