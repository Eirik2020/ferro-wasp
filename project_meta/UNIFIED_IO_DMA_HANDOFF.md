# Objective

> Historical work-package handoff. This file records how the unified I/O/DMA
> refactor was developed; its intermediate “current” statements are not the
> live project status. Use `CODEX_PROJECT_CONTEXT.md`, `CODEX_ACTIVE_WORK.md`,
> and `../mdbook/src/current_support.md` for current behavior.

Start the STM32F4 RTIC 2 unified I/O and DMA refactor on `refactor/hardware`
without changing live motor behavior.

# Repository State Inspected

- Current branch: `refactor/hardware`.
- Existing staged `README.md` edits were present before this pass and were not
  modified.
- The implementation plan file is present as an untracked repository document.
- The current firmware is a single RTIC application with nested support crates,
  not yet the full crate layout proposed by the plan.

# Decisions Applied

- Preserve FerroWasp FCU3 routing.
- Keep the existing safety-gated PWM actuator path live.
- Defer PWM DMA/timer-DMA/DShot integration.
- Begin the workspace split with host-buildable contract and manifest crates.

# Files Changed

- `Cargo.toml`
- `project_meta/ARCHITECTURE_DECISIONS.md`
- `project_meta/UNIFIED_IO_DMA_HANDOFF.md`
- `project_meta/CODEX_ACTIVE_WORK.md`
- `mdbook/src/current_support.md`
- `crates/ferrowasp-io-core/**`
- `crates/ferrowasp-stm32f4/**`
- `crates/ferrowasp-waveform/**`
- `crates/ferrowasp-actuator/**`
- `crates/ferrowasp-bsp/**`
- `crates/ferrowasp-pid/**`
- `crates/ferrowasp-mspv1/**`
- `crates/ferrowasp-drivers/**`
- `crates/ferrowasp-tasks/**`
- `crates/rc-pwm/**`

# Implementation Summary

- Added workspace membership for existing local crates and new skeleton crates.
- Added portable health, timestamp, serial, SPI, waveform, statistics, DShot,
  actuator-authority, and manifest types.
- Added a frozen `ferrowasp_fcu3` BSP manifest matching the current FerroWasp
  FCU3 pin/DMA/timer map.
- Added `ferrowasp-stm32f4` route metadata and compile-shaped HAL DMA route
  checks for the FerroWasp FCU3 map.
- Added host tests for DShot encoding, actuator validation, and duplicate
  manifest claims.

# Commands Run

```powershell
git status --short --branch
```

Result: already on `refactor/hardware`; pre-existing staged `README.md` change
was present.

```powershell
cargo fmt --all --check
```

Result: passed after formatting the new workspace and existing newly included
workspace members.

```powershell
cargo test -p ferrowasp-waveform --target x86_64-pc-windows-msvc
```

Result: passed, 3 tests.

```powershell
cargo test -p ferrowasp-actuator --target x86_64-pc-windows-msvc
```

Result: passed, 2 tests.

```powershell
cargo test -p ferrowasp-bsp --target x86_64-pc-windows-msvc
```

Result: passed, 3 tests.

```powershell
cargo test -p ferrowasp-stm32f4 --target x86_64-pc-windows-msvc
```

Result: passed, 2 tests.

```powershell
cargo test -p ferrowasp-io-core --target x86_64-pc-windows-msvc
```

Result: passed, 0 tests.

```powershell
cargo check --workspace
```

Result: passed. Existing warning remains for an unused import in `src/main.rs`.

```powershell
cargo clippy --workspace --all-targets
```

Result: failed because the configured embedded target cannot build `std`/`test`
harness crates for `thumbv7em-none-eabihf`.

```powershell
cargo clippy --workspace
```

Result: passed with existing prototype warnings.

```powershell
probe-rs list
```

Result: STLINK visible as `STLink V2 -- 0483:3748`.

# Test Results

Host tests for the new portable crates passed. Embedded `cargo check` passed.
No flashing or motor-output hardware test was run in this pass.

# Continuation Notes

- Added `ferrowasp-stm32f4` as a workspace crate.
- Added backend route tables for active serial, SPI, DMA, static PWM, clocks,
  and static storage.
- Added embedded-only HAL DMA route assertions for USART2 RX, UART4 RX/TX,
  SPI1 RX/TX, and ADC1 DMA.
- Added BSP manifest count tests for active DMA streams, timer channels, and
  peripherals.
- Updated `mdbook/src/current_support.md` to say the manifest scaffold exists
  but is not yet the live RTIC construction path.

# Continuation Notes - UART RX Core

- Added a host-tested UART RX DMA/IDLE state model in
  `ferrowasp-stm32f4::serial::rx_state`.
- Added a host-tested UART RX IRQ planner in
  `ferrowasp-stm32f4::serial::irq_plan`.
- Covered these race cases:
  - DMA-full detaches active buffer and advances generation;
  - IDLE detaches partial buffer and suppresses late full IRQ;
  - full IRQ suppresses late IDLE for the old generation;
  - empty IDLE does not advance generation;
  - reported partial length is clamped to buffer capacity.
- Added serial route capability validation for the current USART2 SBUS and
  UART4 MSP routes.
- Added transitional frozen-route constants in `src/stm32f4/init.rs` so the live
  firmware init module points at the new backend metadata without changing
  behavior.
- The live USART2 and UART4 IRQ handlers still use the existing implementation;
  no active IRQ ownership or DMA restart logic was swapped in this continuation.
- Added static storage budget helpers for UART RX buffers, SPI RX buffers, and
  ADC double-buffered samples.

# Continuation Notes - Live UART Planner Bridge

- Added `RxIrqPlanner` ownership to the live `src/stm32f4/uart.rs`
  `UartRxIrqSide`.
- Routed USART2 and UART4 DMA-full interrupt handlers through
  `uart.plan_dma_full()`.
- Preserved current behavior: DMA-full still delivers a full
  `UART_RX_BUFFER_SIZE` chunk, and USART/UART IDLE interrupts still only clear
  the idle condition.
- Added an `rx_generation()` accessor for future diagnostics without exposing
  planner internals.
- Added a direct root dependency on `ferrowasp-io-core` because the live UART
  layer now exposes the shared `StreamGeneration` contract type.

# Continuation Notes - UART DMA Error Path

- USART2 and UART4 RX DMA handlers now check transfer, direct-mode, and FIFO
  error flags before transfer-complete handling.
- DMA error flags route through `uart.plan_dma_error()` so the serial generation
  advances and late events from the failed generation are treated as stale.
- Normal transfer-complete behavior still delivers full `UART_RX_BUFFER_SIZE`
  chunks.

# Continuation Notes - UART RX IRQ Evidence

- Added lightweight live UART RX IRQ counters beside the planner:
  `dma_full_chunks`, `dma_errors`, `stale_events`, and `ignored_events`.
- The counters are updated from planner actions, keeping IRQ logging unchanged
  while making future RTT/telemetry exposure straightforward.
- Tightened the UART4 RX filled-queue-full case to panic instead of warning and
  continuing after buffer ownership has already moved out of DMA. This now
  matches the USART2 fail-fast invariant for unrecoverable RX buffer loss.

# Continuation Notes - Active/Spare UART RX

- Switched live USART2 and UART4 RX DMA setup from HAL double-buffer mode to the
  active/spare single-buffer pattern used by the HAL serial DMA IDLE example.
- The existing four-buffer storage now uses one active DMA buffer and three
  buffers in the free pool.
- USART2 and UART4 IDLE interrupts now compute partial byte count via
  `UART_RX_BUFFER_SIZE - number_of_transfers()`, route the event through
  `uart.plan_idle()`, swap in a fresh DMA buffer with `next_transfer()`, enqueue
  the partial buffer, and spawn the existing parser task.
- The backend now records `LIVE_RX_BUFFERING_MODE = ActiveSpareIdleChunks`.
- Target validation is required for this step because live interrupt-time DMA
  restart behavior changed.

# Continuation Notes - SPI RX DMA Planner Bridge

- Added a host-tested SPI RX IRQ planner in `ferrowasp-stm32f4::spi`.
- Added live SPI RX IRQ stats for completed transfers, DMA errors, and ignored
  events.
- Routed SPI1 RX DMA completion through the planner before rotating the active
  RX buffer.
- SPI1 RX DMA now checks transfer, direct-mode, and FIFO error flags before
  transfer-complete handling; on error it records the planner fault, clears DMA
  flags, deasserts IMU chip-select, and logs a warning.
- The fourth SPI RX buffer is now part of the free pool instead of being
  reserved but unused.
- Corrected the SPI filled-queue ownership panic text so it names SPI1.

# Continuation Notes - ADC1 DMA Planner Bridge

- Added a host-tested ADC DMA IRQ planner in `ferrowasp-stm32f4::adc`.
- Routed the live ADC1 DMA2 Stream0 handler through the planner before rotating
  buffers or publishing battery/current observations.
- ADC1 DMA now ignores non-complete interrupts, reports transfer/direct-mode/FIFO
  DMA errors, clears flags on error, and avoids processing bad samples.
- Replaced the previous unchecked spare-buffer and `next_transfer()` unwrap path
  with explicit panic/warn handling that preserves the existing measurement
  scaling and telemetry outputs.

# Continuation Notes - UART RX Rotation Helper

- Moved the common UART RX buffer rotation sequence into
  `src/stm32f4/uart.rs`:
  planner action, fresh-buffer dequeue, HAL `next_transfer()`, and filled-queue
  enqueue now live in one helper path.
- USART2 and UART4 RTIC handlers still own hardware flag clearing, logging, and
  parser task spawning.
- The bench-validated active/spare RX behavior is preserved; this cleanup
  reduces duplicated full-chunk and IDLE-chunk handling in `src/main.rs`.

# Continuation Notes - SPI RX Rotation Helper

- Moved the common SPI RX buffer rotation sequence into `src/stm32f4/spi.rs`:
  planner action, fresh-buffer dequeue, HAL `next_transfer()`, and filled-queue
  enqueue now live in one helper path.
- The SPI1 RTIC DMA handler still owns DMA flag clearing, IMU chip-select
  cleanup, request lookup, logging, and parser task spawning.
- The bench-validated SPI RX DMA planner behavior is preserved while reducing
  open-coded buffer ownership logic in `src/main.rs`.

# Continuation Notes - ADC1 DMA Sample Helper

- Added `src/stm32f4/adc.rs` as the HAL-specific bridge for ADC1 DMA sample
  completion.
- Moved ADC1 DMA flag planning, spare-buffer rotation, HAL `next_transfer()`,
  flag clearing, and voltage/current millivolt conversion into the helper.
- The RTIC `dma_adc1` task now handles only helper errors, temperature
  calculation, battery/current scaling, and publishing shared telemetry values.
- The bench-validated ADC1 DMA behavior is preserved while reducing open-coded
  buffer ownership logic in `src/main.rs`.

# Continuation Notes - Static PWM / Timer-DMA Boundary

- Extended backend static PWM route metadata with logical motor lanes while
  preserving the FerroWasp FCU3 mapping:
  PA8/TIM1_CH1, PC9/TIM3_CH4, PC8/TIM3_CH3, PB15/TIM12_CH2.
- Added backend tests that pin the current static PWM route order and assert
  timer-DMA/PWM-DMA actuator integration remains explicitly deferred.
- Added ARM-only compile-shaped assertions for the current HAL PWM channel
  aliases; no timer-DMA actuator implementation was added.
- Cleaned one no-op PWM tuple destructuring warning in `src/stm32f4/init.rs`.

# Continuation Notes - MSP UART Fragmentation Coverage

- Added MSP v1 parser tests for frames split across UART chunks, back-to-back
  frames in one chunk, and checksum-error recovery before the next valid frame.
- These tests pin the parser behavior needed by the active/spare UART RX path:
  partial IDLE chunks may arrive at arbitrary byte boundaries.
- Added `Default` implementations for `MspParser`, `MspResponder`, and
  `OsdTask`; each delegates to the existing `new()` constructor.

