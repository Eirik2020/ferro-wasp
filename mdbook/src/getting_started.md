# Getting Started

Choose the path that matches what you want to do.

## Use FerroWasp on a Foxeer F405 V2

Follow the
[Foxeer F405 V2 USB Quick Start](../../docs/FOXEER_F405_V2_QUICK_START.md).
It uses a ready Windows package to:

- validate and flash the included firmware over STM32 ROM DFU;
- inspect and change disarmed flight parameters;
- selectively download onboard flight logs;
- convert retained FWBB logs to ULog for PlotJuggler.

This path does not require a source checkout, Rust, Python,
STM32CubeProgrammer, or an SWD debugger.

FerroWasp is experimental flight-control firmware. Read the safety warnings,
remove every propeller before USB or motor bench work, and repeat the specified
props-off checks after firmware, configuration, wiring, or actuator changes.

## Develop or contribute to FerroWasp

Use the separate
[Developer Getting Started](developer_getting_started.md) guide. It covers the
source checkout, pinned Rust environment, repository checks, isolated firmware
apps, documentation build, and local FerroConfigurator release packaging.

Before contributing, also read [CONTRIBUTING.md](../../CONTRIBUTING.md) and the
repository instructions that apply to the files being changed.

