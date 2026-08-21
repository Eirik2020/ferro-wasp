# Checkpoint 0 Accepted File-Level Migration Map

Status: accepted implementation input

Date: `2026-08-16`

This map applies the user-approved revision-3 architecture in
[`filesystem_ref.md`](filesystem_ref.md) to every Rust source below
`tools/app-builder-v3/xtask/src`. It records destinations and compilation
boundaries; it does not claim that any source has moved or that generated or
target behavior has changed.

Inventory command:

```text
rg --files tools/app-builder-v3/xtask/src | sort
```

Inventory result: **76 files**, each listed exactly once below.

## Compilation and movement rules

- `AB` means the future isolated `tools/app-builder` host crate.
- `GA` means the generated embedded application. For a task row, `AB + GA`
  means the complete source file is host-compiled by `AB`, while only the
  selected `reusable_task!` function body is copied into and compiled by `GA`.
- `EXT` means a repository-owned Rust input compiled by `AB` through the one
  explicit `src/input_catalog.rs` registry. It is not a separate Cargo crate.
- No current V3 source file moves directly into `ferrowasp-stm32f4`. Existing
  shared STM32F4 runtime mechanisms remain dependencies of generated code.
- App-specific task inputs use `apps/foxeer-f405-v2/tasks/`; reusable
  HAL-independent task inputs use top-level `tasks/`; reusable STM32F4
  authoring wrappers use `tools/app-builder/src/backends/stm32f4/tasks/`.
- `file!()` and source extraction must resolve each relocated task file, not
  `input_catalog.rs`. A move is rejected if the emitted body changes.
- “Split” below means a mechanical module-registry split after fixtures exist,
  not a task-body, API, safety-policy, or runtime redesign.

## Host shell and module roots — 4 files

| V3 source below `xtask/src/` | Physical owner / destination | Compiler | Role and generated-source participation | Direct dependencies and path assumptions |
|---|---|---|---|---|
| `main.rs` | `tools/app-builder/src/main.rs` | AB | Host CLI only; emits nothing directly. | `std`, builder library; replace the prototype parent-directory assumption with the isolated mainline root and preserve exit behavior. |
| `lib.rs` | `tools/app-builder/src/lib.rs` | AB | Host module root; emits nothing. | Rewire `hardware_definitions` to `backends`, replace selected `target`/mixed `tasks` modules with `input_catalog` registrations. |
| `generator.rs` | `tools/app-builder/src/generator.rs` | AB | Orchestrates resolve, reconciliation, rendering, formatting, and writes `main.rs`, `prelude.rs`, platform/live config, and the safety report. | `std`, `anyhow`, RTIC renderer/resolver, app inputs; fixed `generated/` paths must become selected app/output paths only after nested-workspace proof. |
| `hardware_definitions/mod.rs` | `tools/app-builder/src/backends/mod.rs` | AB | Host backend module root; emits nothing. | STM32F4 backend module only. |

## RTIC model, resolution, and rendering — 11 files