# Continuation Notes - Mechanical Warning Reduction

- Removed redundant imports from `src/main.rs`.
- Added `Default` implementations for `EdgeDetector` and
  `GyroAngleIntegrator`; both delegate to existing `new()` constructors.
- Collapsed simple conditionals and single-pattern matches flagged by clippy in
  safety qualification, motor mapping, throttle rescaling, UART2 parser spawn,
  and SBUS packet ingestion.
- No runtime behavior, hardware resources, pins, task priorities, or actuator
  authority paths were changed.

# Continuation Notes - Reusable Crate Relocation

- Moved `ferrowasp-pid` from `src/ferrowasp-pid` to
  `crates/ferrowasp-pid`.
- Moved `rc-pwm` from `src/software_drivers/rc-pwm` to `crates/rc-pwm`.
- Moved MSP v1 protocol code into a new `crates/ferrowasp-mspv1` no-std crate.
- Removed the old `src/software_drivers` compatibility facade after target
  validation; the app now imports driver, MSP, task, and waveform crates
  directly.
- SPI2 flash is not a remaining FCU board task; it was only an example in the
  unified plan and this board has no external flash.

# Continuation Notes - Driver and Task Crate Relocation

- Moved the MPU6500 driver from `src/software_drivers/mpu6500.rs` to
  `crates/ferrowasp-drivers/src/mpu6500.rs`.
- Moved the control/mixer/blackbox task logic from
  `src/software_drivers/drone_toolbox.rs` to
  `crates/ferrowasp-tasks/src/drone_toolbox.rs`.
- Moved the DJI O4/MSP OSD task from `src/software_drivers/osd.rs` to
  `crates/ferrowasp-tasks/src/osd.rs`.
- Removed the stale nested `src/software_drivers` package manifest and lockfile.
- DShot packet and waveform helpers are now sourced from `ferrowasp-waveform`;
  no app-local DShot compatibility module remains.
- The OSD menu task now accepts its own `OsdStickRates` input type, avoiding a
  dependency from reusable task code back into the RTIC app crate.

# Continuation Notes - STM32F4 HAL Bridge Extraction

- Moved the ADC1 DMA completion bridge from `src/stm32f4/adc.rs` into
  `ferrowasp-stm32f4::adc`; the FCU3 BSP constructor now invokes this backend.
- Moved the SPI DMA bridge from `src/stm32f4/spi.rs` into
  `ferrowasp-stm32f4::spi_dma`; the FCU3 BSP safely shapes RTIC-owned storage
  and invokes the backend.
- Moved the UART RX/TX DMA bridge from `src/stm32f4/uart.rs` into
  `ferrowasp-stm32f4::uart_dma`; the FCU3 BSP safely shapes RTIC-owned storage
  and invokes the backend.
- Lifted the serial protocol/frame-size contract into
  `ferrowasp-io-core::serial::SerialProtocol`; the old app-local `Mode` name is
  now an alias for that shared protocol type.
- Moved the STM32F4 HAL prelude bucket from `src/stm32f4/common.rs` to
  `ferrowasp-stm32f4::hal_prelude`; the RTIC app imports it directly.
- Moved UART4 TX-DMA setup and ADC1 observation setup into backend helpers.
- Moved static PWM ESC setup into `ferrowasp-stm32f4::static_pwm` while
  preserving PA8/TIM1_CH1, PC9/TIM3_CH4, PC8/TIM3_CH3, PB15/TIM12_CH2; the app
  now imports this backend setup directly.
- Moved USART2/UART4 route pin alternate-function setup into
  `ferrowasp-stm32f4::uart_dma`.
- Moved SPI1 MPU6500 chip-select and bus setup into
  `ferrowasp-stm32f4::spi_dma`.
- Moved plain ADC/UART/SPI resource structs into backend modules and re-exported
  the existing app paths for compatibility initially; the app now imports ADC,
  UART, and SPI backend modules directly.
- Removed `src/stm32f4/init.rs`; board-specific initialization now lives in
  `ferrowasp-bsp::stm32f4::ferrowasp_fcu3::init`.
- Replaced app-side board DMA-storage singleton macros with RTIC
  `#[init(local = [...])]` resources and safe FCU3 storage-shape conversion.
- Removed the empty root `src/lib.rs`; the root package is now just the RTIC
  binary, with reusable code in workspace crates. The ARM-only binary is marked
  `test = false` and `bench = false`; host workspace tests cover the reusable
  crates.

# Continuation Notes - BSP Profile and App-Shell Thinning

- Added `crates/ferrowasp-core` and moved the existing safety types/signals
  into `ferrowasp_core::safety`.
- Removed the old `src/core` compatibility facade after target validation; the
  current RTIC app now imports safety directly from `ferrowasp-core`.
- Forwarded bench motor-selection features from the root crate into
  `ferrowasp-core`, preserving the existing feature-gated actuator command
  variant.
- Moved `throttle_to_u16` command-unit conversion into
  `ferrowasp_core::actuator` and kept the app prelude as a compatibility
  re-export.
- Removed the unused generic low-pass helper from the app prelude; active IMU
  filter behavior remains in `ferrowasp-tasks::drone_toolbox`.
- Removed stale nested `src/core` files; `crates/ferrowasp-core` is now the
  canonical core crate.
- Cleaned core clamp helpers after preserving the existing non-finite actuator
  rejection tests.
- Moved portable UART consumer routing policy into
  `ferrowasp-io-core::serial`, with tests pinning the current USART2 RC input
  and UART4 OSD routing assumptions.
- Added typed board profile constants in
  `ferrowasp-bsp::stm32f4::ferrowasp_fcu3::profiles` for:
  - ADC voltage divider, current scale, and battery cell count;
  - current FCU IMU control-axis index/sign mapping;
  - startup gyro-bias calibration sample count and motion threshold.
- Switched `src/main.rs` to consume these BSP profile constants instead of
  hard-coding board-specific ADC and IMU mapping values locally.
- Added BSP tests that pin the current FCU ADC scaling and IMU mapping values.
- Moved pure OSD/telemetry packing helpers into `ferrowasp-tasks::osd`:
  throttle/rate MSP RC mapping, cell-voltage packing, and current-sense packing.
- Added task-crate tests for these OSD/telemetry conversions.
- Moved control-axis gyro mapping and startup gyro-bias calibration into
  `ferrowasp-tasks::drone_toolbox`, with tests preserving the current FCU axis
  convention and stillness/motion behavior.
- Moved OSD TX queue overflow policy into `ferrowasp-tasks::osd` as
  `enqueue_tx_frame`; the live policy remains drop-oldest/enqueue-newest, and a
  test pins the `heapless::spsc` usable capacity behavior.
- Tightened reusable task APIs so gyro angle integration uses rate/accel arrays
  and compact BB2 construction uses a named fields struct instead of long
  positional argument lists.
- Replaced several app-local raw HAL type spellings in `src/main.rs` with
  backend aliases for ADC transfer, SPI1 CS, and static PWM motor channels.

# Known Limitations / Blockers

- PWM DMA/timer-DMA actuator integration is not implemented yet.
- The live RTIC app intentionally owns concrete HAL resources; board-specific
  construction is delegated to the FCU3 BSP.
- The plan's example USART1, USART6, and TIM1/TIM3 DShot routes are not active
  because the FerroWasp FCU3 mapping is authoritative.
- SPI2 flash is not applicable to the current board.

# Exact Next Work Package

Work Packages 0-2 are complete for the FerroWasp FCU3 map. The next
non-deferred step is to continue moving reusable task behavior out of the RTIC
shell without changing ownership, priorities, or fault behavior.
PWM-DMA/timer-DMA/DShot remains parked for the later actuator work requested by
the user.

# Continuation Verification - 2026-07-17

- Marked the ARM-only root binary `test = false` and `bench = false`, allowing
  host workspace tests to cover all reusable crates without compiling the RTIC
  firmware for the host.
- Replaced stale CI references to deleted nested `src` manifests with workspace
  host tests and host all-target Clippy.
- Added embedded workspace Clippy, duplicate dependency reporting, and a
  workspace-owned unsafe-source rejection step to CI.
- Added the seven RTIC software dispatcher IRQ reservations to the frozen BSP
  manifest, with a host test pinning the list used by the current app shell.
- Added explicit BSP facts and host checks for SWD preservation, optional USB
  pin isolation, hardware IRQ/dispatcher separation, and one mode per timer.
- Moved serial protocol routing and SPI/DMA/static-PWM route metadata out of the
  STM32F4 mechanism crate and into the reference BSP.
- Added `#![forbid(unsafe_code)]` to `ferrowasp-mspv1`; all workspace crates now
  carry the policy.
- Removed obsolete nested `Cargo.lock` files from the moved PID and `rc-pwm`
  workspace crates. The root lockfile remains authoritative.
- `cargo fmt --all --check`: passed.
- `cargo check --workspace`: passed.
- `cargo check --features "blackbox_defmt bench_motor1_only"`: passed.
- `cargo clippy --workspace`: passed.
- `cargo clippy --workspace --all-targets --target
  x86_64-pc-windows-msvc --exclude ferro-wasp`: passed.
- `cargo test --workspace --target x86_64-pc-windows-msvc`: passed, 93 unit
  tests plus doc tests.
- Workspace source scan found no `unsafe` usage in `src/` or `crates/`.
- `cargo tree --workspace --duplicates` reports only transitive compatibility
  boundaries, notably `embedded-hal` 0.2/1.0, `heapless` 0.8/0.9, and supporting
  `defmt`/`bare-metal` versions.

# Continuation Notes - UART/SPI IRQ Wrapper Thinning

- Moved UART RX DMA flag inspection, error planning, buffer rotation, and flag
  clearing into `ferrowasp-stm32f4::uart_dma::UartRxIrqSide`.
- Moved UART IDLE length calculation, partial-buffer delivery, and IDLE
  clearing into the same generic backend.
- Moved SPI RX DMA flag inspection, buffer rotation, and flag clearing into
  `ferrowasp-stm32f4::spi_dma::SpiRxIrqSide`.
- Kept RTIC IRQ bindings and priorities unchanged:
  - SPI1 RX DMA: `DMA2_STREAM2`, priority 13;
  - USART2 RX DMA and IDLE: `DMA1_STREAM5`/`USART2`, priority 11;
  - UART4 RX DMA and IDLE: `DMA1_STREAM2`/`UART4`, priority 6.
- Kept SPI chip-select ownership, parser spawning, port-specific logging, and
  existing fail-stop buffer-ownership policy in the RTIC app.
- Embedded workspace check, bench-feature check, embedded/host Clippy, and 93
  host unit tests plus doc tests pass.

This checkpoint is runtime-adjacent and requires the normal target build,
flash, RTT/LED boot check, SBUS activity check, and UART4 OSD check before more
UART/SPI ownership work.

Target validation passed on 2026-07-17:

- firmware build and flash succeeded;
- RTT showed valid IMU responses;
- ADC battery voltage was plausible;
- UART4 OSD updated throttle and battery voltage;
- the debug LED continued running with no observed crash.

This clears the IRQ-wrapper checkpoint for continued SPI ownership work. SBUS
activity was not explicitly reported in this checkpoint and remains part of the
next full RC-path validation.

# Work Package 3-5 Gaps

- UART active/spare buffers, DMA-full/error handling, IDLE partial delivery,
  and generation suppression exist.
- A standard async serial reader abstraction and sustained loopback/stress
  evidence are not implemented.
- The current board intentionally instantiates USART2 and UART4 only; unused
  plan-example UART1/UART3 routes are not added.
- UART4 has a bounded TX-DMA worker; generalized configured-port TX and boot
  validation remain incomplete.
- SPI1 performs the live MPU6500 register burst with bounded static buffers.
- SPI1 bus, TX/RX DMA halves, chip select, active request, owned job, and
  transaction lifecycle now have one shared backend owner.
- Owned RX copy-back, a standard async `SpiDevice`, deadline enforcement with
  owner-controlled hardware abort/recovery, and logic-analyzer evidence remain
  outstanding.

