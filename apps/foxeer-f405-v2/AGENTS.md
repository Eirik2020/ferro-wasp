# Foxeer F405 V2 Application Instructions

These instructions apply with `../AGENTS.md` and the repository root rules.
This is a secondary flight app; the current FCU3 app remains the behavioral
reference.

## Context and authority

Before Foxeer runtime, motor, configuration, logging, or test work, read:

- `README.md` and `Cargo.toml`;
- `../../project_docs/CODEX_ACTIVE_WORK.md`;
- the current FCU3 implementation for the affected behavior;
- `../../project_docs/testing/targets/foxeer-f405-v2.md` when selecting target
  testing.

Use current code and the target procedure for live defaults. Treat dated hashes,
gains, and observations in large verification documents as evidence, not
current configuration.

## Board-specific boundaries

- Do not transplant FCU3 pins, timers, DMA streams, IMU orientation, motor
  output mapping, electrical assumptions, or storage behavior.
- Keep Foxeer hardware facts in its BSP and document every intentional
  divergence from FCU3 behavior.
- Preserve the explicit physical-body/controller-frame pitch compatibility
  boundary. Do not hide sign changes in an unrelated sensor or mixer layer.
- The normal digital-motor path uses the reviewed DShot/eRPM-qualified safety
  sequence. Diagnostic and fallback features must not silently replace it.
- SPI-NOR logging and disarmed configuration may change data and allowed
  tuning values only; they never gain arming or actuator authority.
- Keep SWD pins reserved and preserve the documented reset/connection behavior.

Ask before changing any Foxeer pin, DMA, timer, orientation, motor mapping,
safety sequence, storage layout, or configuration schema.

## Verification

Build with an explicit target and feature set. Before powered work, compare the
exact image with FCU3 behavior and the Foxeer test catalog chain. Preserve
image/configuration hashes and log identity.

All Foxeer hardware operation is user-executed. Propellers remain removed
until the reviewed preflight and flight prerequisites are complete. Any
unexpected motor, correction, arming, telemetry, storage, or RC-loss behavior
fails closed.