| V3 source below `xtask/src/` | Physical owner / destination | Compiler | Role and generated-source participation | Direct dependencies and path assumptions |
|---|---|---|---|---|
| `rtic/component.rs` | `tools/app-builder/src/rtic/component.rs` | AB | Component vocabulary/validation dispatch; no direct source emission. | STM32F4 component declarations. Preserve initially; app-specific component selection enters through `input_catalog`. |
| `rtic/composition.rs` | `tools/app-builder/src/rtic/composition.rs` | AB | App/task/resource declaration model and structural validation; drives all generated topology. | RTIC model modules, STM32F4 board/component types, PAC interrupt enum. Currently STM32F4-coupled by design. |
| `rtic/mod.rs` | `tools/app-builder/src/rtic/mod.rs` | AB | Host module root. | All RTIC submodules. |
| `rtic/platform_config.rs` | `tools/app-builder/src/rtic/platform_config.rs` | AB | Typed boot-frozen connector/service model; renderer emits generated `platform_config.rs`. | `std::collections`, `ferrowasp-io-core::platform_config`. App values come from sibling app input. |
| `rtic/render.rs` | `tools/app-builder/src/rtic/render.rs` | AB | Extracts task bodies and renders/parses generated `main.rs`, `prelude.rs`, and platform configuration. | `std::fs`, `anyhow`, `proc_macro2`, `quote`, `syn`, resolved model, STM32F4 lowering; source paths must survive relocation. Extend for live config only after its behavior map is fixture-pinned. |
| `rtic/report.rs` | `tools/app-builder/src/rtic/report.rs` | AB | Renders the generic deterministic safety/architecture report. | Resolved model and safety classes; Foxeer-specific appendices remain app verification input. |
| `rtic/resolve.rs` | `tools/app-builder/src/rtic/resolve.rs` | AB | Expands components and validates complete resource/task/interrupt/safety topology; emits no text directly. | `std` maps/sets, `anyhow`, RTIC declarations, STM32F4 declarations. |
| `rtic/safety_channel.rs` | `tools/app-builder/src/rtic/safety_channel.rs` | AB | Typed safety-channel declarations; drives generated private channel storage and bindings. | FerroWasp safety message types by rendered spelling; no HAL ownership. |
| `rtic/state.rs` | `tools/app-builder/src/rtic/state.rs` initially | AB | Generic state vocabulary/validation plus current Foxeer recipes; drives generated task-local state. | `std` maps/sets. Keep byte-stable initially; move Foxeer recipe selection into the app input in Checkpoint 3 without changing recipes. |
| `rtic/task.rs` | `tools/app-builder/src/rtic/task.rs` | AB | Task contracts, authoring contexts, `TaskSource`, and `reusable_task!`; macro itself is not emitted. | `ferrowasp-core`, `fugit`; `file!()` is a fingerprinted source input. |
| `rtic/timing.rs` | `tools/app-builder/src/rtic/timing.rs` | AB | Monotonic and init-delay declarations/validation; drives generated timer initialization. | No external runtime mechanism; selected hardware IDs resolve through the backend. |

## STM32F4 declarations, lowering, and authoring support — 21 files

