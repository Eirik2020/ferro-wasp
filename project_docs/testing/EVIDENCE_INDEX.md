# Historical Test Evidence Index

This bounded index locates retained bench and target evidence without making
the complete historical records part of normal context. It is provenance, not
a current procedure, pass status, or flight-clearance document.

## Current Authority

Use these sources for current work:

- `../CODEX_ACTIVE_WORK.md` for the live bench, logging, and flight handoff;
- `README.md` and `TEST_CATALOG.json` for test selection and prerequisites;
- `targets/fcu3.md` for the current FCU3 operator procedure;
- `targets/foxeer-f405-v2.md` for the current Foxeer F405 V2 procedure;
- `../../docs/FOXEER_F405_V2_QUICK_START.md` for the current USB field
  cheatsheet.

Current code, manifests, features, and tests outrank historical prose. An
active catalog entry means selectable after prerequisites, not passed.

## Target Verification Baseline

Snapshot:
[TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md)

- Bytes: `64,546`
- SHA-256:
  `86E29BB3FBE491D99C2AFBD61F360403552F68714C405933BBB9F5300E6F3134`
- Snapshot captured: 2026-07-27
- Dated evidence present: 2026-07-18 through 2026-07-22

Section map:

- [Build and Flash](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#build-and-flash)
- [Boot and Idle State](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#boot-and-idle-state)
- [Safety and Arming](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#safety-and-arming)
- [RC Input](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#rc-input)
- [IMU and Estimator Path](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#imu-and-estimator-path)
- [Control Loop and Mixer](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#control-loop-and-mixer)
- [Actuator Output - PWM](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#actuator-output---pwm)
- [Actuator Output - FCU3 DShot](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#actuator-output---fcu3-dshot)
- [ADC, Power, and Battery Data](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#adc-power-and-battery-data)
- [OSD, MSP, USB, and Telemetry](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#osd-msp-usb-and-telemetry)
- [Fault Injection](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#fault-injection)
- [Timing and Evidence](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#timing-and-evidence)
- [Foxeer F405 V2 Initial Gate](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#foxeer-f405-v2-initial-gate)
- [Foxeer first-hop corrective gate](../archive/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#foxeer-first-hop-corrective-gate-2026-07-22)

The snapshot contains 99 checked and 113 unchecked checklist items. A checked
item is evidence only for its recorded candidate and conditions. An unchecked
item records the checklist state at capture time; consult current work and the
catalog before treating it as an outstanding obligation.

## Bench Plan Baseline

Snapshot:
[BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md)

- Bytes: `61,010`
- SHA-256:
  `ADB7FAF0ABB66907D9316B8CB845EFB4F93CABAAC663861315BD78101EA424C9`
- Snapshot captured: 2026-07-27

Section map:

- [Safety Setup](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#safety-setup)
- [Tools And Data](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#tools-and-data)
- [Completed Tests](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#completed-tests)
  contains Tests 1-11 and their recorded outcomes.
- [Open Risks](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#open-risks)
  and [Tomorrow Start Here](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#tomorrow-start-here)
  are dated state snapshots, not current instructions.
- [Flight-Trial Preflight Gate](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#flight-trial-preflight-gate)
- [Next Tests](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#next-tests)
  contains the earlier Tests 0 and A-G.
- [Current Flight Gate](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#current-flight-gate)
  is historical despite its original heading.
- [SPSC Motor Command Target Checkpoint](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#spsc-motor-command-target-checkpoint)
- [Foxeer USB RC-Tuning Checkpoint](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#foxeer-usb-rc-tuning-checkpoint)
- [Foxeer Selective Blackbox Download and Boot Grouping](../archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#foxeer-selective-blackbox-download-and-boot-grouping)

Historical tuning values, statuses, recommended next sessions, and proposed
tests apply only to their original development snapshot. Never use them over a
newer reviewed configuration or current target procedure.

## Evidence Handling

- Open only the archive section needed for the specific provenance question.
- Preserve dates, image and log hashes, feature sets, equipment, results, and
  stated limitations when citing evidence.
- Do not edit or append to the immutable baselines.
- Do not point an active catalog procedure into an archive.
- Store new results separately from test definitions using
  `evidence/README.md`. This index remains a map of pre-rollout history rather
  than a cumulative results file.
