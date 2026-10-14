# Historical Test Evidence Index

This bounded index locates retained bench and target evidence without making
the complete historical records part of normal context. It is provenance, not
a current procedure, pass status, or flight-clearance document.

## Current Authority

Use these sources for current work:

- `python tools/components.py due` for evidence owed by changed components;
- `README.md` and `TEST_CATALOG.json` for test selection and prerequisites;
- `targets/foxeer-f405-v2.md` for the current Foxeer F405 V2 procedure;
- `../../mdbook/src/user/foxeer_f405_v2.md` for the Foxeer user guide.

Current code, manifests, features, and tests outrank historical prose. An
active catalog entry means selectable after prerequisites, not passed.

## Target Verification Baseline

Snapshot:
[TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md)

- Bytes: `64,435`
- SHA-256:
  `E88DC3F56C7020A3D559A576DD9BB6DC201FCB89D1A916E0E71387E850394B10`
- Snapshot captured: 2026-07-27
- Dated evidence present: 2026-07-18 through 2026-07-22

Section map:

- [Build and Flash](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#build-and-flash)
- [Boot and Idle State](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#boot-and-idle-state)
- [Safety and Arming](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#safety-and-arming)
- [RC Input](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#rc-input)
- [IMU and Estimator Path](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#imu-and-estimator-path)
- [Control Loop and Mixer](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#control-loop-and-mixer)
- [Actuator Output - PWM](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#actuator-output---pwm)
- [Actuator Output - FCU3 DShot](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#actuator-output---fcu3-dshot)
- [ADC, Power, and Battery Data](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#adc-power-and-battery-data)
- [OSD, MSP, USB, and Telemetry](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#osd-msp-usb-and-telemetry)
- [Fault Injection](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#fault-injection)
- [Timing and Evidence](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#timing-and-evidence)
- [Foxeer F405 V2 Initial Gate](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#foxeer-f405-v2-initial-gate)
- [Foxeer first-hop corrective gate](../../archive/project_meta/TARGET_VERIFICATION_FULL_BASELINE_2026-07-27.md#foxeer-first-hop-corrective-gate-2026-07-22)

The snapshot contains 99 checked and 113 unchecked checklist items. A checked
item is evidence only for its recorded candidate and conditions. An unchecked
item records the checklist state at capture time; consult current work and the
catalog before treating it as an outstanding obligation.

## Bench Plan Baseline

Snapshot:
[BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md)

- Bytes: `61,010`
- SHA-256:
  `ADB7FAF0ABB66907D9316B8CB845EFB4F93CABAAC663861315BD78101EA424C9`
- Snapshot captured: 2026-07-27

Section map:

- [Safety Setup](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#safety-setup)
- [Tools And Data](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#tools-and-data)
- [Completed Tests](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#completed-tests)
  contains Tests 1-11 and their recorded outcomes.
- [Open Risks](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#open-risks)
  and [Tomorrow Start Here](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#tomorrow-start-here)
  are dated state snapshots, not current instructions.
- [Flight-Trial Preflight Gate](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#flight-trial-preflight-gate)
- [Next Tests](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#next-tests)
  contains the earlier Tests 0 and A-G.
- [Current Flight Gate](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#current-flight-gate)
  is historical despite its original heading.
- [SPSC Motor Command Target Checkpoint](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#spsc-motor-command-target-checkpoint)
- [Foxeer USB RC-Tuning Checkpoint](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#foxeer-usb-rc-tuning-checkpoint)
- [Foxeer Selective Blackbox Download and Boot Grouping](../../archive/project_meta/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md#foxeer-selective-blackbox-download-and-boot-grouping)

Historical tuning values, statuses, recommended next sessions, and proposed
tests apply only to their original development snapshot. Never use them over a
newer reviewed configuration or current target procedure.

## Evidence Handling

- Everything linked here is in the restricted archive: an agent asks the user
  before opening any of it, names the section and why, and opens only that.
- Preserve dates, image and log hashes, feature sets, equipment, results, and
  stated limitations when citing evidence.
- Do not edit or append to the immutable baselines.
- Do not point an active catalog procedure into an archive.
- Store new results separately from test definitions using
  `evidence/README.md`. This index remains a map of pre-rollout history rather
  than a cumulative results file.