| V3 source below `xtask/src/` | Physical owner / destination | Compiler | Role and generated-source participation | Direct dependencies and path assumptions |
|---|---|---|---|---|
| `hardware_definitions/stm32f4/board_declaration.rs` | `tools/app-builder/src/backends/stm32f4/board_declaration.rs` | AB | Host board schema and validation; emits nothing directly. | `ferrowasp-core` frame rotation, `ferrowasp-io-core` IMU ID, all STM32F4 physical declaration types. |
| `hardware_definitions/stm32f4/dma_route.rs` | `tools/app-builder/src/backends/stm32f4/dma_route.rs` | AB | Typed DMA controller/stream/channel facts. | No external crate dependency. |
| `hardware_definitions/stm32f4/dshot.rs` | `tools/app-builder/src/backends/stm32f4/dshot.rs` | AB | Typed DShot lane/bank declarations. | Backend DMA routes and pins. |
| `hardware_definitions/stm32f4/dshot_actuator.rs` | `tools/app-builder/src/backends/stm32f4/dshot_actuator.rs` | AB | Host DShot/ESC component declaration, validation, and resource-type lookup; drives lowering. | Board, DShot, serial, and RTIC composition declarations. The app continues to select the disabled Foxeer instance. |
| `hardware_definitions/stm32f4/dshot_authoring.rs` | `tools/app-builder/src/backends/stm32f4/task_authoring/dshot.rs` | AB | Host-checkable placeholder API for task bodies; file is not target runtime code and is not emitted. | `ferrowasp-stm32f4::serial` outcome types. Generated prelude supplies actual target mechanisms. |
| `hardware_definitions/stm32f4/golden_service_authoring.rs` | `tools/app-builder/src/backends/stm32f4/task_authoring/golden_services.rs` | AB | Host placeholders for ADC, SPI NOR, USB, and watchdog task authoring; not emitted. | `ferrowasp-drivers::spi_nor`; generated prelude supplies actual target types. |
| `hardware_definitions/stm32f4/golden_services.rs` | `apps/foxeer-f405-v2/components/golden_services.rs` (EXT) | AB | Foxeer-specific service-suite declaration, defaults, exact route validation, and resource-type map; drives app component expansion. | STM32F4 board/backend types. It is supplied app semantics, not generic backend policy. |
| `hardware_definitions/stm32f4/gpio.rs` | `tools/app-builder/src/backends/stm32f4/gpio.rs` | AB | Typed GPIO modes and EXTI declarations. | Backend pin IDs. |
| `hardware_definitions/stm32f4/hw_endpoint/imu_endpoint.rs` | `tools/app-builder/src/backends/stm32f4/endpoints/imu.rs` | AB | Host SPI-IMU endpoint/component contract and validation; drives resource/task expansion. | RTIC task/resource bindings and STM32F4 board declarations. |
| `hardware_definitions/stm32f4/hw_endpoint/mod.rs` | `tools/app-builder/src/backends/stm32f4/endpoints/mod.rs` | AB | Host endpoint module root. | IMU and serial endpoint declarations. |
| `hardware_definitions/stm32f4/hw_endpoint/serial_endpoint.rs` | `tools/app-builder/src/backends/stm32f4/endpoints/serial.rs` | AB | Host serial endpoint/component contract and validation; drives resource/task expansion. | RTIC task/resource bindings and STM32F4 board declarations. |
| `hardware_definitions/stm32f4/lower.rs` | `tools/app-builder/src/backends/stm32f4/lower.rs` | AB | Lowers the resolved graph into generated prelude, resource, and initialization text. | `anyhow`, resolved model, all backend declarations, shared FerroWasp runtime APIs by rendered spelling. No generated path may point back to V3. |
| `hardware_definitions/stm32f4/mcu.rs` | `tools/app-builder/src/backends/stm32f4/mcu.rs` | AB | MCU and clock declaration model. | No external crate dependency. |
| `hardware_definitions/stm32f4/mod.rs` | `tools/app-builder/src/backends/stm32f4/mod.rs` | AB | STM32F4 backend root and board prelude. | All backend modules; app/task inputs are registered explicitly rather than owned here. |
| `hardware_definitions/stm32f4/periodic_control.rs` | `tools/app-builder/src/backends/stm32f4/periodic_control.rs` | AB | Timer-backed component declaration plus host authoring placeholder; drives task/resource expansion. | RTIC bindings/contracts and backend board/timer declarations. Keep mixed host concerns together for the behavior-preserving move. |
| `hardware_definitions/stm32f4/pins.rs` | `tools/app-builder/src/backends/stm32f4/pins.rs` | AB | Typed GPIO port/pin facts. | No external crate dependency. |
| `hardware_definitions/stm32f4/serial.rs` | `tools/app-builder/src/backends/stm32f4/serial.rs` | AB | Typed USART/UART route facts. | Backend DMA routes and pins. |
| `hardware_definitions/stm32f4/service_hardware.rs` | `tools/app-builder/src/backends/stm32f4/service_hardware.rs` | AB | ADC, SPI NOR, and USB physical declaration types. | Backend DMA, pins, and SPI peripheral types. |
| `hardware_definitions/stm32f4/spi.rs` | `tools/app-builder/src/backends/stm32f4/spi.rs` | AB | Typed SPI peripheral/pin/DMA facts. | Backend DMA routes and pins. |
| `hardware_definitions/stm32f4/spi_imu.rs` | `tools/app-builder/src/backends/stm32f4/task_authoring/spi_imu.rs` | AB | Host aliases/placeholders for SPI IMU task checking; not emitted as runtime implementation. | `ferrowasp-stm32f4::spi_imu_endpoint` runtime interfaces plus driver frame types. |
| `hardware_definitions/stm32f4/timer.rs` | `tools/app-builder/src/backends/stm32f4/timer.rs` | AB | Typed STM32F4 timer declarations. | No external crate dependency. |

## Current STM32F4 hardware-task authoring — 15 files

Every body in this table is host-authored with `reusable_task!`. None is moved
unchanged into the runtime HAL crate.

