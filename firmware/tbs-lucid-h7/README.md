# TBS Lucid H7 Flight App

This isolated RTIC 2 application targets the TBS Lucid H7 (STM32H743). It
carries the Foxeer F405 V2 feature set on the Lucid's own peripherals, through
the `ferrowasp-ferrohal` backend, and owns its own Cargo graph, linker
configuration, and binary.

It has passed bench checks on one board — boot, IMU, USB, and microSD logging —
but has never driven motors or flown. Flight arming is disabled in
`board/src/profiles.rs` until the IMU orientation, motor order, and ADC scale
are verified on the board. Pins, differences from the Foxeer, and current
limitations are in
[Current Support](../../mdbook/src/current_support.md#tbs-lucid-h7-bench-only).

The internal barometer's I2C2 bus on PB10/PB11 is wired: ArduPilot's
`TBS_LUCID_H7` hwdef carries the part's footprints there, probing the DPS310
driver at 0x76 for the DPS368 ("DPS368 (DPS310 driver)", one of the two
populated per build) alongside a BMP390 that is not yet driven. The app
probes with the DPS310-family driver - the DPS368 and the DPS310 share its
interface, down to the `0x10` chip ID at `0x0D` - and, when no barometer
answers, keeps probing the part's two strappings while sweeping the bus.
The FWDBG1 line carries `baro` (the chip ID it answered with), `baro_part`
(which driver measured it: 0 none, 1 the SPL06-007's, 2 the DPS310
family's), `baro_addr`, `baro_ready`, `baro_press_pa`, `baro_temp_dc`,
`baro_probe` (how far the probe reached: 0 no bus, 1 measuring, 2 nothing
at the address, 3 the lines misbehaved, 4 a wrong chip ID, 5 coefficients,
6 oversampling configure) and `baro_scan` (how many addresses the sweep saw
answer). This bench unit's bus is silent: `baro_probe=2 baro_scan=0`, a
clean NACK at every address with no line faults, while the IMU keeps its
2 kHz cadence.

Build:

```text
cargo build --release --locked --target thumbv7em-none-eabihf
```
