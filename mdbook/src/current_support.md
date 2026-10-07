# Current Support

Foxeer F405 V2 is the golden flight target and the behavioral reference for
new flight-board work. NUCLEO-F401RE is a non-actuating development target.
FerroWasp FCU3, obsolete since 2026-09-22, was removed on 2026-10-07. TBS
Lucid H7 is an unverified port of the Foxeer feature set to the STM32H743; it
builds, but has not run on hardware.

The matrix records the important supported capabilities without attempting to
list every peripheral or diagnostic feature.

| Capability | Foxeer F405 V2 | NUCLEO-F401RE |
|---|---|---|
| Role | Golden flight target | Non-actuating bring-up target |
| MCU / runtime | STM32F405, RTIC 2 | STM32F401, RTIC 2 |
| RC input | SBUS over USART2 DMA by default; ports bound at boot from saved config | None |
| IMU | Runtime-selected MPU6500, ICM42688-P or MPU-6000; EXTI data-ready sampling | None |
| Control | 400 Hz rate controller and Quad X mixer | None |
| ESC output | Four-lane DShot600 | None |
| ESC telemetry | Standard BLHeli legacy UART eRPM path | None |
| Arming qualification | Fresh idle eRPM from all four motors | Not applicable |
| USB | Standard USB CDC | None |
| Configuration | Persistent onboard configuration through FerroConfigurator | None |
| Blackbox | Standard onboard SPI-NOR FWBB logging and download | None |
| Pilot display | DJI O4 MSP DisplayPort OSD | None |
| ADC | Battery voltage and current inputs | None |
| Current evidence | Boot, USB, IMU, RC, DShot, eRPM-qualified arming, blackbox, controlled hops, and confined-area flight | Build and target smoke checks |

## Foxeer F405 V2

The Foxeer app lives in `firmware/foxeer-f405-v2`. USB, DShot, ESC telemetry,
persistent configuration, onboard blackbox storage, and MSP OSD are standard
parts of its flight image rather than optional board capabilities.

The fitted SPI1 IMU is detected at boot. MPU6500, ICM42688-P and MPU-6000
share the bounded DMA transport and board orientation contract; PC4/EXTI4
supplies the normal data-ready event. The controller consumes fresh samples at 400 Hz.

Four DShot600 lanes use TIM1 and TIM8 with board-local pin and DMA routes. The
actuator path continuously selects stop while disarmed, rejects stale motor
commands, and contains a lane or DMA failure across the whole four-output
bank. The bounded ESC manager associates PA10 / USART1 legacy telemetry with
requested physical outputs and supplies eRPM evidence for guarded arming.

Onboard SPI-NOR stores CRC-protected configuration and FWBB flight records.
The [FerroConfigurator](user/ferro_configurator.md) uses USB CDC to manage that
storage while the aircraft is disarmed.

## TBS Lucid H7 (unverified)

The app lives in `firmware/tbs-lucid-h7` and carries the Foxeer feature set on
the Lucid's own peripherals, through the `ferrowasp-stm32h7` backend. Pins
follow the upstream Betaflight `TBS_LUCID_H7` target. Nothing here has run on
the board, so flight arming is disabled at compile time until the IMU
orientation, motor order, and ADC scale are verified on it.

| Function | Lucid H7 resource |
|---|---|
| IMU | SPI1 (PA5, PA6, PD7), CS PC15, data-ready PB2 / EXTI2; MPU6500, ICM42688-P or MPU-6000 |
| SBUS (default `uart6`) | USART6 RX PC7, inverted in the UART |
| MSP DisplayPort (default `uart3`) | USART3 (PD8 TX, PD9 RX) |
| ESC telemetry (default `uart8`) | UART8 RX PE0 |
| DShot600 | PB0, PB1 (TIM3), PA0, PA1 (TIM5), DMA2 streams 0-3 |
| ADC | Voltage PC0, current PC1 |
| Storage | microSD on SDMMC1 |
| USB | USB CDC on PA11/PA12 |

Differences from the Foxeer that matter on the bench:

- The microSD card stands in for SPI NOR. FerroWasp claims only a card whose
  first block is blank or already its own, so a card with a partition table
  is refused and left untouched; zero its first block to give it to
  FerroWasp. The card is then no longer readable by a PC.
- TIM3 and TIM5 are started back to back in software rather than
  hardware-synchronized, so the motor 1-2 and 3-4 frames may be offset by a
  few cycles.
- The ADC is read with blocking conversions against a nominal 3.3 V
  reference.
- The core runs at 400 MHz. The image links at `0x08000000`, replacing any
  bootloader.
- The Lucid's second IMU, barometer, and extra UARTs and motor outputs are
  not used.

## NUCLEO-F401RE

The bring-up app lives in `firmware/stm32f401-bringup`. It owns a status LED,
USART heartbeat, and RTIC scheduling resources. Its board contract declares no
IMU or actuator outputs, so it is useful for non-actuating STM32F4 development
without pretending to be a flight target.

## Shared Flight Behavior

The two flight apps use the same bounded behavior for the main control chain:

```text
SBUS input
  -> safety-qualified arm request
  -> fresh IMU sample
  -> 400 Hz rate controller and Quad X mixer
  -> bounded motor-command queue
  -> safety-owned 500 Hz DShot600 service
  -> legacy ESC telemetry and idle-eRPM qualification
```

The controller includes gyro filtering, an accelerometer-assisted attitude
estimate, PID and feedforward primitives, and a Quad X mixer. Foxeer fresh
storage currently defaults to a conservative P-only profile; persistent
configuration is authoritative after it has been written and validated.

RC-PWM remains available in shared libraries for servos and auxiliary outputs.
It is not an alternative ESC protocol in the flight apps.

For detailed behavior, see [IMU](imu.md), [Arming Sequence](arming.md),
[DShot](dshot.md), and [Communication Protocols](communication_protocols.md).

## Known Limitations

Important open work includes:

- measure DShot pulse timing, cross-timer phase, jitter, and physical stop
  latency on target instrumentation;
- add an independent actuator deadline for complete loss of future control-loop
  wake-ups;
- expand target fault injection for IMU freshness and wider system-health
  escalation;
- finish estimator and controller validation, bounded I-term repair, and
  airframe-specific tuning;
- fine-calibrate Foxeer voltage and current scaling;
- add CRSF/ELRS while retaining SBUS;
- validate any experimental MSPv2 configurator endpoint before making it part
  of the standard image;
- extend self-describing blackbox data with the remaining configuration,
  per-motor telemetry, and crash-analysis fields.

See the [Roadmap](roadmap.md) for planned work rather than treating this page
as a backlog or test log.