| V3 source below `xtask/src/` | Physical owner / destination | Compiler | Role and generated-source participation | Direct dependencies and path assumptions |
|---|---|---|---|---|
| `hardware_definitions/stm32f4/tasks/button_exti.rs` | `tools/app-builder/src/backends/stm32f4/tasks/button_exti.rs` | AB + GA | Reusable STM32F4 EXTI task; function body emitted. | `stm32f4xx-hal::gpio::ExtiPin`, task macro/context. |
| `hardware_definitions/stm32f4/tasks/foxeer_control.rs` | `apps/foxeer-f405-v2/tasks/foxeer_control.rs` (EXT) | AB + GA | Foxeer-specific periodic control task; function body emitted. | Periodic-control authoring API, `ferrowasp-core`, drivers, IO core, `ferrowasp-tasks`, monotonic. App semantics prohibit backend ownership. |
| `hardware_definitions/stm32f4/tasks/imu_data_ready.rs` | `tools/app-builder/src/backends/stm32f4/tasks/imu_data_ready.rs` | AB + GA | Reusable STM32F4 EXTI/IMU wake task; body emitted. | `stm32f4xx-hal::gpio::ExtiPin`, monotonic, task macro/context. |
| `hardware_definitions/stm32f4/tasks/mod.rs` | Split between `tools/app-builder/src/backends/stm32f4/tasks/mod.rs` and explicit app-task registration in `input_catalog.rs` | AB | Current task registry; emits nothing. | All hardware-task modules. Do not retain a backend re-export that makes `foxeer_control` backend-owned. |
| `hardware_definitions/stm32f4/tasks/periodic_control_tick.rs` | `tools/app-builder/src/backends/stm32f4/tasks/periodic_control_tick.rs` | AB + GA | Reusable timer acknowledge/cadence wrapper; body emitted. | Periodic-control authoring API and task macro/context. |
| `hardware_definitions/stm32f4/tasks/serial_rx_bridge.rs` | `tools/app-builder/src/backends/stm32f4/tasks/serial_rx_bridge.rs` | AB + GA | Reusable STM32F4 owned-RX bridge; body emitted. | `ferrowasp-stm32f4::serial` bridge service/outcomes. |
| `hardware_definitions/stm32f4/tasks/serial_rx_dma_irq.rs` | `tools/app-builder/src/backends/stm32f4/tasks/serial_rx_dma_irq.rs` | AB + GA | Reusable STM32F4 RX-DMA IRQ wrapper; body emitted. | `ferrowasp-stm32f4::serial` IRQ service/outcomes. |
| `hardware_definitions/stm32f4/tasks/serial_rx_idle_irq.rs` | `tools/app-builder/src/backends/stm32f4/tasks/serial_rx_idle_irq.rs` | AB + GA | Reusable STM32F4 UART IDLE IRQ wrapper; body emitted. | `ferrowasp-stm32f4::serial` IRQ service/outcomes. |
| `hardware_definitions/stm32f4/tasks/serial_tx_dma_irq.rs` | `tools/app-builder/src/backends/stm32f4/tasks/serial_tx_dma_irq.rs` | AB + GA | Reusable STM32F4 TX-DMA completion wrapper; body emitted. | `ferrowasp-io-core::serial`, `ferrowasp-stm32f4::{memory,serial}`. |
| `hardware_definitions/stm32f4/tasks/serial_tx_worker.rs` | `tools/app-builder/src/backends/stm32f4/tasks/serial_tx_worker.rs` | AB + GA | Reusable STM32F4 DMA TX worker; body emitted. | `ferrowasp-io-core::serial`, `ferrowasp-stm32f4::{memory,serial}`. |
| `hardware_definitions/stm32f4/tasks/spi_imu_owner_service.rs` | `tools/app-builder/src/backends/stm32f4/tasks/spi_imu_owner_service.rs` | AB + GA | Reusable STM32F4 SPI owner wrapper; body emitted. | Backend SPI-IMU authoring aliases and monotonic. |
| `hardware_definitions/stm32f4/tasks/spi_imu_parser.rs` | `tools/app-builder/src/backends/stm32f4/tasks/spi_imu_parser.rs` | AB + GA | Reusable STM32F4 DMA-frame parser/orientation wrapper; body emitted. | SPI-IMU authoring aliases, `ferrowasp-core` frames, IMU drivers. |
| `hardware_definitions/stm32f4/tasks/spi_imu_poll.rs` | `tools/app-builder/src/backends/stm32f4/tasks/spi_imu_poll.rs` | AB + GA | Reusable STM32F4 async SPI transaction wrapper; body emitted. | SPI-IMU authoring aliases, `ferrowasp-io-core::spi`, IMU drivers. |
| `hardware_definitions/stm32f4/tasks/spi_imu_rx_dma_irq.rs` | `tools/app-builder/src/backends/stm32f4/tasks/spi_imu_rx_dma_irq.rs` | AB + GA | Reusable STM32F4 SPI RX-DMA IRQ wrapper; body emitted. | Backend SPI-IMU authoring aliases. |
| `hardware_definitions/stm32f4/tasks/spi_imu_timeout.rs` | `tools/app-builder/src/backends/stm32f4/tasks/spi_imu_timeout.rs` | AB + GA | Reusable STM32F4 SPI deadline recovery wrapper; body emitted. | Backend SPI-IMU authoring aliases, `fugit`, monotonic. |