# Continuation Notes - SPI Poller Cleanup

- Removed the allocated but unused SPI request queue, producer, and consumer.
  The live implementation had never enqueued or consumed a request.
- Removed stale `imu_request` and `rc_input_raw` RTIC shared resources and their
  unused task declarations.
- Moved SPI TX DMA release, register-read frame preparation, transfer rebuild,
  and start into `SpiPollerSide::start_register_read`.
- Kept request labeling and chip-select assertion in the priority-12 RTIC poll
  task, at the same point before DMA start.
- The SPI1 RX DMA binding remains `DMA2_STREAM2` at priority 13.
- Formatting, embedded workspace/bench-feature checks, embedded and host
  Clippy, and 93 host tests plus doc tests pass.

Because TX DMA start sequencing moved into the backend after the prior hardware
validation, this checkpoint requires another build/flash and confirmation that
continuous MPU6500 samples still arrive over RTT and the debug LED remains
alive. ADC/OSD behavior was not changed by this cleanup.

Target validation passed:

- IMU sequence advanced by about 1,601 samples per two-second heartbeat log,
  matching the intended approximately 800 Hz poll rate;
- raw gyro values continued updating and responded to motion;
- gyro bias calibration completed;
- RTT and heartbeat LED remained alive.

The reported initial battery value of `0.1V` was expected because this test used
USB-only FCU power. Battery/OSD readings should only be treated as powered-path
evidence when the user explicitly connects that hardware.

# Continuation Notes - Portable SPI Transaction Lifecycle

- Added `SpiTransactionState` to `ferrowasp-io-core` with the planned bounded
  states: free, prepared, pending, active, cancel-requested, completing,
  completed, and faulted.
- Added transaction and wrapping generation tokens.
- Added explicit busy and invalid-transition errors.
- Added deadline checks that request cancellation rather than recovering
  hardware outside the owner.
- Added owner-finalized cancellation with distinct timeout/cancel faults.
- Added DMA fault handling and stale-generation rejection.
- Added five host tests covering successful reuse, occupied-slot rejection,
  timeout cancellation, pending cancellation, and late IRQ rejection after a
  newer transaction starts.
- Formatting, embedded workspace/bench-feature checks, embedded/host Clippy,
  and 98 host tests plus doc tests pass.

At this checkpoint the portable lifecycle was not yet connected to the live
SPI1 IMU transfer, so no additional flash was required. It is connected by the
later single-owner checkpoint below.

# Continuation Notes - Owned SPI Job

- Added `OwnedSpiJob<MAX_OPS, MAX_BYTES>` with bounded `heapless` operation
  metadata and fixed TX/RX storage.
- Added owned metadata for read, write, transfer with unequal read/write
  lengths, transfer-in-place, and bounded delay operations.
- Packing copies caller TX bytes and operation metadata into a staged job; the
  owner slot retains no caller borrows.
- Packing is atomic: operation/byte limit failures leave the previous job
  unchanged.
- RX copy-back validates the complete operation shape before modifying any
  caller destination and copies only read-bearing results.
- Added five host tests for mixed packing, copy-back, TX capacity, operation
  capacity, and copy-back shape rejection.
- Formatting, embedded workspace/bench-feature checks, embedded/host Clippy,
  and 103 host tests plus doc tests pass.

At this checkpoint the owned job was portable and not yet connected to live
SPI1, so no target flash was required. Its TX packing and metadata are
connected by the later single-owner checkpoint below.

# Continuation Notes - Live SPI1 Single Owner

- Added `SpiDmaOwner` in `ferrowasp-stm32f4`. It exclusively owns the SPI1 RX
  and TX DMA halves, MPU6500 chip select, current request label, one
  `OwnedSpiJob`, and one `SpiTransactionState`.
- Replaced the separate RTIC shared chip-select/register resources and local
  TX/RX DMA resources with one shared `Spi1Mpu6500Owner`.
- The priority-12 poll task now submits the existing fixed 15-byte
  `AccelXoutH` burst to the owner. The request cadence and wire frame are
  unchanged.
- The `DMA2_STREAM2` priority-13 handler now services completion through the
  same owner. Terminal completion and DMA/delivery faults release chip select
  and recycle or fault the matching lifecycle token.
- An IRQ without an active lifecycle token clears the DMA flags and does not
  rotate a stale RX buffer into the parser.
- The DMA halves are private inside the owner, preventing app code from
  bypassing its lifecycle.
- Existing pin, DMA, timer, interrupt, parser queue, and actuator mappings are
  unchanged. This step does not touch motor authority or static PWM output.
- Formatting, embedded workspace/bench-feature checks, embedded and host
  Clippy, and 103 host tests plus doc tests pass.

The owner records a 250 us transaction deadline, but the live firmware does
not check it yet. The current RTIC monotonic also has 1 ms resolution. Before
enabling timeout recovery, add a suitably precise time source and an
owner-controlled DMA/SPI abort sequence, then test late-IRQ suppression and
recovery on target. The live MPU path still delivers RX buffers to the existing
parser queue; owned-job RX copy-back and async `SpiDevice` are not active.

This checkpoint changes the live SPI1 path and requires target validation:

- build and flash with the existing `cargo embed` workflow;
- confirm system init and gyro bias calibration complete;
- confirm heartbeat raw-gyro values update and sequence advances by roughly
  1,600 samples every two seconds;
- move the FCU and confirm the raw gyro values respond;
- confirm no repeated SPI busy/start/DMA errors appear;
- confirm the heartbeat/debug LED continues running without a crash.

ADC and OSD behavior were not changed. With USB-only power, the previously
observed near-zero battery reading remains expected.

Target validation passed on 2026-07-17:

- firmware initialized and gyro bias calibration completed;
- raw gyro heartbeat samples remained plausible and stable;
- IMU sequence advanced from 1,603 to 14,415 in eight two-second intervals,
  consistently alternating increments of 1,601 and 1,602 samples;
- no SPI busy, transaction-start, or DMA errors were reported;
- RTT remained alive throughout the supplied capture.

The reported `0.0V` initial battery value was expected because the FCU was
USB-powered for this test. This clears the live single-owner checkpoint.

At that checkpoint, the planned timeout source still needed a board-level
decision: the unified plan reserved TIM2 as the RTIC timebase while the
preserved FCU mapping used TIM2 as the control-loop scheduler. The following
checkpoint records the explicit decision and reassignment.

# Continuation Notes - Generic Control Scheduler And TIM2 Timebase

The user explicitly authorized internal timer reassignment while keeping every
external pin mapping frozen.

- Moved the current board's 800 Hz scheduler interrupt from TIM2 to unclaimed,
  pinless TIM4. The existing sample divider still produces the 400 Hz control
  update.
- Added generic STM32F4 scheduler construction and interrupt acknowledgement;
  the backend accepts any suitable HAL timer instance.
- Added board target aliases for the selected scheduler timer/type. RTIC's
  `binds = TIM4` remains explicit in the app shell because hardware interrupt
  names are compile-time macro inputs.
- Assigned 32-bit TIM2 to a 1 MHz free-running I/O timebase.
- Added portable 32-bit wrap extension and tests across normal and rollover
  observations.
- SPI transaction start deadlines now use TIM2 microsecond timestamps instead
  of the 1 ms SysTick timestamp. Timeout checking and recovery are still
  disabled in this checkpoint.
- TIM1/TIM3/TIM12 motor channels, all motor pins, SPI/UART/ADC pins and DMA
  routes, task priorities, control rates, and actuator authority are unchanged.
- Updated the BSP manifest so TIM4 is the control scheduler and TIM2 is the
  microsecond timebase with non-overlapping ownership.
- Formatting, embedded workspace/bench-feature checks, embedded Clippy, and
  105 host tests plus doc tests pass.

This timer reassignment changes the live control interrupt and requires target
validation before adding the 8 kHz I/O watchdog:

- build and flash with the normal `cargo embed` workflow;
- confirm system initialization and gyro bias calibration;
- confirm IMU sequence still advances by about 1,600 samples per two seconds,
  which exercises the TIM4 scheduler-driven SPI poll;
- confirm raw gyro values respond to movement;
- confirm no SPI busy/start/DMA errors;
- confirm RTT and the heartbeat/debug LED remain alive.

No powered motor test is required for this checkpoint.

Target validation passed on 2026-07-17:

- system initialization and gyro bias calibration completed;
- IMU sequence advanced by 1,601 or 1,602 samples per two-second interval;
- raw gyro values responded clearly when the FCU was moved;
- no SPI busy/start/DMA errors were reported;
- RTT remained alive throughout the supplied capture.

The USB-powered `0.0V` battery reading was expected. This clears the TIM4
scheduler and TIM2 microsecond-timebase checkpoint for watchdog integration.

# Continuation Notes - SPI Deadline Watchdog And DMA Recovery

- Added a pinless TIM6 8 kHz I/O watchdog at priority 9.
- TIM6 reads the TIM2 microsecond timebase and only probes the active SPI
  deadline. It performs no peripheral recovery itself.
- An expired deadline posts a bounded priority-13 `spi1_timeout` software task,
  serialized with the priority-13 SPI1 RX DMA IRQ.
- Reserved `CAN2_RX0` as one additional RTIC software dispatcher. It has no
  external pin or hardware-peripheral role.
- The SPI owner now pauses TX/RX DMA, deasserts CS, replaces the partial RX
  buffer, restarts RX DMA, advances the lifecycle generation, and recycles the
  job after a 250 us timeout.
- Added a fifth 15-byte SPI RX buffer dedicated to timeout recovery. Static SPI
  RX storage increased from 60 to 75 bytes.
- A failed RX restart retains the recovery buffer, marks the owner unavailable,
  and rejects later requests with one-shot RTT logging.
- Added timeout and recovery-failure statistics.
- Added the opt-in `bench_spi_timeout_recovery` feature. It intentionally
  stalls only the first owned SPI1 transaction before TX DMA starts, forcing a
  deterministic timeout; later transactions use the normal path.
- All external pin mappings, motor timers, actuator authority, rates, and DMA
  routes remain unchanged.
- Formatting, normal and injection embedded checks, both Clippy paths, and 106
  host tests plus doc tests pass.

Safe-HAL limitation:

- the split SPI DMA handles support safe stream pause/restart;
- they do not expose safe SPI DMA-request disable, SPI busy/flush, or peripheral
  reset operations;
- this checkpoint therefore validates bounded DMA ownership recovery, not full
  SPI peripheral reinitialization after every possible electrical/bus fault;
- no workspace `unsafe` was added to bypass the HAL boundary.

Target validation should be staged:

1. Flash the normal build first. Confirm calibration, about 1,600 IMU samples
   per two seconds, motion response, no timeout warnings, and a live heartbeat
   LED.
2. Then flash with `--features bench_spi_timeout_recovery`. Expect exactly one
   `SPI1 transaction timed out; DMA ownership recovered` warning near startup,
   followed by calibration and continuously advancing IMU sequence values.
3. Confirm no `SPI1 timeout recovery failed; owner disabled` or
   `SPI1 owner unavailable after recovery failure` warning.

No powered motor test is required.

Normal and injected target validation passed on 2026-07-18.

Normal build:

- initialized and calibrated without a timeout warning;
- IMU sequence advanced from 1 to 3,204 at the expected approximately 800 Hz;
- RTT remained alive and no recovery failure was reported.

`bench_spi_timeout_recovery` build:

- emitted one expected initial IMU-stale warning while the deliberately stalled
  transaction was active;
- emitted exactly one `SPI1 transaction timed out; DMA ownership recovered`
  warning;
- gyro calibration then completed;
- IMU sequence advanced to 1,602 and 3,203, one sample behind the normal path
  because the injected transaction was intentionally discarded;
- no recovery-failed or owner-unavailable warning was reported.

The USB-powered `0.0V` battery values were expected. This clears the SPI
deadline-watchdog and bounded DMA-recovery checkpoint.

# Continuation Notes - Async SPI1 Device And Owned Mailbox

