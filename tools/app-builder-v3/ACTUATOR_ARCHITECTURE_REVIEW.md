# Foxeer physical-actuator architecture review

Status: software architecture reviewed for generation; physical output remains
disabled. This review does not authorize flashing, actuator power, bench work,
or flight.

## Scope and authoritative references

The generated candidate must compose the current shared FerroWasp mechanisms
and the physical facts in the handwritten golden Foxeer app. It must not copy
register sequencing into the builder or invent alternate routes.

Reviewed sources:

- `apps/foxeer-f405-v2/src/board/routes.rs`, `aliases.rs`, `init.rs`, and
  `serial.rs` for physical ownership;
- `apps/foxeer-f405-v2/src/main.rs` for RTIC priorities and handoff order;
- `crates/ferrowasp-stm32f4/src/dshot.rs` for DShot timing, DMA containment,
  buffer ownership, lease expiry, frame timeout, and fault shutdown;
- `crates/ferrowasp-stm32f4/src/uart_dma.rs` for USART1 RX DMA/IDLE ownership;
- `crates/ferrowasp-tasks/src/esc_manager.rs` for request association and
  fresh per-output eRPM qualification;
- `crates/ferrowasp-core/src/safety.rs` for arming guards, motor-command
  freshness, and actuator commands.

## Controlled physical facts

| Output | Timer channel | Pin | DMA route | Polarity |
| --- | --- | --- | --- | --- |
| Physical 1 / logical M1 | TIM1_CH1 | PA8 AF1 | DMA2 Stream1 Channel6 | main output |
| Physical 2 / logical M2 | TIM8_CH4 | PC9 AF3 | DMA2 Stream7 Channel7 | main output |
| Physical 3 / logical M3 | TIM8_CH3 | PC8 AF3 | DMA2 Stream2 Channel0 | main output |
| Physical 4 / logical M4 | TIM1_CH3N | PB15 AF1 | DMA2 Stream6 Channel6 | complementary output |

The motor order remains identity `[1, 2, 3, 4]`. TIM1 is the frame master and
starts TIM8 through the existing shared backend. USART1 RX uses PA10 AF7 and
DMA2 Stream5 Channel4 for legacy BLHeli telemetry. The UART peripheral and DMA
interrupts remain priority 5; the telemetry manager remains priority 4.

## Safety and scheduling decision

- The priority-16 safety master remains the sole owner of arming state and the
  temporary actuator permit.
- Control at priority 14 may only publish bounded, timestamped motor requests.
- The safety-owned actuator adapter remains priority 15 and is the only task
  that may translate an accepted request into a DShot bank command.
- All four DMA completion handlers remain priority 16. The bounded DShot
  service remains priority 13 with the shared backend's 2 ms cadence.
- Safety-to-actuator guard observations and actuator-to-safety completion or
  fault reports use exclusive bounded channels. Missing, stale, full, or
  rejected communication fails closed.
- Pre-arm behavior retains the 100 ms stop hold, DShot idle command 65, and
  fresh per-output eRPM qualification: 3000..10000 eRPM, three consecutive
  samples per physical output, 200 ms sample age, and 1.2 s timeout.
- DShot DMA errors, spurious completion, incomplete four-lane completion,
  frame timeout, command-lease expiry, invalid command, or missing fresh
  authority force stop and report a safety fault.

## Default inhibition and evidence boundary

The selected composition has two independent software gates: the safety state
is constructed output-inhibited and the physical-actuator component is
configured with output disabled. With the component gate disabled, the DShot
service does not start stop or throttle frames and the ESC manager does not
request telemetry; initialized pads remain at the shared backend's fail-closed
low state. Tests must reject a generated default that enables either gate.

Compilation of the optional physical path establishes only declaration,
ownership, rendering, type-checking, and linking evidence. Before any future
enablement, the current Foxeer procedure still requires an exact-image review,
electrical/waveform evidence (including M4 complementary polarity and TIM1/TIM8
phase), explicit propeller and power state, operator-executed props-off gates,
and fresh target evidence. The open ESC-only power-cycle recovery defect
remains fail-closed and is not silently addressed by this builder slice.