## Selected Foxeer inputs — 5 files

| V3 source below `xtask/src/` | Physical owner / destination | Compiler | Role and generated-source participation | Direct dependencies and path assumptions |
|---|---|---|---|---|
| `target/app_composition.rs` | `apps/foxeer-f405-v2/app_composition.rs` (EXT) | AB | Complete output-inhibited Foxeer graph; drives all generated topology but is not emitted verbatim. | Sibling board/platform/live configuration inputs, RTIC model, backend components/tasks, app tasks, PAC interrupts. Existing handwritten firmware is not overwritten. |
| `target/board.rs` | `boards/foxeer-f405-v2/board.rs` (EXT) | AB | Physical Foxeer facts and validation tests; drives lowering. | STM32F4 board prelude only; registered explicitly and never compiled into the handwritten embedded app module tree. |
| `target/golden_reconciliation.rs` | `apps/foxeer-f405-v2/verification/golden_reconciliation.rs` (EXT) | AB | App-specific pinned semantic validation and report appendix. | Resolved model plus sibling app/board/config inputs; pinned handwritten source identities remain evidence, not runtime inputs. |
| `target/mod.rs` | Retire after equivalent entries exist in `tools/app-builder/src/input_catalog.rs` | AB | Selected-target module registry; emits nothing. | Board, platform config, composition, reconciliation. No global selected-target module remains. |
| `target/platform_config.rs` | `apps/foxeer-f405-v2/platform_config.rs` (EXT) | AB | Authored boot-frozen service routing; renderer emits generated `platform_config.rs`. | Builder platform-config model and IO-core service IDs. It remains beside app-owned `live_config.rs`. |

## Current general task directory — 20 files

The current folder name is not evidence of portability. Direct imports and
app-specific strings/defaults determine ownership.