- Added exact workspace dependencies on `embedded-hal` and
  `embedded-hal-async` 1.0.
- Added a unique, non-cloneable `AsyncSpiDevice` in `ferrowasp-io-core`.
- Added a bounded `SpiRequestMailbox<MAX_OPS, MAX_BYTES>` containing one owned
  job, lifecycle/generation state, terminal result, and owned waiter.
- Submission copies operation metadata and TX bytes before the future can
  become pending. No caller borrow is stored in static or IRQ-owned state.
- Successful RX data remains owned until the waiting future validates the
  operation shape and performs copy-back.
- Dropping a pending future marks its exact generation abandoned, requests
  owner cancellation, and never frees active DMA state from task context.
- Terminal faults wake an existing future and recycle when observed; abandoned
  results are discarded only after owner finalization.
- Added a short-critical-section executor that pends an owner callback and
  never holds its mailbox lock across `.await`.
- Refactored the live SPI1 owner to consume the mailbox. The priority-13 owner
  service starts DMA; `DMA2_STREAM2` copies the completed 15-byte frame into the
  owned job before retaining the existing parser-queue delivery.
- The priority-12 poll task now issues a real
  `embedded-hal-async::SpiDevice::transaction`.
- The existing parser remains the live MPU6500 state producer for this first
  target checkpoint, allowing direct comparison with prior RTT behavior.
- TIM6 timeout recovery and the `bench_spi_timeout_recovery` injection now
  finalize the same mailbox generation awaited by the async task.
- Static storage accounting now includes the architecture-sized owned mailbox
  in addition to the five 15-byte SPI RX DMA buffers.
- The portable layer supports bounded standard read, write, transfer,
  transfer-in-place, and delay operation shapes. The live STM32F4 engine
  currently accepts only the existing single 15-byte full-duplex MPU burst;
  variable-length/multi-operation DMA sequencing remains open.
- No external pins, DMA routes, timer assignments, control rates, parser
  behavior, motor resources, or actuator authority changed.
- No workspace `unsafe` was added.

Verification completed before target testing:

- `cargo fmt --all --check`;
- `cargo check --workspace`;
- embedded feature check with `blackbox_defmt`, `bench_motor1_only`, and
  `bench_spi_timeout_recovery`;
- embedded library/binary Clippy with warnings denied;
- host library Clippy with warnings denied;
- 116 host unit tests plus doc tests;
- `git diff --check`;
- workspace unsafe scan.

The broad `cargo clippy --workspace --all-targets` command is not valid for the
embedded default target because it tries to link host `std` test harnesses for
`thumbv7em-none-eabihf`. Embedded library/binary Clippy and host test-target
checks are run separately.

This live ownership change requires target validation:

1. Flash the normal build. Confirm init and gyro calibration, IMU sequence
   increments of about 1,600 per two seconds, motion response, no SPI backend
   or timeout warnings, and continued RTT/LED liveness.
2. Flash with `--features bench_spi_timeout_recovery`. Expect one initial stale
   sample warning and one timeout-recovered warning, followed by calibration
   and continuously advancing IMU sequence values.
3. Confirm no cancellation-recovery-failed, timeout-recovery-failed, or
   owner-unavailable warning.

No powered motor test is required. Logic-analyzer chip-select and timing
evidence remains open after functional target validation.

## Target Regression - Deadline Started Before Owner Dispatch

Initial target testing of the async-mailbox checkpoint failed in both the
normal and `bench_spi_timeout_recovery` builds. RTT emitted continuous
`SPI1 transaction timed out; DMA ownership recovered` warnings instead of the
expected normal completion or one-shot injected timeout.

Cause:

- the mailbox started the 250 us deadline when priority-12 submitted the
  request;
- priority-13 software-owner dispatch latency was therefore charged against
  the active SPI wire budget;
- the previous direct-start path did not include that added scheduling stage.

Correction:

- pending requests retain a 250 us deadline from submission;
- owner activation rebases the active transport deadline to a fresh TIM2
  timestamp immediately before DMA start;
- the timeout value was not relaxed;
- added lifecycle and mailbox tests proving expiry is measured from the
  rebased owner timestamp.

Post-correction static verification:

- 27 focused `ferrowasp-io-core` tests pass;
- embedded workspace check passes;
- normal and injected firmware builds link;
- embedded and host Clippy pass with warnings denied;
- all 118 host unit tests plus doc tests pass;
- formatting, diff checks, and the workspace unsafe scan pass.

Target validation remains pending. Reflash the normal build first; it must
produce no timeout-recovered warning. Only after normal completion is restored
should the injected build be checked for exactly one recovery warning.

Corrected normal-build target validation passed:

- firmware initialized and the heartbeat remained active;
- one expected startup stale tick occurred while the first async transaction
  was pending;
- the first IMU sample arrived with sequence 1;
- gyro bias calibration completed;
- IMU sequence advanced through 1,602, 3,203, 4,805, 6,407, 8,008, and 9,610,
  matching the intended approximately 800 Hz sample rate;
- raw gyro values remained plausible;
- no SPI timeout, DMA, backend, recovery-failure, or owner-unavailable warning
  appeared.

The USB-powered `0.0V` battery value was expected. This clears the corrected
normal async SPI1 path. The one-shot `bench_spi_timeout_recovery` target check
remains pending.

Corrected timeout-injection target validation passed:

- firmware initialized and the heartbeat remained active;
- the deliberately stalled first transaction produced one expected startup
  stale tick;
- exactly one `SPI1 transaction timed out; DMA ownership recovered` warning
  appeared;
- gyro bias calibration completed after recovery;
- IMU sequence advanced through 1,601, 3,202, and 4,804 at the intended
  approximately 800 Hz rate;
- raw gyro values remained plausible;
- no repeated timeout, cancellation-recovery-failed,
  timeout-recovery-failed, backend, or owner-unavailable warning appeared.

The USB-powered `0.0V` battery value was expected. This clears functional
target validation of the async SPI1 mailbox, owner dispatch, owned RX
copy-back, generation-scoped timeout, and bounded DMA recovery. Work Package 5
still needs logic-analyzer chip-select/timing evidence and broader
variable-length/multi-operation backend support; the current FCU MPU6500 path
is functionally complete for the planned fixed 15-byte burst.

The normal firmware was reflashed after injection testing:

- startup produced one transient stale tick followed by IMU sequence 1;
- gyro bias calibration completed;
- sequence advanced to 1,602 and 3,203 at the expected rate;
- no timeout or SPI backend warning appeared.

The user does not currently have a logic analyzer. Chip-select and wire-timing
capture is therefore deferred until suitable equipment is available and is not
an immediate blocker for continued non-actuator work on this branch.

# Continuation Notes - Owned Async Serial RX And UART4 Bridge

- Added exact workspace dependencies on `embedded-io` 0.7.1 and
  `embedded-io-async` 0.7.0.
- Replaced the unused borrowed serial `RxChunk` placeholder with a bounded
  owned chunk carrying valid length, timestamp, DMA-full/IDLE completion cause,
  stream generation, discontinuity-before, and UART-error metadata.
- Added `SerialRxChannel`, its single producer, an
  `embedded_io_async::Read` consumer with an internal chunk offset, and a
  separate discontinuity/status observer.
- A pending read retains only its waker. Dropping it does not consume queued
  data or retain the caller's output buffer.
- Queue overflow is bounded: the new chunk is rejected, the overflow counter
  advances, a structured discontinuity is recorded, and the next accepted
  chunk is marked as following a discontinuity.
- Added structured `embedded_io::Error` support for serial faults.
- Added host tests for owned-copy validation, partial reads, adjacent chunk
  behavior, overflow/discontinuity propagation, dropped pending reads,
  disabled streams, and compile-time async trait conformance.
- STM32F4 detached UART buffers now retain their completion cause and
  generation from the IDLE/full race planner.
- UART4 MSP/OSD is the first live bridge. `osd_refresh` copies each detached
  DMA buffer into the owned channel, immediately recycles the DMA buffer, and
  consumes bytes through `embedded_io_async::Read` before invoking the existing
  MSP parser.
- At this checkpoint, USART2 SBUS remained on the previously validated direct
  parser route pending UART4 target evidence and explicit
  continuity-to-RC-validity integration. The later USART2 continuation notes
  record that migration.
- The current UART4 bridge still uses the existing filled-buffer queue as its
  IRQ-to-task handoff. Direct IRQ-owner publication and reader wakeup are a
  later serial checkpoint.
- Static accounting now reflects both four-buffer, 70-byte UART DMA ports
  (560 raw DMA bytes) and the target-resolved `size_of` the four-chunk owned
  UART channel.
- No external pins, baud/parity/stop settings, DMA routes, IRQ priorities,
  protocol consumers, motor resources, or actuator authority changed.
- No workspace `unsafe` was added.

Static verification:

- normal and `bench_spi_timeout_recovery` embedded workspace checks pass;
- embedded library/binary Clippy passes with warnings denied;
- host all-target Clippy passes for all host-buildable workspace crates with
  warnings denied;
- all 125 host unit tests plus doc tests pass;
- formatting, diff checks, and the workspace unsafe scan pass.

UART4 target validation remains:

1. Flash the normal build and confirm normal init, IMU sequence progress,
   heartbeat LED, and no new UART4 owned-RX warnings.
2. With the DJI O4/MSP link powered, confirm battery, throttle, and attitude
   OSD updates still respond.
3. Exercise MSP requests for several minutes and confirm no
   `UART4 RX discontinuity`, `owned RX queue rejected`, or
   `owned RX reader failed` warning.

No powered motor test is required.

UART4 owned-RX target validation passed on 2026-07-18:

- the normal firmware built and flashed successfully;
- the IMU remained healthy;
- the DJI O4 OSD displayed and continued updating;
- OSD throttle followed live RC stick input, proving the UART4 MSP request,
  owned RX channel, async reader, existing parser, and telemetry response path
  remained connected;
- no UART4 discontinuity, queue rejection, reader failure, SPI, or other RTT
  warning appeared.

This cleared the staged UART4 owned-reader bridge. USART2/SBUS remained
unchanged until RC transport-loss invalidation became explicit in the later
USART2 continuation checkpoint.

# Continuation Notes - Portable Async Serial TX Contract

- Added bounded owned `TxChunk` storage and validation.
- Added `SerialTxChannel`, `SerialWriter`, and a serialized `SerialTxOwner`.
- `SerialWriter` implements the workspace `embedded_io_async::Write` trait.
- `write` copies at most one chunk and waits for queue capacity without
  changing shared state while pending.
- Standard `write_all` splits larger byte ranges into ordered bounded chunks.
- `flush` waits for both an empty queue and explicit hardware-owner completion
  of the in-flight chunk.
- The owner can await work, report completion, disable the stream, report a
  terminal fault, and recover after backend-owned hardware recovery.
- Terminal owner faults clear queued bytes and wake pending capacity, owner,
  and flush waiters.
- Added host tests for bounded partial writes, ordered `write_all`, queue-full
  wakeup, true flush semantics, canceled pending writes, owner wakeup, empty
  writes, fault propagation, and standard trait conformance.
- The live UART4 DMA polling and OSD TX queues are unchanged. Integrating the
  portable writer with a persistent UART4 TX worker and DMA1 Stream 4
  completion IRQ is the next TX hardware checkpoint.
- No pins, DMA routes, IRQ bindings, protocol behavior, motor resources, or
  actuator authority changed.
- No workspace `unsafe` was added.

Verification:

- normal and `bench_spi_timeout_recovery` firmware builds pass;
- embedded library/binary Clippy passes with warnings denied;
- host all-target Clippy passes for all host-buildable workspace crates with
  warnings denied;
- all 133 host unit tests plus doc tests pass;
- formatting, diff checks, and unsafe-source checks pass.

No target reflash is required for this portable-only checkpoint.

# Continuation Notes - Live UART4 Async TX DMA

- Split portable TX ownership into a protocol-side `SerialWriter`, serialized
  `SerialTxOwner`, and IRQ-side `SerialTxCompletion`.
