# Board Support

A flight board is a firmware application in `firmware/<board>/` with a board
data crate, HAL glue, and an RTIC shell; [Project Structure](project_structure.md)
shows the layout. This chapter covers what a board owns and the conventions
every board follows.

## What a Board Owns

The board data crate, `firmware/<board>/board`, holds the board's physical
facts as plain, host-tested data: pins, DMA and serial routes, resource claims,
the IMU orientation and motor output order, the control-loop rate, USB
identity, and the board's verification flags. It cannot import a chip HAL, so
anything needing one belongs in `src/board/` or a backend crate.

Each board owns its own configuration. Its default tuning and default stored
configuration are `DEFAULT_TUNING` and `DEFAULT_STORED_CONFIG` in
`board/src/profiles.rs`. A board never borrows another board's defaults, and a
shared crate never holds defaults named after a board. A valid configuration
stored on the board takes precedence over its defaults.

## Serial Port Names

Serial ports are named after the chip's own numbering and always spelled UART,
whether the peripheral is a USART or a UART: the STM32's USART3 is `UART3`,
and `uart3` where the pilot types it, as in `config bind uart3 osd`. The same
names are used in port types and configuration fields. HAL types, interrupt vectors,
and the peripheral strings in route tables keep the HAL's own names.

## Behavioural Reference

Foxeer F405 V2 is the golden flight application: the reference for arming,
failsafe, task priorities, actuator handling, and logging. Another board matches
its behaviour and differs only where its hardware does. Its hardware facts are
never copied to another board.

## Adding a Board

1. Start from the sibling boards and the shared crates, not from a copy of one
   application. Code that would still work after substituting pins, routes, or
   configuration belongs in a shared crate.
2. Put every hardware fact in the new board data crate, with tests, and its
   own defaults.
3. Keep `src/main.rs` to the `ferroforge::app!` declaration, selecting the
   shared task definitions.
4. Run `python tools/check_rtic_boundaries.py`, and compare the new board's
   enabled features with Foxeer's.
