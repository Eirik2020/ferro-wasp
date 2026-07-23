# STM32F4 backend 0.8.0

The MVP backend supports one deliberately narrow MCU compatibility profile:

- `STM32F401`, using `thumbv7em-none-eabihf`
- `stm32f4xx-hal` 0.23.0 with the `stm32f401` feature
- flash at `0x08000000` with 512 KiB and RAM at `0x20000000` with 96 KiB
- the 16 MHz internal HSI source and an 84 MHz system clock
- the compact manifest pin token `PA5`, translated to GPIOA pin 5
- PA5 as a low-speed push-pull output with no pull resistor
- SysTick as a backend-owned 1 kHz monotonic scheduling endpoint
- PC13 as a pull-up input, with falling-edge EXTI15_10 interrupt handling
- USART1 on PA9/PA10 at AF7 with separate DMA2 stream 5 RX and stream 7 TX

The profile name follows Betaflight's family-level convention. It is a
backend-defined contract for supported STM32F401 boards, not a claim that
every STM32F401 package and density has the profile's memory layout. The BSP
declares only `mcu = "STM32F401"`; the compilation target, HAL crate and
feature, PAC path, and memory map are derived from the versioned backend
profile. Exact ordering codes can be introduced later as additional profiles
if they provide a concrete validation or hardware-support benefit.

The BSP manifest declares the physical pin and default physical output level
under a stable resource ID. The application selects task priority and period.
The backend derives ordinary software delays from its shared timebase. The blink LED
backend owns its fixed electrical policy: push-pull output, no pull resistor,
and low speed. These rules are documented backend behavior, not silently
selected manifest defaults. The backend rejects unsupported resources and
never chooses a replacement pin, priority, or period.

Board-level clock setup and GPIO-bank decomposition are emitted by the base
application renderer. The blink feature consumes those board resources; its
fragment no longer freezes RCC or splits GPIOA itself. The button feature
similarly consumes GPIOC and EXTI while sharing the LED and blinker state
provided by the earlier blink feature.

## Button and debounce policy

The NUCLEO user button is represented by the compact BSP pin token `PC13`.
The backend fixes its electrical and interrupt policy to pull-up input and
falling-edge EXTI15_10. On the first edge, the handler acknowledges and masks
the EXTI line, then schedules the application-declared debounce interval
(20 ms in the example) through the monotonic. The deferred task accepts the
press only if PC13 is still low, clears any accumulated edge, and rearms EXTI.

An accepted press toggles shared blinker state. Disabling also forces LD2 low,
so the state is unambiguous. Button and debounce tasks run at priority 2; the
blink task runs at priority 1.

## Fixed clock translation

For the supported clock tuple, the backend emits:

```rust
cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()))
```

With the pinned HAL this derives an AHB/system clock of 84 MHz, an APB1 clock
of 42 MHz, and an APB1 timer clock of 84 MHz. Those derived prescalers are a
documented and versioned backend rule rather than an allocator decision. A
different source, source frequency, system frequency, HAL version, or timer
clock declaration is unsupported and fails validation.

## Shared timebase

The backend starts `rtic-monotonics` from Cortex-M SysTick at 1 kHz. Blink,
button debounce, and OSD refresh are restricted logical scheduling policies:
they declare periods and priorities but cannot change the timebase clock,
counter mode, interrupt, or ownership. TIM2/TIM3/TIM4 remain free for future
control loops and pin-backed PWM/capture endpoints. Pin-backed timer channels
will remain explicit BSP resources because their pin, channel, and DMA mapping
are physical board facts.

## Experimental serial/OSD profile

The BSP declares USART1 PA9/PA10 and DMA controller/streams/channels. The
application declares buffer sizes, bounded queue capacities, and task
priorities. The backend derives AF7 from the MCU endpoint mapping. UART baud
and framing ultimately belong to the boot-selected platform profile rather
than either manifest.

The `usart1_rx` and `usart1_tx` route roles also determine DMA direction. The
BSP does not repeat peripheral-to-memory or memory-to-peripheral strings.

Until the boot router exists, the isolated OSD prototype statically applies
the `msp_displayport` backend profile: 115200 baud and 8-N-1. This is a
transitional constraint, not the long-term configuration boundary.

USART1, both DMA streams, transfer buffers, and their interrupts remain in the
hardware layer. The MSP component receives only a bounded `SerialRxTx`
endpoint. This experimental path is not wired into the flight-tested
FerroWasp applications. See `osd-usart1-dma.md`.

The shared monotonic schedules a refresh every 100 ms. Each refresh queues a DisplayPort
heartbeat and one frame from FerroWasp's bounded 15-step overlay sequence.
The sequence clears the display, writes the title and status rows, and issues
`DRAW_SCREEN`, completing in approximately 1.5 seconds before repeating.

The OSD application also composes the separate `button_arm_toggle` feature.
B1 on PC13 uses EXTI15_10 and the shared 20 ms debounce policy. An
accepted press toggles only the experimental OSD telemetry state's `armed`
flag; it does not execute FerroWasp flight arming logic or enable actuators.