- Completion and terminal fault publication wake a worker suspended on hardware
  completion; `flush` still waits for both an empty queue and no in-flight
  transfer.
- Replaced the live OSD application queue and 10 ms DMA completion polling with
  a persistent priority-4 UART4 TX worker.
- Bound DMA1 Stream 4 at priority 6. The IRQ clears completion/error flags and
  publishes exactly one terminal result for the current owned chunk.
- Added a host-testable STM32F4 TX DMA lifecycle. Overlapping starts are
  rejected, and duplicate or stale completion/error events are ignored.
- Preserved the validated fixed 70-byte UART4 DMA transfer by zero-padding
  shorter owned MSP chunks. Variable-length DMA storage remains a later safe-HAL
  backend checkpoint.
- Reserved `CAN2_RX1` as the ninth RTIC software dispatcher required by the new
  persistent worker. The BSP manifest now also claims DMA1 Stream 4 as a
  hardware IRQ and verifies no dispatcher overlap.
- Static memory accounting includes the 16-chunk owned UART TX channel.
- PA0/PA1, UART4 mode, DMA1 Stream 4 Channel 4, MSP routing, motor pins,
  actuator resources, and safety authority are unchanged.
- PWM/DShot DMA integration remains explicitly deferred.
- No workspace `unsafe` was added.

Static verification:

- normal and `bench_spi_timeout_recovery` embedded workspace checks pass;
- embedded library/binary Clippy passes;
- all 141 host unit tests plus doc tests pass;
- formatting and the BSP resource-conflict tests pass;
- default-target `cargo clippy --workspace --all-targets` remains inapplicable
  because it requests host test harnesses for the no-std thumb target.

UART4 target validation remains:

1. Flash the normal build with the DJI O4/MSP link powered.
2. Confirm firmware init, heartbeat, IMU sequence progress, and gyro bias
   calibration.
3. Confirm the OSD appears and battery/throttle values update, including live
   RC throttle movement.
4. Exercise MSP traffic for several minutes.
5. Confirm no UART4 TX writer, worker, DMA-start, completion-state, or DMA-error
   warning appears.

No powered motor test is required.

First target attempt and correction:

- the IMU remained active, but UART4 TX entered a terminal state and repeated
  `UART4 TX writer rejected MSP frame`;
- inspection found the IRQ handler promoted the HAL's advisory DMA FIFO-error
  flag to a terminal fault;
- the handler now clears FIFO error without completing or failing the active
  transfer, matching the HAL UART DMA policy;
- true transfer and direct-mode errors remain terminal and now have distinct
  RTT messages;
- writer-side terminal reporting is one-shot, preventing an RTT flood while RX
  buffer recycling and the rest of the OSD task continue;
- host tests prove FIFO-only is non-terminal, FIFO plus transfer-complete still
  completes, and transfer error takes precedence over completion;
- normal and injected firmware builds, embedded and host Clippy, formatting,
  and all 141 host tests pass after the correction.

Corrected UART4 target validation passed on 2026-07-18:

- the OSD displayed and continued updating;
- OSD throttle followed live RC stick input, exercising UART4 RX requests,
  MSP parsing, bounded async TX queueing, the persistent DMA owner, and DMA1
  Stream 4 completion;
- the powered battery reading was plausible at 23.0 V;
- the heartbeat remained active and gyro bias calibration completed;
- IMU sequence advanced from 2 through 28,831 over approximately 36 seconds,
  consistent with the intended approximately 800 Hz sample rate;
- one expected startup IMU-stale tick occurred before the first async sample;
- no UART4 writer, worker, DMA-start, completion-state, transfer-error, or
  direct-mode-error warning appeared.

This cleared functional target validation of the corrected UART4 async TX
path. USART2/SBUS then remained on its validated direct parser path until the
portable RX discontinuity path could immediately invalidate RC validity; that
migration is recorded below.

# Continuation Notes - USART2 Owned RX And RC Validity

- Added a reusable STM32F4 owned-RX bridge that copies one detached DMA buffer
  into a bounded portable `RxChunk`, recycles the static DMA buffer
  immediately, and publishes explicit queue/recycle outcomes.
- Migrated USART2 SBUS from per-IRQ parser spawning to one persistent
  priority-10 `embedded_io_async::Read` consumer.
- USART2 DMA1 Stream 5 and USART2 IDLE IRQs publish completed chunks close to
  the hardware owner and report DMA/transport discontinuities immediately.
- Added explicit `RcLinkState` safety policy:
  - invalid at startup;
  - valid after three consecutive healthy SBUS frames;
  - invalid after 100 ms without a healthy frame;
  - invalid on transport loss, DMA error, parser error, SBUS frame-lost, or
    SBUS failsafe;
  - a newly observed low arm switch is required after every invalidation.
- Arming and actuator-idle completion recheck RC validity. An accepted
  invalidation revokes actuator permission, clears armed state, and posts
  disarm through the existing actuator owner.
- RC setpoints are neutralized when the protocol task observes parser,
  failsafe, frame-lost, reader, or discontinuity failure.
- A transport discontinuity resets the incremental SBUS parser before
  post-gap bytes are consumed.
- Repeated identical invalidations are deduplicated, and a previously healthy
  disarmed link produces one observable timeout transition rather than a
  startup warning or RTT flood.
- Static storage accounting now includes owned RX channels for both live UART
  ports.
- USART2 remains PA2/PA3 at 100000 baud, even parity, two stop bits, DMA1
  Stream 5 Channel 4. IRQ priorities, timers, motor pins, and actuator
  authority are unchanged.
- PWM/DShot DMA remains explicitly deferred.
- No workspace `unsafe` was added.

Static verification:

- normal and `bench_spi_timeout_recovery` firmware builds pass;
- embedded workspace Clippy passes;
- all 145 host unit tests plus doc tests pass;
- formatting and diff checks pass.

Target validation remains:

1. Flash the normal build with receiver power and the arm switch low.
2. Confirm one `RC link valid after healthy-frame qualification` message,
   normal IMU progress, OSD operation, throttle response, and heartbeat.
3. Interrupt RC frames for longer than 100 ms with actuators unpowered.
4. Confirm exactly one timeout, SBUS failsafe, or frame-lost invalidation
   warning and no warning flood.
5. Restore frames with the arm switch high and confirm no arm request.
6. Move the arm switch low, then high for the existing qualification time and
   confirm arming can only be requested after that low observation.

No powered motor test is required.

USART2 target validation passed on 2026-07-18 with the DJI O4 Air Unit Lite,
DJI Goggles 3, and DJI RC3:

- normal RC traffic qualified the link through the owned async reader;
- removing goggles power interrupted the RC link and produced one frame-timeout
  invalidation without repeated warnings;
- restoring the link produced healthy-frame qualification;
- the RC3 arm toggle had been set high while disconnected, but restoring the
  link did not automatically request arming;
- the first arm-button press observed the required low state and did not arm;
- the second press created a fresh low-to-high transition and produced
  `RC Requests ARM!` followed by the existing BLHeli PWM arming sequence;
- IMU sampling remained active throughout.

This clears functional target validation of USART2 owned RX, timeout
invalidation, recovery qualification, and the post-loss rearm latch. Actuators
were unpowered for the test.

# Continuation Notes - Bounded Arming-Idle Abort

- Added a portable arming guard with explicit reasons for revoked permission,
  invalid RC link, low arm switch, high throttle, and completion-delivery
  failure.
- The arming throttle boundary is now consistently
  `ARMING_MAX_THROTTLE`, currently 65 command counts.
- The priority-15 actuator owner checks all guard inputs every 10 ms during the
  existing 2.5-second low hold and 500 ms idle hold.
- A failed guard writes all four PWM outputs low and clears idle completion
  before posting `ArmingAborted` to the priority-16 safety master.
- Arm-low and RC-loss events may revoke permission while the actuator task is
  occupied; its own guard then performs the bounded forced-low action without
  relying on another instance of the single-instance actuator executor.
- Successful completion uses a small priority-13 notifier so the actuator task
  returns before the safety master performs final arming checks.
- The notifier retries if the safety executor is momentarily occupied.
- Existing PWM timing, hold durations, external pins, motor order, timers, DMA
  routes, and actuator authority are unchanged.
- PWM/DShot DMA remains explicitly deferred.
- No workspace `unsafe` was added.

Target validation remains props-off with actuators unpowered:

1. Start with throttle zero and arm low; wait for RC qualification.
2. Request arm and, after `Applying low throttle`, raise throttle above 65
   before the 2.5-second hold finishes.
3. Confirm one throttle-high arming-abort warning, no `Applying idle throttle`,
   and no `SYSTEM ARMED`.
4. Return throttle to zero and toggle arm low before making another request.
5. Let one normal sequence complete and confirm the existing arming flow still
   reaches `SYSTEM ARMED`.
6. Toggle arm low and confirm normal disarm.

No powered motor test is required.

Arming-idle abort target validation passed on 2026-07-18 with actuators
unpowered:

- two throttle-high attempts aborted during the low hold with one explicit
  warning each;
- neither failed attempt applied idle or declared the system armed;
- a later throttle-low request completed both holds and reached
  `SYSTEM ARMED`;
- IMU sampling remained at approximately 800 Hz without new stale warnings.

The RC-link rearm latch also prevents arm-high at boot or reconnection from
immediately arming: a low arm state must be observed before a later high
transition can qualify.

# Continuation Notes - SPSC Motor Command Authority

- The priority-14 control loop now exclusively owns `MotorCmdWriter`.
- The priority-15 actuator task now exclusively owns `MotorCmdReader`.
- Active mixer output, equal-motor mode, physical selected-motor modes, and
  logical selected-motor modes all publish timestamped, sequenced commands
  through the four-entry SPSC queue.
- `ApplyLatestThrottle` and `ApplyBenchSelectedMotor` are wake-up commands only;
  no actuator RTIC spawn carries motor values.
- The actuator drains to the newest queued entry and rejects missing commands
  or commands older than `MOTOR_CMD_MAX_AGE_MS`, currently 20 ms, before the
  existing finite-value and range validation.
- Missing, stale, or invalid commands apply low output.
- Queue overflow or a rejected actuator wake requests disarm through the safety
  master.
- Disarm and BLHeli arming entry discard all queued commands, preventing replay
  across arm cycles.
- Removed the unused `UnsafeApplyThrottle` command variant.
- Added `bench_motor_cmd_stale_rejection`; its first armed command is
  deliberately 21 ms old for a props-off target rejection check.
- External pins, motor mapping, PWM timing, timers, DMA routes, arming behavior,
  and actuator authority are unchanged.
- PWM/DShot DMA remains explicitly deferred.
- No workspace `unsafe` was added.

Static verification:

- normal firmware and `bench_spi_timeout_recovery` builds pass;
- `pwm_cal`, equal-motor, physical selected-motor, logical selected-motor, and
  stale-command injection feature checks pass;
- all 147 host unit tests plus doc tests pass;
- embedded workspace Clippy and host all-target Clippy pass;
- formatting passes.

Target validation remains props-off with actuators unpowered:

1. Flash the normal build, arm with throttle low, exercise throttle, and
   disarm.
2. Confirm normal IMU, heartbeat, RC, and OSD behavior with no motor-command
   queue, wake, missing, or stale warning.
3. Flash with `--features bench_motor_cmd_stale_rejection`.
4. Arm with actuators unpowered and confirm exactly one stale-command warning
   for sequence 1 while the firmware remains responsive.
5. Confirm no queue-overflow or rejected-wake warning, then disarm.

This checkpoint validates freshness when a command reaches actuator output.
An independent deadline watchdog for complete loss of future control-loop
wakes remains a separate safety work package.

The stale-command injection target check passed on 2026-07-18 with actuators
unpowered:

- BLHeli low and idle holds completed normally and the safety master declared
  `SYSTEM ARMED`;
- the first queued motor command was rejected exactly once as sequence 1 at
  21 ms old;
