# RTIC app builder cheatsheet

Run these commands from the repository root.

## Generate and build

Use catalog names for the normal workflow:

```powershell
cargo xtask generate --app nucleo-f401re-blinky
cargo xtask generate --app nucleo-f401re-osd
```

Explicit paths remain available:

```powershell
cargo xtask generate `
  --bsp .\bsp\nucleo-f401re.toml `
  --manifest .\applications\nucleo-f401re-blinky.toml
```

## Generate, build, and flash

Build a named application without accessing hardware:

```powershell
cargo xtask build --app nucleo-f401re-blinky
cargo xtask build --app nucleo-f401re-osd
```

Build, program, verify, and reset:

With the NUCLEO connected through ST-LINK:

```powershell
cargo xtask flash --app nucleo-f401re-blinky
cargo xtask flash --app nucleo-f401re-osd
```

The command performs a fresh generation and release build, invokes
`probe-rs download`, verifies the programmed image, and resets the MCU.

## Build and embed

Build and start an attached `cargo embed` session using the generated ELF:

```powershell
cargo xtask embed --app nucleo-f401re-blinky
cargo xtask embed --app nucleo-f401re-osd
```

Use `Ctrl+C` to end an attached session. This command requires `cargo-embed`
from the probe-rs tools on `PATH`.

## Flash an existing build manually

```powershell
probe-rs download `
  --chip STM32F401RE `
  --protocol swd `
  --verify `
  --reset `
  .\generated\nucleo-f401re-blinky\target\thumbv7em-none-eabihf\release\rtic-generated-app
```

## Change the blinker

Edit `features.blink_led.toggle_period_ms` in
`applications/nucleo-f401re-blinky.toml`. The value is the interval between
LED state changes; a complete on/off cycle takes twice that interval. The MVP
currently accepts positive values that divide 1000 ms exactly.

The example also maps B1 on PC13 to the `button_toggle` feature. A debounced
press stops blinking and forces LD2 off; the next debounced press restarts it.
The debounce interval is `features.button_toggle.debounce_ms` (20 ms by
default).

In `nucleo-f401re-osd`, B1 instead toggles the demonstration OSD state between
`ARMED` and `DISARMED` after the same 20 ms debounce. This is display-only and
does not enable actuators.

## Resume or clean

Resume is valid only when all fingerprinted inputs are unchanged:

```powershell
cargo xtask generate `
  --app nucleo-f401re-blinky `
  --resume
```

Remove one generated application:

```powershell
cargo xtask clean --app nucleo-f401re-blinky
```

## Test the builder

```powershell
cargo check -p xtask --locked
cargo test -p xtask --locked
```
