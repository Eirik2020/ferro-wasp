# Current Support

Foxeer F405 V2 is the golden flight target and the behavioral reference for
new flight-board work. FerroWasp FCU3 is a supported secondary flight target.
NUCLEO-F401RE is a non-actuating development target.

The matrix records the important supported capabilities without attempting to
list every peripheral or diagnostic feature.

| Capability | Foxeer F405 V2 | FerroWasp FCU3 | NUCLEO-F401RE |
|---|---|---|---|
| Role | Golden flight target | Secondary flight target | Non-actuating bring-up target |
| MCU / runtime | STM32F405, RTIC 2 | STM32F405, RTIC 2 | STM32F401, RTIC 2 |
| RC input | SBUS over USART2 DMA | SBUS over USART2 DMA | None |
| IMU | Runtime-selected MPU6500 or ICM42688-P; EXTI data-ready sampling | MPU6500; timer-driven polling | None |
| Control | 400 Hz rate controller and Quad X mixer | 400 Hz rate controller and Quad X mixer | None |
| ESC output | Four-lane DShot600 | Four-lane DShot600 | None |
| ESC telemetry | Standard BLHeli legacy UART eRPM path | Standard BLHeli legacy UART eRPM path | None |
| Arming qualification | Fresh idle eRPM from all four motors | Fresh idle eRPM from all four motors | Not applicable |
| USB | Standard USB CDC | Optional USB CDC | None |
| Configuration | Persistent onboard configuration through FerroConfigurator | Compile-time and app-local configuration | None |
| Blackbox | Standard onboard SPI-NOR FWBB logging and download | RTT / BB2 development logging | None |
| Pilot display | DJI O4 MSP DisplayPort OSD | DJI O4 MSP DisplayPort OSD | None |
| ADC | Battery voltage; current input is present but not yet reliable | Battery voltage and current inputs | None |
| Current evidence | Boot, USB, IMU, RC, DShot, eRPM-qualified arming, blackbox, controlled hops, and confined-area flight | Boot, RC, IMU, DShot, eRPM-qualified arming, props-off checks, and controlled flight | Build and target smoke checks |

## Foxeer F405 V2

The Foxeer app lives in `apps/foxeer-f405-v2`. USB, DShot, ESC telemetry,
persistent configuration, onboard blackbox storage, and MSP OSD are standard
parts of its flight image rather than optional board capabilities.

The fitted SPI1 IMU is detected at boot. MPU6500 and ICM42688-P share the
bounded DMA transport and board orientation contract; PC4/EXTI4 supplies the
normal data-ready event. The controller consumes fresh samples at 400 Hz.

Four DShot600 lanes use TIM1 and TIM8 with board-local pin and DMA routes. The
actuator path continuously selects stop while disarmed, rejects stale motor
commands, and contains a lane or DMA failure across the whole four-output
bank. The bounded ESC manager associates PA10 / USART1 legacy telemetry with
requested physical outputs and supplies eRPM evidence for guarded arming.

Onboard SPI-NOR stores CRC-protected configuration and FWBB flight records.
The [FerroConfigurator](user/ferro_configurator.md) uses USB CDC to manage that
storage while the aircraft is disarmed.

## FerroWasp FCU3

The FCU3 app lives in `apps/stm32f405-flight`. It shares the reusable STM32F4,
driver, task, safety, DShot, and telemetry implementations with Foxeer while
retaining its own pins, DMA routes, timer assignments, IMU orientation, and
motor map.

FCU3 uses an MPU6500 and timer-driven IMU polling. Four-lane DShot600 and
legacy UART ESC telemetry are standard for ESC control and arming
qualification. USB remains optional, and FCU3 does not yet use the standard
Foxeer persistent configuration and onboard blackbox workflow.

## NUCLEO-F401RE

The bring-up app lives in `apps/stm32f401-bringup`. It owns a status LED,
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
- diagnose the operator-reported Foxeer current reading remaining near `0.8 A`
  throughout flight; treat it as unavailable for load or safety decisions,
  correct ADC1 clock initialization, and then validate PC1 response, offset,
  and scale against external measurements;
- add CRSF/ELRS while retaining SBUS;
- validate any experimental MSPv2 configurator endpoint before making it part
  of the standard image;
- extend self-describing blackbox data with the remaining configuration,
  per-motor telemetry, and crash-analysis fields.

See the [Roadmap](roadmap.md) for planned work rather than treating this page
as a backlog or test log.