| V3 source below `xtask/src/` | Physical owner / destination | Compiler | Role and generated-source participation | Direct dependencies and path assumptions |
|---|---|---|---|---|
| `tasks/actuator_fault_reporter.rs` | `tasks/actuator_fault_reporter.rs` (EXT) | AB + GA | HAL-independent bounded safety fault reporter; body emitted. | `ferrowasp-core` safety/channel APIs. |
| `tasks/adc_observation.rs` | `tools/app-builder/src/backends/stm32f4/tasks/adc_observation.rs` | AB + GA | STM32F4 ADC poll and DMA completion wrappers; two selected bodies emitted. | `ferrowasp-stm32f4::adc`, golden-service authoring types, `ferrowasp-tasks`, `fugit`. |
| `tasks/blink_led.rs` | `tools/app-builder/src/backends/stm32f4/tasks/blink_led.rs` | AB + GA | STM32F4 HAL digital-output task used by validation apps; body emitted. | `stm32f4xx-hal` digital traits, `fugit`, monotonic. A later embedded-hal abstraction is outside this move. |
| `tasks/dshot_dma.rs` | `tools/app-builder/src/backends/stm32f4/tasks/dshot_dma.rs` | AB + GA | STM32F4 DShot lane-completion wrapper; body emitted. | DShot authoring types and core preparation report. |
| `tasks/dshot_service.rs` | `tools/app-builder/src/backends/stm32f4/tasks/dshot_service.rs` | AB + GA | STM32F4 DShot service/telemetry wrapper; body emitted. | DShot authoring types, shared ESC manager APIs, `fugit`, monotonic. |
| `tasks/esc_manager.rs` | `tools/app-builder/src/backends/stm32f4/tasks/esc_manager.rs` | AB + GA | STM32F4 UART-backed ESC manager wrapper; body emitted. | DShot/UART authoring types, `ferrowasp-stm32f4::memory`, shared ESC manager APIs, monotonic. |
| `tasks/esc_uart_irq.rs` | `tools/app-builder/src/backends/stm32f4/tasks/esc_uart_irq.rs` | AB + GA | STM32F4 USART1 telemetry DMA/IDLE wrappers; two bodies emitted. | STM32F4 serial outcomes and DShot UART authoring type. |
| `tasks/foxeer_safety_master.rs` | `apps/foxeer-f405-v2/tasks/foxeer_safety_master.rs` (EXT) | AB + GA | Foxeer golden safety policy wrapper; body emitted. | Core safety/channels, IO-core SBUS snapshot, STM32F4 owned UART reader, `ferrowasp-tasks` Foxeer safety/telemetry, monotonic. App ownership does not move actuator authority. |
| `tasks/golden_flash.rs` | `apps/foxeer-f405-v2/tasks/golden_flash.rs` (EXT) | AB + GA | Foxeer SPI-NOR logging and bounded live-config persistence policy; body emitted. | Shared flash-storage/config implementation, Foxeer default profile, STM32F4 SPI2 authoring type, telemetry, monotonic. |
| `tasks/heartbeat.rs` | `apps/foxeer-f405-v2/tasks/heartbeat.rs` (EXT) | AB + GA | Foxeer-named diagnostic heartbeat and USB status scheduler; body emitted. | Shared telemetry/status, STM32F4 USB pend mechanism, `fugit`, monotonic. |
| `tasks/imu_control_bridge.rs` | `tasks/imu_control_bridge.rs` (EXT) | AB + GA | MCU-independent IMU sample bridge; body emitted. | Core safety channel, MPU6500-compatible sample type, `fugit`, monotonic. Sensor-type generalization is separate. |
| `tasks/inhibited_actuator.rs` | `tasks/inhibited_actuator.rs` (EXT) | AB + GA | HAL-independent output-inhibited actuator consumer; body emitted. | Core commands/channel and shared actuator logic, monotonic. |
| `tasks/io_watchdog.rs` | `tools/app-builder/src/backends/stm32f4/tasks/io_watchdog.rs` | AB + GA | STM32F4 TIM6/SPI1 deadline-recovery wrapper; body emitted. | Golden-service watchdog authoring type, SPI-IMU owner aliases, monotonic. |
| `tasks/mod.rs` | Split into `tasks/mod.rs`, `apps/foxeer-f405-v2/tasks/mod.rs`, and `tools/app-builder/src/backends/stm32f4/tasks/mod.rs`; external roots registered by `input_catalog.rs` | AB | Mixed current task registry; emits nothing. | All 19 current task files. Preserve stable exported task module names in composition. |
| `tasks/msp_osd.rs` | `tools/app-builder/src/backends/stm32f4/tasks/msp_osd.rs` | AB + GA | STM32F4 owned-UART MSP DisplayPort/tuning wrapper; body emitted. | `embedded-io-async`, STM32F4 memory handles, shared OSD/telemetry/tuning logic, monotonic. |
| `tasks/observe_button_change.rs` | `tasks/observe_button_change.rs` (EXT) | AB + GA | HAL-independent observation-only button event task; body emitted. | Task macro/context only. |
| `tasks/physical_actuator.rs` | `tools/app-builder/src/backends/stm32f4/tasks/physical_actuator.rs` | AB + GA | Safety-owned DShot adapter over the STM32F4 bank; body emitted. | Core authority/commands/channels, shared actuator/ESC qualification, DShot authoring bank, monotonic. The app owns enablement and bindings; the backend wrapper never grants authority. |
| `tasks/serial_discard.rs` | `tools/app-builder/src/backends/stm32f4/tasks/serial_discard.rs` | AB + GA | STM32F4 owned-UART discard consumer for validation apps; body emitted. | `embedded-io-async`, STM32F4 memory reader/capacity. |
| `tasks/simple_osd.rs` | `tools/app-builder/src/backends/stm32f4/tasks/simple_osd.rs` | AB + GA | STM32F4 owned-UART simple OSD validation wrapper; body emitted. | `embedded-io-async`, STM32F4 memory writer, IMU driver type, `fugit`, monotonic. |
| `tasks/usb_cdc.rs` | `apps/foxeer-f405-v2/tasks/usb_cdc.rs` (EXT) | AB + GA | Foxeer-named USB CDC diagnostics and whitelisted command forwarding; body emitted. | Shared flash/telemetry/USB-debug APIs, STM32F4 USB authoring/pend mechanisms, monotonic. No safety or actuator handles. |