- no motor-command queue-overflow or rejected-wake warning appeared;
- IMU sequence continued at approximately 800 Hz after the rejection;
- an initial SBUS parser error recovered through healthy-frame qualification
  before arming.

The normal-build no-warning arm, throttle-response, and disarm check remains
before closing target validation of this work package.

# Continuation Notes - Explicit FerroWasp FCU3 BSP Target

- Renamed the active board target from the transitional
  `generic_f405_reference` name to `ferrowasp_fcu3`, board name
  **FerroWasp FCU3**.
- Added board identity for STM32F405RGT6/LQFP64 and kept the externally
  validated pin map unchanged.
- The typed FCU3 manifest now claims both UARTs, SPI1/MPU6500, ADC1, four PWM
  outputs, PB0/PB1 debug LEDs, six DMA routes, hardware/dispatcher IRQs, and
  TIM2/TIM4/TIM6 roles. SWD and optional USB pins remain explicit reservations.
- Added tests for identity, exclusive resource claims, motor order, timer
  compatibility, hardware/dispatcher IRQ separation, profile values, serial
  capability, debug LEDs, and correspondence between every active DMA route
  and its manifest claim.
- Reversed the transitional dependency: `ferrowasp-bsp` depends on the
  reusable `ferrowasp-stm32f4` backend, and the backend no longer selects or
  depends on a board.
- Moved FCU3 target aliases, board pin conversion, device construction, and
  safe DMA-storage shaping into the BSP.
- RTIC `#[init(local = [...])]` resources own board DMA storage. No
  board-storage singleton macro or workspace `unsafe` was introduced.
- Removed the obsolete `src/stm32f4` adapter. The root `src` directory now
  contains only the RTIC app shell in `main.rs`.
- External pins, DMA streams, timer roles, interrupt priorities, motor order,
  safety authority, and runtime policy are unchanged.
- PWM/DShot DMA remains deferred.

Static verification:

- normal debug and release firmware builds pass;
- SPI timeout and stale motor-command fault-injection builds pass;
- PWM calibration, equal-motor, physical/logical selected-motor, and USB
  feature checks pass;
- all 148 host unit tests plus doc tests pass;
- embedded workspace and host all-target Clippy pass;
- mdBook, formatting, dependency-direction, workspace-unsafe, and diff checks
  pass.

Target validation remains:

1. Flash the normal build with actuators unpowered.
2. Confirm system initialization, heartbeat LED alternation, and advancing IMU
   sequence without repeated transport warnings.
3. With the DJI link powered, confirm RC qualification, OSD display/update,
   throttle response, and the expected battery reading for the chosen power
   source.
4. Arm at low throttle with actuators unpowered, confirm the normal low-to-idle
   sequence, then disarm.
5. Confirm no motor-command queue, missing-command, stale-command, UART, SPI,
   or ADC warning attributable to the BSP/storage move.

The normal-build FCU3 target smoke test passed on 2026-07-18. After flashing,
the user reported that the system appeared to be working properly. This clears
the immediate runtime checkpoint for the BSP initialization and static
DMA-storage relocation. Detailed subsystem evidence remains represented by the
earlier IMU, RC, OSD, ADC, SPI-timeout, arming, and stale-command target checks.

# Continuation Notes - NUCLEO-F401RE Multi-Target Proof

- Added the typed `nucleo_f401re` BSP target for STM32F401RET6/LQFP64.
- The Nucleo capability manifest explicitly disables attached-IMU and actuator
  capabilities.
- Added an isolated sibling application under `apps/stm32f401-bringup` so
  F401 and F405 HAL/PAC features are never unified in one Cargo build graph.
- The app uses 84 MHz HSI, a 512 KiB flash / 96 KiB SRAM linker map, PA5 LD2,
  PA2 USART2 TX at 115200 baud, and SysTick for one-second heartbeat timing.
- Added ST-LINK `probe-rs` and Cargo Embed profiles for STM32F401RE.
- No RC, sensor, PWM, motor, or actuator path is present in the Nucleo app.
- Added CI compilation for the isolated Nucleo firmware.
- Existing FCU3 debug firmware still builds after BSP feature isolation.

Target validation passed on 2026-07-18:

- DBGMCU ID code `0x433` confirmed that the connected target is an STM32F401
  device before programming.
- The firmware was flashed and then allowed to run with the debugger detached.
- Windows identified `COM4` as the STMicroelectronics STLink Virtual COM Port.
- USART2 produced monotonically increasing
  `FerroWasp NUCLEO-F401RE heartbeat <sequence>` lines at 115200 8N1.
- Ten sampled 30-second checkpoints completed without a timeout or sequence
  rollback. The final checkpoint advanced from sequence 802 to 833.
- GPIOA ODR bit 5 changed between `0x20` and `0x00`, confirming that the
  firmware toggled PA5 for LD2.
- No sensor, RC, PWM, motor, or actuator resources were present in the target.

Visual confirmation that the physical LD2 package follows PA5 remains useful,
but the GPIO register transition, serial heartbeat, and five-minute standalone
soak complete the initial multi-target bring-up checkpoint.

# Continuation Notes - Isolated RTIC App Packages

- Converted the repository root into a virtual workspace for reusable crates.
- Moved the unchanged F405 RTIC shell and its linker, runner, and Cargo Embed
  files to `apps/stm32f405-flight`.
- The F405 app selects FCU3 through the default
  `board-ferrowasp-fcu3` feature and keeps the `FerroWasp` binary name.
- Kept `apps/stm32f401-bringup` as the separate F401 bring-up contract, with
  Nucleo selected through its default board feature.
- Each app now owns an independent Cargo lockfile and PAC feature graph.
- Updated CI, VS Code, RTT terminal tooling, and FerroDebugger build helpers
  to build the F405 package explicitly.
- No runtime logic, external pin, DMA, timer, interrupt, motor mapping, safety
  authority, or PWM behavior changed.

Static verification passed:

- F405 and F401 debug/release builds and Clippy;
- F405 SPI-timeout, stale-command, blackbox/equal-motor,
  blackbox/selected-motor, PWM-calibration, USB, and DShot feature checks;
- 153 reusable-crate host tests plus doc tests;
- reusable-crate host and embedded Clippy;
- root, F405-app, and F401-app formatting checks;
- mdBook, workspace unsafe-source scan, and diff checks;
- the FerroDebugger bridge produced
  `target-codex-fresh/thumbv7em-none-eabihf/release/FerroWasp` from the new app.

One normal FCU3 target smoke test with actuators unpowered remains before
closing the layout migration. The currently connected Nucleo was not flashed
with the F405 image. PWM/DShot DMA remains deferred.

# Continuation Notes - F401 Minimal RTIC Shell

- Replaced the Nucleo polling entry point with a minimal RTIC 2 application
  based on the F405 app-shell conventions.
- The app contains only RTIC initialization and one persistent priority-1
  async heartbeat task.
- SysTick is the 1 kHz RTIC monotonic; EXTI0 is reserved as the software-task
  dispatcher in the Nucleo resource manifest.
- The existing 84 MHz HSI clock, PA5 LD2, PA2 USART2 TX, 115200-baud heartbeat,
  one-second period, linker map, and ST-LINK runner are unchanged.
- No sensor, RC, OSD, ADC, control, PWM, motor, or actuator authority was
  copied from the F405 flight app.
- The original polling image's five-minute target result remains board and
  peripheral evidence.
- The post-conversion RTIC image passed its target smoke test on 2026-07-18;
  the user flashed it to the connected Nucleo and confirmed that it works.
  This closes the immediate scheduler/LED/USART bring-up checkpoint without
  claiming a new extended-duration soak.

# Continuation Notes - Foxeer F405 V2 BSP And App

- Added the typed `foxeer_f405_v2` BSP for STM32F405RGT6/LQFP64 with an 8 MHz
  HSE and 168 MHz system clock.
- Added a separate `apps/foxeer-f405-v2` RTIC package and binary so the FCU3
  resource contract and image remain independent.
- Implemented the current FerroWasp subset: SPI1 mode-3 identity probe and
  runtime-selected MPU6500/ICM42688-P data path, USART2 SBUS, UART4 MSP
  DisplayPort, ADC voltage/current observation, and four conventional RC PWM
  motors.
- Assigned active DMA streams without colliding with the four documented
  deferred timer-DMA/DShot routes.
- Implemented M1/M4 on TIM1 and M2/M3 on TIM8. M4 is explicitly
  `TIM1_CH3N`; both advanced-timer output gates start disabled.
- Preserved PA13/PA14 for SWD instead of claiming the status LEDs.
- Added an explicit arming inhibit for unverified IMU identity/orientation,
  ADC calibration, motor order, and M4 complementary-output polarity.
- Preserved FCU3's measured physical motor behavior through its
  Betaflight-convention logical-to-physical map `[3, 4, 2, 1]`, and added
  Foxeer's provisional Betaflight logical-to-physical map `[1, 2, 3, 4]`.
- Made an unsupported or failed IMU probe nonfatal; MPU6500 identity `0x70`
  selects the `0x3b` burst and ICM42688-P identity `0x47` selects the `0x1d`
  burst. Both use the existing 15-byte DMA transport.
- Kept PWM DMA/DShot, M5-M8, SPI flash, analog OSD, I2C, buzzer, camera
  control, and LED strip outside the active contract.
- Added a direct USB DFU build/flash script targeting application origin
  `0x08000000`; no Foxeer target was flashed during implementation.
- The Foxeer PWM bank validates and prepares all four compares before opening
  either advanced-timer `MOE` gate. Motor pads remain GPIO-low until timer
  polarity and off-state configuration is complete.
- Static verification passed: four formatting scopes, 182 host tests, strict
  host and embedded workspace Clippy, strict Clippy and release builds for all
  three apps, two Foxeer feature combinations, mdBook, diff checks, the
  workspace unsafe-source scan, and DFU binary generation.
- The recorded baseline non-USB Foxeer binary is
  `apps/foxeer-f405-v2/target/thumbv7em-none-eabihf/release/FerroWaspFoxeerF405V2.bin`;
  that build was 72,352 bytes with SHA-256
  `F552B767FD834168361E722A74C0626713D58DF013A7F2AC0FDF7DE7D5E90122`.
  Physical boot, peripheral, and waveform evidence remains pending.

# Continuation Notes - ICM42688-P And Foxeer RC PWM

- Added an allocation-free `ferrowasp-drivers::icm42688p` driver using
  `embedded-hal` 1.0 SPI, output-pin, and delay traits.
- The driver verifies reset completion and `WHO_AM_I=0x47`, preserves
  big-endian sensor data while disabling I2C, configures 1 kHz low-noise
  accel/gyro output at +/-16 g and +/-2000 dps, reads configuration back, and
  observes the documented reset and gyro-startup delays.
- Added blocking register/sample APIs, a 15-byte temp/accel/gyro burst
  decoder, physical-unit conversion helpers, stuck-frame rejection, and
  chip-select deassertion on SPI failure.
- Foxeer now selects MPU6500 or ICM42688-P from the identity probe before DMA
  starts. The request register and parser are tied to that detected kind;
  unsupported identities and configuration failures keep sampling disabled.
- Invalid parser frames now return their static DMA buffer before the task
  continues, preventing gradual exhaustion of the receive pool.
- The active Foxeer motor protocol is explicitly conventional 400 Hz RC PWM,
  1000..2000 us, on M1-M4. The specialized TIM1/TIM8 owner remains necessary
  for M4 `TIM1_CH3N`; only the actuator path owns it. PWM DMA and DShot remain
  deferred.
- Foxeer arming remains compile-time inhibited. Identity, orientation, ADC
  calibration, motor order, and M4 polarity still require physical evidence.

The two bullets above describe the earlier bring-up checkpoint. They are
superseded by the 2026-07-22 default-DShot flight-candidate state recorded in
`CODEX_ACTIVE_WORK.md` and
`project_meta/testing/targets/common-boot-idle.md`.

# Continuation Notes - Foxeer Read-Only USB Debug

