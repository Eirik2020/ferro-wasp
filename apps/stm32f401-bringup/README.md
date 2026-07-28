# STM32F401 Bring-Up App

This is the deliberately minimal STM32F401 bring-up runtime contract. It
currently selects the NUCLEO-F401RE board support by default and owns only:

- PA5: onboard LD2 user LED;
- PA2 / USART2 TX: ST-LINK virtual COM port;
- SysTick: RTIC monotonic for one-second heartbeat scheduling;
- EXTI0: RTIC software-task dispatcher.

It has no sensor, RC, actuator, PWM, or motor-output authority.
The RTIC app contains only initialization and one persistent priority-1 async
heartbeat task.

## Build

Run commands from this directory so Cargo uses the Nucleo-specific target and
runner configuration:

```powershell
cd apps/stm32f401-bringup
cargo build --locked
```

## Flash With ST-LINK

Either command builds, flashes, resets, and attaches RTT:

```powershell
cargo run --locked
```

```powershell
cargo embed
```

Open the ST-LINK virtual COM port at 115200 baud, 8 data bits, no parity, and
one stop bit. Expected output:

```text
FerroWasp NUCLEO-F401RE heartbeat 0
FerroWasp NUCLEO-F401RE heartbeat 1
```

LD2 should toggle once per heartbeat. Continuous LED and USART progress are the
initial liveness evidence. The post-RTIC-conversion target smoke test passed on
2026-07-18. The earlier polling-image result remains the longer-duration soak
evidence.