## `live_config.rs` authoritative behavior map

There is no current V3 file named `live_config.rs`. The final authored file is
a sibling of `platform_config.rs`:

```text
apps/foxeer-f405-v2/
├── platform_config.rs
└── live_config.rs
```

It is an app-owned selection/wiring boundary compiled by `AB` through
`input_catalog.rs`. It does not replace or duplicate the shared configuration
schema and persistence mechanisms.

| Existing authoritative behavior | Current source | Required preservation in the future app input/generated file |
|---|---|---|
| Public mutable keys and numeric bounds | `crates/ferrowasp-core/src/config.rs` (`ConfigKey`, `ConfigValueSpec`) | Reference the shared API; keep all 21 keys, bounds, finite-value checks, and integer-only fields unchanged. |
| Foxeer tuning defaults and RC-rate defaults | `crates/ferrowasp-tasks/src/drone_toolbox.rs` (`TuningProfile::default_foxeer_f405_v2`, `RC_RATE_PROFILE`) | Select the Foxeer default through shared APIs; do not copy numeric gains/rates into builder-owned code. |
| Stored representation, schema version, legacy migration, validation, and log divisor | `crates/ferrowasp-tasks/src/flash_storage.rs` (`StoredConfig`, encode/decode/set, copy-on-write helpers) | Keep shared implementation authoritative, including schema version 2, legacy migration, `log_rate_divisor` range `1..=16`, sanitized tuning, and persistence verification. |
| V3 initial live values | `hardware_definitions/stm32f4/lower.rs` (`tuning_profile`, `tuning_request_seq = 1`, `flash_log_rate_divisor = 1`) | Generated initialization must select identical values before storage recovery. |
| V3 live shared-resource bindings | `target/app_composition.rs` (`tuning_profile`, `tuning_request_seq`, `flash_log_rate_divisor`, `storage_status`) | The app input owns these bindings; names, types, owners, lock usage, and task consumers remain unchanged. |
| Recovery, staged edits, disarmed-only set/save, alternating slots, read-back verification, and publication | `tasks/golden_flash.rs` plus shared `flash_storage` APIs | Preserve exact fail-closed behavior. Configuration failure may disable storage/configuration but cannot arm or command actuators. |
| Disarmed OSD tuning publication | `tasks/msp_osd.rs` | Preserve the armed check, sanitization, and sequence increment; no direct actuator authority. |
| USB command parsing/forwarding | `tasks/usb_cdc.rs` plus shared `flash_storage::CommandParser` | Preserve the bounded whitelist and queues. USB does not own arming state or actuator handles. |
| Handwritten golden-app integration reference | `apps/foxeer-f405-v2/src/main.rs`, `src/lib.rs`, shared core/tasks crates | Checkpoint 1 pins the current source identity and fixtures. Historical flight evidence is not transferred to generated code. |

