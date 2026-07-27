# Testing

FerroWasp testing is staged: static checks, props-off bench checks, then one
bounded flight change at a time. The user operates powered hardware. Stop on
any unexpected motor response, oscillation, heat, smoke, loss of RC/video, or
loss of confidence.

## Operator guides

- [Foxeer F405 V2 USB Quick Start](../../docs/FOXEER_F405_V2_QUICK_START.md) covers ROM-DFU
  release flashing, disarmed parameter changes, selective blackbox download,
  analysis, and ULog conversion.
- Published configurator ZIPs are attached to
  [GitHub Releases](https://github.com/Eirik2020/ferro-wasp/releases). Local
  packaging writes generated ZIPs under
  [`tools/ferro-configurator/dist`](../../tools/ferro-configurator/dist/);
  that Git-ignored folder exists only after the package script runs.
- The repository test catalog is in `project_docs/testing/README.md`.
- The current board-specific procedure is in
  `project_docs/testing/targets/foxeer-f405-v2.md`.

The Quick Start is an operating guide, not flight authorization. A newly built
image still requires checks proportional to its changes, and hardware,
configuration, wiring, or airframe changes can invalidate prior evidence.

## Current Foxeer flight status

The current experimental P-only baseline is roll/pitch/yaw
`2.5 / 2.5 / 2.0`, with all I and D gains zero. The operator has classified
this setup as flyable in a confined area, but not well tuned. Verify the
persisted configuration before every later session and change only one tuning
variable at a time.

Before flight, power the FCU and ESCs together from the flight battery, keep
the aircraft still through gyro calibration, and complete the active preflight
procedure. After landing, disarm and wait at least two seconds for the final
blackbox page before removing power.
