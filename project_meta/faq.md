# FAQ

Why the obvious alternatives lost. An entry exists only where the
alternative is tempting enough that someone, or an agent, would propose it
again; the decision itself lives in the requirement or the spec section the
entry names, never here.

Every entry has the same shape, and `reqtool validate` checks it: a
"Why not …?" heading under a topic, one plain paragraph of at most 60 words
saying what was chosen and why the alternative lost, and a last line
`See:` naming the requirement or the document section that holds the
decision. No lists, tables, code, dates, PR or ADR numbers, names of who
decided, or history.

## Links between tasks

### Why not one shared request channel to the coordinator?

One queue per requester means a requester that floods fills only its own
queue, and the queue a request arrives on says who sent it. A shared channel
would let one requester crowd out the rest and would make the sender name
itself in the message.

See: [the coordinator](requirements/architecture.md#11-the-coordinator)

## Tracing requirements to code

### Why not trace evidence by crate closure?

A crate boundary is sound but coarse: keeping each piece of critical code's
evidence apart would need a crate per piece, multiplied across every
peripheral and chip family. Tracing per source file, with each file's direct
imports frozen by identity, keeps evidence as narrow as a crate split would,
without the crates.

See: [tracing per source file](requirements/tracing.md#20-tracing-per-source-file)

### Why not follow the call graph?

A call graph from rust-analyzer or rustc names exactly what each function
reaches, but it is heavy, ties the tool to compiler internals and breaks when
they change. Imports resolved to files are read from source alone and widen
where a path is unclear, so they never miss a change.

See: [tracing per source file](requirements/tracing.md#20-tracing-per-source-file)

### Why not list each module's requirements in a manifest?

A manifest beside the code drifts from it: a moved or renamed file leaves its
entry dangling, or pointing at the wrong code. A declaration inside the file
travels with it, and a safety-critical file that declares nothing is found by
the check.

See: TRACE-001

### Why not attach a requirement's tests to implementations with a macro?

A macro that stamps tests onto each implementation hides what runs, and ties
the tests to the macro's expansion. A generic test function over a trait, or
one taking a closure adapter, is ordinary code the implementation's tests
call, and that call is the link.

See: [tracing per source file](requirements/tracing.md#20-tracing-per-source-file)

### Why not let evidence depend on every transitive import?

A change deep in the tree would re-owe every module above it, which is the
crate closure again. A module's evidence covers its own code and the identity
of what it imports directly; each imported module's own evidence keeps
proving the requirements the importer relies on.

See: TRACE-004

## Flashing

### Why not a FerroWasp bootloader?

On request the firmware restarts into the chip's ROM USB DFU bootloader, as
Betaflight does on F4 and H7 targets. A bootloader of FerroWasp's own would
move every flight image's link address and add its own USB stack and update
failure modes, for nothing the ROM bootloader lacks.

See: REBOOT-002

## DShot

### Why not bitbang DShot, as Betaflight does on the F4?

Bitbang writes GPIO through DMA2, which STM32F405 erratum 2.2.19 can corrupt
while APB2 peripherals such as SPI1 use DMA2 at the same time; Betaflight
takes SPI off DMA for that reason. Timer compare DMA for frames, and DMA reads
of GPIO for replies, keep the IMU's SPI on DMA.

See: OUTPUT-003

### Why not input capture for bidirectional replies?

On the Foxeer, motor 4 is TIM1's complementary output, and TIM1 channel 3
captures only from the ordinary channel 3 pin, which is the ESC telemetry
wire. Motor 1 shares TIM1, which cannot run freely for capture and at the
sampling period at once. Sampling each pin's GPIO input works on every lane.

See: OUTPUT-003

## Arming

### Why not accept one valid reply per motor since boot, as Betaflight does?

A reply from before the arm attempt says that a motor answered once, not that
it is spinning now. Arming waits for three consecutive fresh in-range samples
from every output, taken during guarded idle, so a stopped, disconnected or
failed motor ends the attempt.

See: ARM-001

### Why not disarm when eRPM replies are lost in flight?

Stale eRPM moves only the RPM filter's notches, never motor output: arming
needs fresh eRPM, and dynamic idle holds and then decays its increase while
eRPM is stale. Disarming on reply loss would turn a filtering fault into a
drop from the sky.

See: [motor speed feedback](requirements/hardware-and-loop.md#21-motor-speed-feedback)

## Calibration

### Why not store the gyro bias as well?

Arming already requires a stationary gyro-bias calibration at every boot, so
a stored bias would always be replaced before it was used. The
accelerometer's offset from level is a fact of how the board is mounted in
one craft, so that is what is stored.

See: CTRL-009

## Board support

### Why not test board facts through a `#[path]` harness?

A data crate with no chip HAL cannot import one, so the compiler, not
convention, keeps hardware glue out of a board's facts, and their tests run in
every workspace test run. When the facts' tests first ran, three had gone
stale unnoticed.

See: HW-ROUTE-001

## Crates and tasks

### Why not give each subsystem crate its chip family's backend?

A subsystem crate that took a chip feature would carry a HAL, and shared logic
stays free of HAL types so that it runs in host tests for every board. Tasks
name hardware through neutral traits instead, and each firmware binds its own
chip's types.

See: [crate layout](requirements/configuration-and-crates.md#16-crate-layout)

### Why not copy the application glue into each firmware?

One copy of the facade, the task groups and the channel constructors serves
every board, so a fix lands once. Copies drift: when each board had its own
flash manager, every storage fix had to be made twice.

See: [crate layout](requirements/configuration-and-crates.md#16-crate-layout)

## Watchdog

### Why not feed the independent watchdog from the control loop?

A hung control loop is the safety master's to detect and report. Feeding the
watchdog from the loop would make a dead IMU reset the board over and over on
the bench instead of reporting why, so only the safety master's own periodic
check feeds it.

See: [the independent watchdog](requirements/fault-handling.md#the-independent-watchdog)

## Configuration

### Why not clamp a stored value that fails validation?

Storage is untrusted input, and a clamped value is one nobody chose. A
damaged slot, or one that fails validation, falls back to the older slot or
the board default, and arming waits until the pilot acknowledges the
fallback.

See: CONFIG-004