The first `live_config.rs` implementation must therefore express existing
app selections and bindings over shared types. It must not introduce a new
wire format, schema version, key, bound, default, persistence algorithm,
write-while-armed path, or safety/actuator capability.

Generated acceptance is behavioral and deterministic, not a fictitious
pre/post file diff: V3 has no original generated `live_config.rs`. Checkpoint
1 must pin the sources above, expected defaults/schema, and repeated output
hash before Checkpoint 2 creates the file.

## Existing canonical builder preservation boundary

`tools/rtic-app-builder` remains untouched through Checkpoint 1. Its inputs,
fixtures, compatibility adapter, command behavior, staging, fingerprints,
resume checks, diagnostics, and release link are protected migration inputs.
The 15 current `xtask/src` modules have these eventual responsibilities in
the isolated `tools/app-builder` workspace; consolidation happens only after
their regression fixtures are retained.

| Existing canonical module | Eventual mainline responsibility |
|---|---|
| `architecture.rs` | RTIC architecture contracts and resolved-graph validation. |
| `assembler.rs` | Transactional generation pipeline and checked promotion. |
| `backend.rs` | STM32F4 backend/build-policy support. |
| `cli.rs` | Scriptable application selection and command surface. |
| `diagnostics.rs` | Bounded retained command diagnostics. |
| `feature.rs` | Compatibility ingestion until typed component/task recipes cover protected fixtures. |
| `lib.rs` | Mainline host library module root. |
| `main.rs` | Mainline CLI binary. |
| `manifest.rs` | Strict BSP/application input parsing for preserved NUCLEO fixtures. |
| `mcu.rs` | MCU profile, target, linker, and probe metadata within backend/build policy. |
| `render.rs` | Crate/linker skeleton rendering coordinated with the V3 RTIC renderer. |
| `runner.rs` | Confined command construction/execution. |
| `state.rs` | Fingerprints, checkpoint state, and resume validation. |
| `syntax.rs` | Rust syntax-aware checked insertion compatibility until the single renderer replaces fragments. |
| `validate.rs` | Pre-mutation strict input and architecture validation. |

The existing `bsp/`, `applications/`, `architecture-contracts/`,
`feature-library/`, `templates/`, `compat/`, and checked NUCLEO fixtures are
retained in place until their Checkpoint 1 protections pass. This map does not
authorize deleting or silently translating them.

## Dependency and ordering consequences

1. Create the isolated `tools/app-builder` shell and `input_catalog.rs` before
   relocating any `EXT` file.
2. Preserve current V3 generated-source hashes and task-source fixtures before
   the first path change.
3. Move generic RTIC/backend host files before external inputs, keeping the V3
   implementation available for comparison.
4. Register board, portable tasks, Foxeer app tasks/components, app
   composition, platform config, and reconciliation explicitly; reject missing
   or duplicate registrations.
5. Add and fixture `live_config.rs` only after its authoritative-source pins
   are recorded. Its creation is not bundled with schema or runtime changes.
6. Prove nested Cargo isolation before moving generated output below the
   existing app directory.
7. Do not remove either existing builder until the applicable mainline-plan
   parity checkpoint passes and the user approves cleanup.

## Checkpoint 0 acceptance

- All 76 V3 Rust sources have one classification row.
- Physical ownership and exact host compiler are explicit.
- Every task records whether its body is emitted into generated firmware.
- No current host-authoring source is assigned to the target runtime crate.
- App-specific Foxeer behavior is supplied through app-owned inputs.
- `platform_config.rs` and `live_config.rs` are sibling app inputs with distinct
  boot-frozen and bounded-live responsibilities.
- Existing canonical-builder capabilities remain protected inputs.
- No file move, workspace mutation, CLI change, safety change, output-gate
  change, hardware change, or generated-output change is claimed here.