- Replaced the Foxeer one-shot USB hello stub with an opt-in CDC ACM
  diagnostic stream on the board's reserved PA11/PA12 OTG_FS route.
- Added the host-tested, allocation-free `ferrowasp-tasks::usb_debug`
  formatter. Its worst-case `FWDBG1` line fits a fixed 224-byte frame.
- The existing roughly two-second heartbeat snapshots RC qualification and
  coalesces one status request. OTG_FS priority 5 formats and writes from
  atomics using bounded 64-byte RX and 256-byte TX storage.
- Status includes uptime, selected IMU/readiness, IMU and control sequences,
  raw gyro, stale state, RC validity/armability, throttle, arm switch, system
  arm state, pack decivolts, and centiamps.
- USB RX drains one bounded packet and discards it. The endpoint has no parser,
  config writer, safety handle, arming request, or actuator resource.
- Added optional USB claims to the Foxeer BSP without changing the default
  active resource set or any external motor/RC/OSD mapping.
- Added `-UsbDebug` to `flash-dfu.ps1` and a bounded-line Windows COM reader in
  `read-usb-debug.ps1`.
- Normal and `usb_serial` Foxeer checks pass, as do 186 reusable host tests.
  USB enumeration, reconnect behavior, live field values, and host-RX flood
  behavior still require target validation.
- The release USB diagnostic `.bin` generated by
  `.\flash-dfu.ps1 -BuildOnly -UsbDebug` is 79,672 bytes with SHA-256
  `120E02F024D4D9C4AE903C075C3D1C9F67BF834F33F0AB0488A8610E74A60173`.
- Physical USB testing on 2026-07-18 enumerated the image as COM6 after a
  normal reconnect and produced the read-only header plus advancing `FWDBG1`
  frames. `imu=icm42688p ready=1` confirms the mode-3 `WHO_AM_I=0x47` probe
  and configuration path. IMU and control sequences advanced at roughly
  800 Hz and 400 Hz with `stale=0`; motion changed the reported gyro values.
- A subsequent bounded COM6 soak captured 152 continuous frames over
  300.584 seconds. Derived rates were 799.996 Hz for IMU samples and
  400.001 Hz for control updates; the maximum report gap was 2001 ms.
  There were no malformed/stale frames, readiness failures, timestamp or
  sequence regressions, serial errors, disconnects, or safety-state changes.
- The USB sequence/stale/gyro portion of the five-minute gate passes.
  Accel/temperature and explicit RTT-warning observation remain open. USB-only
  `vbat_dV=0` is expected, while `current_cA=793..820` remains uncalibrated.
- `read-usb-debug.ps1` now accepts `-DurationSeconds` for bounded captures;
  omitting it preserves the interactive Ctrl+C reader.

# Continuation Notes - Foxeer Cargo DFU Runner

- Added `apps/foxeer-f405-v2/tools/dfu-runner.ps1` and configured the app-local
  Cargo runner so `cargo run --release --locked` flashes through
  STM32CubeProgrammer.
- The runner discovers CubeProgrammer from an override, `PATH`, standard
  installation paths, or STM32CubeCLT under `C:\ST`.
- It validates ELF magic and the `0x08000000` load origin, hashes the exact
  ELF, enumerates `USBn` DFU ports, handles explicit multi-device selection,
  writes, verifies, and starts execution at `0x08000000`.
- STM32CubeProgrammer 2.23.0 requires a recognized filename extension even
  for a valid Cargo ELF. The runner therefore makes a temporary `.elf` copy
  for programming and removes it in a `finally` block.
- `-ListOnly` and `FERROWASP_DFU_DRY_RUN=1` provide non-destructive discovery
  and complete Cargo-runner checks.
- The existing `flash-dfu.ps1` retains `.bin` generation and `-UsbDebug`, but
  now delegates flashing to the CubeProgrammer runner instead of requiring
  `dfu-util`.
- Added VS Code tasks for DFU discovery and normal/USB-debug Cargo flashing.
- This machine already has STM32CubeProgrammer 2.23.0, STM32CubeCLT 1.17.0,
  the signed STM32 bootloader driver, `thumbv7em-none-eabihf`, objcopy, and
  readelf installed. No system installation or driver change was required.
- Physical ROM-DFU testing on 2026-07-18 found exactly one intended `USB1`
  device (`0x0413`). CubeProgrammer programmed and verified the 77.80 KiB
  USB-debug ELF at `0x08000000`; a separate `-s 0x08000000` operation started
  it successfully and the board left DFU mode. The guide's original `-rst`
  command was rejected because CubeProgrammer limits it to JTAG/SWD.

# Continuation Notes - FCU3 Single-Motor DShot600

- The user explicitly opened the previously deferred FCU3 DShot work.
- Added an opt-in physical-M1 backend on the existing PA8/TIM1_CH1 pin through
  DMA2 Stream1 Channel6. The normal four-channel RC PWM image is unchanged.
- The app requires `dshot bench_motor1_only`; conflicting bench modes and PWM
  calibration fail at compile time. PC9, PC8, and PB15 are held low.
- Host-tested waveform code now validates DShot value bounds, DShot600 timing
  at a 168 MHz TIM1 clock, and the compare-event DMA sequence.
- TIM1_CH1 compare events load bits 14..0 and a terminating zero. The
  priority-16 DMA handler stops TIM1 after the final falling edge and leaves
  CCR1 at zero.
- A priority-13 actuator service is the sole frame starter and repeats stop or
  the latest authorized M1 value every 2 ms. Actuator commands only update the
  requested value and lease, preserving the fixed 500 Hz cadence. Nonzero
  values have a 20 ms lease; expiry selects stop and requests disarm. During
  the guarded 500 ms arming-idle phase, the actuator owner rechecks permission,
  RC link, arm switch, and throttle every 10 ms before renewing the normal
  20 ms lease.
- A frame still in flight after 1 ms is treated as a lost completion:
  TIM1/DMA are stopped, the backend is faulted, and disarm is requested.
- The unsafe boundary is isolated in `ferrowasp-stm32f4::dshot` and documents
  only the TIM1 CCR1 address/width, the fixed DMA route, and bounded PAC
  writes. HAL `Transfer` retains DMA-buffer ownership and fencing.
- Final static verification passes 195 host tests, strict host/ARM Clippy,
  default PWM and `dshot bench_motor1_only` release builds, mdBook,
  formatting, and the expected feature-conflict failures. The DShot release
  ELF binds IRQ 57 to `DMA2_STREAM1` at `0x0800464C` (vector word
  `0x0800464D`), has SHA-256
  `8388A10B45376B5D26CC570812FFB2F8F969F91500791B0AC258501F5D7908DA`,
  and ends its loadable flash image at `0x08011D20`.
- The initial unpowered FCU3 target checkpoint passed on 2026-07-18 for at
  least 12,000 frame starts at the intended 500 Hz service rate. Snapshots
  consistently showed `completed = started - 1`, zero busy, lease-expiry,
  timeout, and fault counts, and continuing IMU sample progress.
- The first powered props-off M1 attempt on 2026-07-18 showed that the ESC
  decoded idle frames and spun the motor. The original one-shot 520 ms lease
  then expired before the asynchronous 500 ms hold returned; stop frames were
  selected and the safety master disarmed as intended.
- The fix renews the normal 20 ms lease every 10 ms only after a successful
  arming-guard check.
- The corrected-image powered arming-idle checkpoint passed on 2026-07-18:
  M1 value `112` spun the motor, the sequence reached `BLHeli ESCs idling` and
  `SYSTEM ARMED`, and zero RC throttle then selected value `0`. Frame
  completion continued with zero busy, lease-expiry, timeout, and fault
  counts.
- The final powered props-off run completed physical-M1 interoperability. The
  motor followed RC throttle as expected through requested DShot values `112`,
  `177`, `167`, and `219`, returned to value `0` on explicit disarm, and
  reached 10,000 frame starts with zero busy, lease-expiry, timeout, and fault
  counts while IMU sampling continued.
- Logic-analyzer waveform evidence remains open; props-off ESC interoperability
  does not establish electrical pulse widths or jitter.
- This historical M1 checkpoint has been superseded by the four-motor FCU3
  bench backend below. Bidirectional telemetry, special commands, flight use,
  and Foxeer DShot remain separately deferred.

# Continuation Notes - FCU3 Four-Motor DShot600

- Extended the opt-in FCU3 backend to all four unchanged external motor pins:
  M1 PA8/TIM1_CH1/DMA2 Stream1 Channel6,
  M2 PC9/TIM8_CH4/DMA2 Stream7 Channel7,
  M3 PC8/TIM8_CH3/DMA2 Stream4 Channel7, and
  M4 PB15/TIM1_CH3N/DMA2 Stream6 Channel6.
- Replaced the public single-lane owner with one `DshotMotorBank` that owns
  TIM1, TIM8, all four pins, four HAL DMA transfers, and all static waveform
  buffers.
- TIM1 exports CEN as TRGO and TIM8 waits on ITR0 in trigger mode. All four DMA
  streams are enabled before TIM1 starts, giving the two timer domains one
  hardware start event instead of independent software starts.
- M4 uses `CC3NP=0` active-high polarity. With TIM1_CH3 disabled and only
  TIM1_CH3N enabled, RM0090 Table 96 defines the pad as
  `OC3N = OC3REF xor CC3NP`, matching the ordinary active-high channels.
- A frame set completes only after all four DMA lanes finish. Any lane error,
  unexpected or duplicate interrupt, or 1 ms deadline miss faults the entire
  bank, closes both MOE gates, drives every timer output to its configured low
  idle state, and requests disarm.
- The base build contract is `dshot bench_equal_motors`. The default remains
  four-channel RC PWM. The DShot bench path retains RC qualification, guarded
  arming, the 250-count cap, 500 Hz frame cadence, and 20 ms nonzero-command
  lease.
- Added fixed-route compile assertions, BSP profile/manifest claims, duplicate
  DMA-stream checks, unchanged-pin tests, four priority-16 DMA IRQ tasks, and
  per-lane plus frame-set diagnostics.
- Unsafe code remains isolated to two documented trait-implementation templates
  expanded for four private DMA endpoints in `ferrowasp-stm32f4::dshot`; each
  documents its fixed CCR address, halfword transfer width, DMA route, and
  ownership contract. The RTIC
  app continues to deny unsafe code.
- Final static verification passes 196 host tests plus doc tests,
  DShot-feature tests, strict host/default/DShot Clippy, default PWM and
  four-motor DShot release builds, formatting, mdBook, feature-conflict guards,
  the unsafe-source scan, and diff checks.
- The optimized image binds `DMA2_STREAM1`, `DMA2_STREAM4`, `DMA2_STREAM6`,
  and `DMA2_STREAM7` at `0x080048F4`, `0x08004CFC`, `0x08004D54`, and
  `0x08004DAC`. Its ELF SHA-256 is
  `AA3A5A3D96B0B97BD1FA99031A4AA6FCD2A0A37650B8F4AEAA8121BFBF5FF13C`;
  loadable flash ends at `0x08012EC0`.
- The unpowered FCU3 target checkpoint passed on 2026-07-18 for at least 12,000
  frame starts. Every snapshot showed `completed = started - 1`, four equal
  lane counters matching completed sets, and zero busy, lease-expiry, timeout,
  and fault counts. No spurious warning appeared, while IMU sequence numbers
  continued from at least 3,204 through 28,829.
- The first powered props-off four-motor attempt did not pass. M1-M3 spun, but
  physical M4/front-right did not. Equal requested values, equal completion
  counters, explicit disarm, and zero backend faults isolate the failure from
  command generation and DMA completion. The tested pre-correction ELF had
  SHA-256
  `5FE1AE6883E6448A89541731CA3F61F5758E065B9BA2CBAD32F4E81380F702F2`.
- The tested image incorrectly set `CC3NP=1`, selecting active-low on the only
  enabled CH3 output. Source now uses the PAC's typed `active_high()` setting,
  and the same latent error was corrected in the arming-inhibited Foxeer PWM
  bank. The Foxeer correction remains without target evidence.
