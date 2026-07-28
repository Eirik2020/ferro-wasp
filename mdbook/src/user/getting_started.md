# User Guide

FerroWasp currently supports a packaged Windows workflow for the Foxeer F405
V2. You can flash firmware, manage configuration, and retrieve blackbox logs
without installing a Rust toolchain or cloning the repository.

Start with the [Foxeer F405 V2 guide](foxeer_f405_v2.md). It covers:

- validating and flashing a release image through USB DFU;
- connecting to the board through USB CDC;
- backing up and changing disarmed configuration;
- downloading FWBB logs and converting them to ULog.

The [FerroConfigurator chapter](ferro_configurator.md) gives a shorter overview
of the companion application and its safety boundaries.

FerroWasp firmware is experimental. Remove propellers and disconnect actuator
power before USB maintenance or firmware changes.
