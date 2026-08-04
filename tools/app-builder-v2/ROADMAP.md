# Golden Foxeer App Generation Roadmap

## Goal

Generate the RTIC shell for the current `apps/foxeer-f405-v2` golden flight
application from audited board facts, reusable task definitions, component
instances, and application composition.

Full support means the generated application preserves the golden app's task
graph, priorities, resource ownership, interrupt bindings, initialization
order, feature behavior, and safety boundaries. A successful build alone is
not behavioral or hardware evidence.

During development, the generated Foxeer target remains a non-authoritative
prototype. The handwritten golden app remains authoritative until explicit
parity and safety gates are met.

## Roadmap

### 1. Establish the Foxeer target boundary

- Add a separate `foxeer_f405_v2` builder target; do not derive it from the
  FCU3 declaration merely because both boards use an STM32F405.
- Keep the existing Foxeer `src/board/` support as the authoritative source of
  pin, DMA, timer, clock, electrical, and orientation facts.
- Add a deliberate synchronization or parity check for any neutral board facts
  consumed by the builder so the two representations cannot silently drift.
- Start with an actuator-inhibited target and preserve the existing golden app
  as the release image.

### 2. Remove core generation blockers

- Support the Foxeer 8 MHz HSE, 168 MHz system clock, and USB-valid PLL48 clock.
- Resolve and render DMA1 and DMA2 resources and conflicts.
- Replace the fixed EXTI dispatcher pool with target-declared, conflict-checked
  dispatchers.
- Extend resource resolution beyond single pins to peripherals, DMA streams,
  timer channels, interrupt vectors, and multi-pin hardware groups.
- Generalize typed software and init-local resources without adding one
  builder enum variant for every Foxeer application type.

### 3. Generalize component initialization

- Let a component declare ordered initialization and return multiple typed
  outputs into RTIC local/shared resources.
- Preserve component-private storage, exposed outputs, and explicit ownership.
- Model initialization failure behavior instead of hiding it in generated
  backend strings.
- Add support for lock-free resources, bounded queues/channels, and singleton
  storage used by the golden app.

### 4. Add non-actuator Foxeer components

- Full-duplex serial: USART2 SBUS, UART4 MSP RX/TX DMA, and USART1 ESC
  telemetry RX.
- SPI1 DMA IMU with PC4/EXTI4 data-ready and supported-sensor probing.
- ADC1 battery/current observation with DMA.
- USB FS debug/configuration transport.
- SPI2 NOR storage and blackbox/configuration queues.
- TIM4 control scheduler, TIM2 timebase, and TIM6 I/O watchdog.

Prefer the existing `ferrowasp-stm32f4`, `ferrowasp-drivers`, and
`ferrowasp-tasks` mechanisms; the builder should compose them rather than
reimplement them.

### 5. Migrate the golden task graph

- Move the handwritten RTIC task bodies into unified reusable task modules
  without changing their behavior.
- Expand the task checker for monotonic `now`/`delay_until`, task spawn
  arguments, target-specific concrete types, feature-gated code, and the
  required embedded traits.
- Bind task-to-task spawns explicitly where reusable task instances cannot
  safely name concrete generated tasks.
- Recreate the exact golden priorities, interrupts, init spawns, local/shared
  ownership, and bounded failure handling.

### 6. Integrate safety-owned actuator output last

- Add the four-lane TIM1/TIM8 DShot component and its DMA completion tasks.
- Add BLHeli telemetry association and ESC-manager wiring.
- Preserve the safety-master, actuator-permit, command-freshness, RC-loss,
  IMU-health, failsafe, and disarm ordering from the golden app.
- Ensure no ordinary task or generic component gains direct motor-peripheral
  authority; only the safety-owned actuator path may command DShot hardware.

### 7. Prove parity before cutover

- Compare generated tasks, priorities, resources, IRQs, dispatchers,
  initialization order, feature gates, and Cargo configuration against the
  handwritten golden app.
- Build the generated target across the golden app's supported feature sets.
- Run reusable host tests, builder tests, RTIC boundary checks, and the Foxeer
  software/build test chain.
- Treat live bench, preflight, and flight evidence as separate user-executed
  gates.
- Only replace the handwritten RTIC shell after an explicit review confirms
  behavioral and safety equivalence.