- The corrected FCU3 powered props-off checkpoint passed on 2026-07-18. The
  operator skipped the separate short unpowered rerun and proceeded directly
  to the motor test. Before arming, the capture showed synchronized stop-frame
  accounting through 3,000 starts. All four motors, including M4/front-right,
  spun and responded to equal capped RC throttle values `112`, `123`, `129`,
  and `158`. Explicit disarm selected four zeros.
- The corrected run reached 10,000 frame starts with
  `completed = started - 1`, identical lane counters, and zero busy,
  lease-expiry, timeout, and fault counts while IMU sampling continued.
- This validates four-lane ESC interoperability and the M4 polarity correction.
  Waveform timing/jitter, TIM1/TIM8 phase alignment, measured RC-loss stop
  latency, motor numbering/direction, bidirectional telemetry, special
  commands, flight use, and Foxeer DShot remain open.
- The functional powered props-off RC-loss/recovery checkpoint passed on
  2026-07-18. Link loss selected sustained four-lane stop, arm-high link
  recovery did not automatically rearm, a fresh low-to-high transition
  completed normal guarded arming, and explicit disarm restored stop. The
  retained excerpt begins after timeout invalidation and the initial stop
  transition, so those events and exact stop latency were not captured.
  Retained values remained `[0, 0, 0, 0]` through at least starts 44,000 to
  47,000 before the fresh manual arm, all lane counters remained equal, all
  backend counters remained zero, and IMU sequence reached 126,527. After
  rearming, four idle values `112` were observed; disarm restored zeros through
  at least 52,000 starts.
- The next props-off mapping stage is cleared for target execution. Exactly one
  `bench_logical_motorN_only` feature may be added to the required DShot base
  pair. It uses the committed `[3, 4, 2, 1]` logical-to-physical map and the
  same capped `MotorCmd`/actuator path. Physical selections, multiple logical
  selections, PWM calibration, full mixed output, and default DShot remain
  compile-time excluded.
- All four logical variants compile. The current equal-motor RC-loss candidate
  has ELF SHA-256
  `F36B3D1C9B468FBC4999CC9771A0E9E72F4DA2A6913F11B9B0F0684782E97224`,
  unchanged DShot IRQ addresses, and loadable flash end `0x08012FB0`.
- The logical-motor 1/2/3/4 release candidates have SHA-256 values
  `A3E569EDFADC5E64DED1A1AC4147610E540A6F56B337262672AF4D5B33D9BA9A`,
  `8626339FEDDAD7A7EB9CFA606E669A354443AFCD96DF115419C4983510AFE152`,
  `373D40848C86BE9FBEE2360E97E728C088B1E164588979DBDFFFA99EDF6C0C04`,
  and
  `1BE803EF05BF3B5694400538F903FC3BD46C3554E37CB9222870136F3607C65D`
  respectively. All four retain the expected DMA IRQ addresses and loadable
  flash end `0x080130D8`. These candidates are ready for sequential props-off
  mapping tests and are not yet target evidence.
- The operator subsequently ran the exact logical-motor 1/2/3/4 feature
  commands and observed rear-right/front-right/rear-left/front-left
  respectively. This target result confirms physical outputs 3/4/2/1 and the
  committed `[3, 4, 2, 1]` map. Rotation direction, flashed-ELF hash
  confirmation, and per-run RTT backend counters were not included and remain
  open evidence items.
- Added `bench_dshot_unequal_motors` as the next bounded four-lane packet
  checkpoint. It requires the existing DShot/equal-motor base gate, cannot be
  combined with logical selection or PWM calibration, and does not enable the
  PID mixer. Below a 100-count trigger it requests stop; above the trigger it
  maps fixed logical `[140, 120, 100, 80]` to physical
  `[80, 100, 140, 120]` and wire values `[127, 147, 187, 167]`.
- The unequal-vector path uses the same fresh `MotorCmd` queue, armed-only
  actuator owner, 20 ms lease, synchronized four-lane service, and bank fault
  containment. Host tests pin its trigger/remap and exact wire values.
- The source-side unequal-vector release candidate has ELF SHA-256
  `78FD890B9D93DCA8D1A456548F0C89DB6153561496C1BB42B8D42238676DABF3`,
  unchanged DShot IRQ addresses, and loadable flash end `0x080130E8`. Static
  verification passes 203 host tests plus doc tests, strict host/embedded
  Clippy, all relevant F405 feature variants, default/equal/unequal release
  builds, and intended feature-conflict failures. The flashed ELF hash was not
  independently read back from the target.
- Unequal-vector Part A passed functionally on target on 2026-07-18. The
  operator reports completing its five-report hold, three command-to-stop
  transitions, and explicit disarm. Retained RTT directly preserves four
  consecutive `[127, 147, 187, 167]` reports from sets `46999/47000` through
  `49999/50000`, equal lane counters, one frame in flight, zero backend faults,
  and continuing IMU progress. Explicit disarm returned to sustained zeros at
  `50999/51000` and `51999/52000`. The fifth active report and other repeated
  transitions were observed but are not present in the retained excerpt.
- Active-command unequal-vector RC loss passed functionally on target on
  2026-07-19. The operator reports prompt stop, sustained inhibit through link
  absence and arm-high recovery, no automatic restart, successful fresh
  low-to-high rearm, and final disarm.
- `tools/terminal_embed.py` now accepts release, locked, and feature-selected
  F405 builds, strips host ANSI sequences, and flushes decoded RTT to
  `logs/terminal_embed` line by line.
- Retained capture `logs/terminal_embed/20260719_164203_rtt.log` closes the
  Part B RTT evidence gap. It preserves the exact unequal vector before loss,
  timeout invalidation followed by sustained zeros through recovery and
  additional link flaps, no automatic rearm, a fresh guarded rearm, restored
  unequal output, and final disarm to zeros. All 23 DShot reports had equal
  lane counters, one set in flight, and zero backend fault counters; IMU
  sequence advanced from 1 through 56,056. The release ELF hash exactly
  matched the recorded candidate. Waveform timing, physical stop latency, RPM
  ordering, full mixer output, and default DShot remain unvalidated.
- The fresh post-arming-change Part B release ELF built on 2026-07-19 has
  SHA-256
  `07A44529265B9895818F57C0B5CD608E35A9E6C95CF381373BE1372B60C24F1D`.
  It retains DShot IRQ symbols `DMA2_STREAM1/4/6/7` at
  `0x080048F4/0x08004CFC/0x08004D54/0x08004DAC`, ends its loadable flash data
  at `0x08012CC8`, and contains metadata for unequal-vector mode and the new
  stop-only arming sequence.

# Continuation Notes - DShot-Specific Arming And Idle Policy

- Replaced only the FCU3 DShot arming preparation with a 100 ms stop-only
  dwell. The existing 200 ms arm-switch qualification and all permission, RC,
  throttle, freshness, lease, and fault checks remain active.
- The actuator owner rechecks the guard every 10 ms and reports preparation
  complete while all four DShot values remain zero. The safety master must
  still pass its final check and set armed before any nonzero command is
  accepted.
- FCU3 PWM and Foxeer PWM retain the historical 2.5-second low plus 500 ms idle
  behavior. Motor pins, timer/DMA routes, priorities, motor mapping, and the
  actuator-authority boundary are unchanged.
- Added FCU3 BSP settings `prearm_stop_hold_ms = 100` and
  `idle_throttle_command = 65`. Command `65` maps to DShot value `112`, so the
  initial idle behavior preserves the prior target-proven value while allowing
  independent reviewed tuning. Compile-time policy caps it at 250; the
  unequal-vector image separately requires idle `<= 80` so tuning cannot
  silently alter its expected packet vector.
- Added reusable validation for a protocol-specific idle floor, including
  rejection tests for non-finite, zero, negative, and out-of-range settings.
- Static verification passes 205 host tests plus doc tests, strict workspace
  host/embedded Clippy, strict default/equal/unequal FCU3 variants, all four
  logical-motor checks, release builds, formatting, mdBook, added-unsafe, and
  diff checks.
- The final equal-motor props-off candidate has SHA-256
  `34CB9BFB9225AC9AE3B9771F4647F52FABE9176D392ABC54DE50BCC0AE764807`.
  Its four DMA2 IRQ symbols remain at `0x080048F4`, `0x08004CFC`,
  `0x08004D54`, and `0x08004DAC`; loadable flash ends at `0x08012BA0`.
  Defmt metadata contains the new stop-only sequence and neither legacy PWM
  idle message.
- Initial powered props-off target evidence passed on 2026-07-19. RTT retained
  four stop values through the 100 ms dwell and pre-arm completion, then
  `SYSTEM ARMED` before four value-`112` commands. The operator reported all
  four motors running. Explicit disarm restored sustained zeros through at
  least 25,000 starts; all lanes remained equal with
  `completed = started - 1`, IMU sequence advanced from 40,040 through 59,260,
  and all backend fault counters remained zero.
- The operator subsequently confirmed that the motors ran only while armed and
  that all four idled at value `112`. The DShot-specific arming/idle checkpoint
  is therefore complete, command `65` / value `112` remains the accepted FCU3
  bench idle, and unequal-vector Part B is unblocked. The startup profile line
  and flashed-ELF hash were not retained. Cold-start margin remains necessary
  before any future flight/default promotion.

# Continuation Notes - FCU3 DShot Mixed-Control Candidate

- Added the explicit `dshot_mixed_control` feature, which implies the
  four-motor DShot transport and activates the existing normal PID/mixer path.
- Bare `dshot` remains invalid. The mixed candidate rejects every equal,
  physical/logical selected, unequal-vector, stale-command injection,
  SPI-timeout injection, and PWM-calibration feature. Existing capped bench
  images continue to compile under `dshot bench_equal_motors`.
- The candidate introduces no second motor authority. Mixed physical outputs
  still cross the bounded fresh `MotorCmd` queue, armed-only actuator
  validation, normal 20 ms lease, and sole 500 Hz DShot service.
- Generalized the FCU3 BSP name from `DshotFourMotorBenchProfile` to
  `DshotFourMotorProfile`; route, timing, stop-dwell, and idle values are
  unchanged. Four-channel PWM remains the default profile.
- Added one reusable four-lane mapping helper and host tests pinning mixed
  vector lane order plus stop/minimum/maximum/saturation behavior.
- CI now checks the clean mixed candidate and all intended compile-time
  conflicts alongside the existing equal, logical, and unequal bench images.
- The corrected clean mixed release candidate has SHA-256
  `757F918B29771F0D62E7EBCC14B6DCE4A840E0062626B0A21F8F3EA7C6453269`,
  retains DShot IRQ symbols at
  `0x08004B44/0x08004F4C/0x08004FA4/0x08004FFC`, has matching vector-table
  entries, and ends its loadable flash data at `0x08013968`.
- No external pin, timer, DMA stream, IRQ priority, motor map, RC policy,
  arming state, or unsafe boundary changed.
- Retained unpowered capture
  `logs/terminal_embed/20260719_172718_rtt.log` exactly matched the superseded
  candidate hash
  `3EDC1D7767B119D9124874D96316EAEF16AF8EC6A950BBC926E4747CB7E66DAF`.
  It reached 67,000 frame starts with four zeros, synchronized completion
  counters, one set in flight, no backend counters, monotonic IMU progress
  through sequence 160,160, and no warning, panic, or safety event.
- That capture exposed roughly 333 Hz service because RTIC relative
  `delay(2 ms)` adds one tick on the 1 kHz monotonic. The loop now uses an
  absolute two-tick `delay_until` deadline and reports monotonic milliseconds.
  Static verification passes strict candidate Clippy, 206 host tests, default
  PWM and equal-motor DShot release checks, formatting, and diff checks.
- Corrected 500 Hz runtime plus reset/boot-high checks remain pending. Then
  proceed to powered props-off mixed stick/tilt direction and disarm, followed
  by active-command RC loss with arm-high recovery inhibition and fresh rearm.
