# Foxeer F405 V2 Flight App

This isolated RTIC 2 application targets the Foxeer F405 V2. It is FerroWasp's
golden flight app and the behavioral reference for other flight targets. The
package owns its board contract, Cargo graph, linker configuration, and
`FerroWaspFoxeerF405V2` binary.

Users should start with the
[Foxeer USB Quick Start](../../mdbook/src/user/foxeer_f405_v2.md). The
published Windows package contains the checked release image and does not
require a Rust toolchain, Python, STM32CubeProgrammer, or an SWD probe.

## Application Contract

- `src/main.rs` contains RTIC resources, initialization wiring, locking,
  scheduling, and task declarations.
- `src/lib.rs` exposes the app's internal facade.
- `src/board/` owns Foxeer pins, peripherals, DMA routes, timers, electrical
  profiles, orientation, and motor mapping.
- Reusable STM32F4 mechanisms belong in `ferrowasp-stm32f4`; reusable protocol,
  safety, driver, and task behavior belongs in the narrowest shared crate.
- Foxeer USB, DShot600, ESC telemetry, persistent configuration, onboard
  blackbox storage, and DJI O4 MSP DisplayPort are standard parts of the
  flight image.
- Diagnostic and bench features are not flight configurations and may not
  bypass normal arming, actuator, freshness, or failsafe checks.

## Board Contract

- 8 MHz HSE and 168 MHz system clock.
- SPI1 IMU on PA4-PA7 with PC4/EXTI4 data ready; boot selects an MPU6500
  (`WHO_AM_I=0x70`) or ICM42688-P (`WHO_AM_I=0x47`).
- SBUS on USART2 PA2/PA3 and DJI O4 MSP DisplayPort on UART4 PA0/PA1.
- Battery-voltage and current observation on ADC1 PC0/PC1.
- Four-lane DShot600 on PA8, PC9, PC8, and PB15.
- BLHeli legacy telemetry RX on PA10/USART1.
- USB FS on PA11/PA12.
- 16 MiB-class SPI2 NOR storage on PB12/PB13/PC2/PC3.
- SWD/RTT reserves PA13/PA14.

M4 uses the complementary `TIM1_CH3N` output. Its functional polarity has
powered props-off evidence, but exact electrical waveform measurement remains
open. Preserve the explicit boundary between the physical Foxeer IMU mapping
and the controller-frame pitch compatibility transform.

The board manifest and files under `src/board/` are authoritative when this
summary and code disagree. Do not transplant FCU3 hardware facts into this app.

## Build

From this app directory:

```powershell
cargo build --release --locked
```

From the repository root, the explicit Foxeer SWD/RTT workflow is:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
```

Do not use the FCU3 binary on this board. Select diagnostic features only
through a reviewed test procedure, and keep propellers removed for every
bench-actuator image.

## Documentation and Testing

- [Current support and limitations](../../mdbook/src/current_support.md)
- [Foxeer USB and FerroConfigurator guide](../../mdbook/src/user/foxeer_f405_v2.md)
- [Arming and safety](../../mdbook/src/arming.md)
- [DShot and ESC telemetry](../../mdbook/src/dshot.md)
- [IMU behavior](../../mdbook/src/imu.md)
- [Foxeer target procedure](../../project_meta/testing/targets/foxeer-f405-v2.md)
- [Retained evidence index](../../project_meta/testing/EVIDENCE_INDEX.md)

The target procedure defines current bench, preflight, and flight gates.
Historical hashes and prior passes are evidence, not authorization to operate
hardware or a substitute for testing the proposed image.
