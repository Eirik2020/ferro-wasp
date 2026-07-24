# NUCLEO-F401RE hardware smoke test

Perform this gate with only the development board connected. Do not connect
actuators or hazardous hardware.

1. Run the generator with `bsp/nucleo-f401re.toml` and
   `applications/nucleo-f401re-blinky.toml`, then confirm the final release
   build passes.
2. Flash the ELF produced for `thumbv7em-none-eabihf` using the board's ST-LINK
   interface and a known compatible flashing tool.
3. Confirm LD2 on PA5 changes state at the interval declared by
   `toggle_period_ms`. A complete on/off cycle is twice that interval.
4. Press and release B1 once. Confirm blinking stops and LD2 remains off.
   Press and release B1 again and confirm blinking restarts. Each physical
   press must cause exactly one state change despite contact bounce.
5. Record the board revision, probe identity, flashing tool/version, manifest
   fingerprint, binary fingerprint, and observation.
6. Change only `features.blink_led.toggle_period_ms` in the application
   manifest to another exact divisor of 1000, regenerate, rebuild, and flash
   again.
7. Confirm the observed transition interval matches the new manifest value
   without editing generated Rust.

Compilation and this smoke test validate the MVP mechanism; neither is a
safety or certification claim.
