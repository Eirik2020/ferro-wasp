# DShot

Four-motor DShot600 is the ESC protocol on every flight board. RC PWM remains
shared infrastructure for servos and auxiliary outputs; it is never a
flight-app ESC fallback. How arming uses DShot idle and ESC telemetry is in
[Arming Sequence](arming.md).

## Packet

A DShot packet is 16 bits, sent most significant bit first:

```text
[ 11-bit value ][ telemetry request bit ][ 4-bit checksum ]
```

| Value | Meaning |
|---|---|
| 0 | stop |
| 1..47 | special commands |
| 48..2047 | throttle |

The checksum is `(payload ^ (payload >> 4) ^ (payload >> 8)) & 0x0f`. At
DShot600 a bit lasts 1.667 us: a `0` is high for 3/8 of it and a `1` for 3/4.
A packet takes about 27 us. Packet encoding, throttle mapping, and the
compare-value sequence are host-tested in `ferrowasp-waveform::dshot`.

Normal output carries only stop or mapped throttle. Special commands, such as
FerroConfigurator's spin-direction change and save, are queued for one motor
and refused unless every motor is stopped and no command lease is active.

## Motor Bank

Each lane is a timer compare channel whose DMA stream writes the next bit's
compare value at every bit boundary. The four lanes form one bank and one
fault domain: the bank owns its timers, all four DMA streams, and every DMA
buffer.

Which timers, channels, and streams carry the motors is the board's. Its glue
hands the chip backend its timers and names each motor's lane as a timer, a
channel, and a stream; the DMA plan is in its data crate under
`firmware/<board>/board/src/routes.rs`. A lane's compare register and DMA
request follow from the timer's and channel's types, so they cannot disagree,
and a route the backend has not declared does not compile. All of a bank's
lane timers must count the same timer clock, because one bit timing encodes
every lane; on the STM32F405 that means not mixing the APB2 timers (TIM1,
TIM8) with the APB1 ones.

| Board | Timers | Start |
|---|---|---|
| Foxeer F405 V2 | TIM1 and TIM8 | TIM1 starts TIM8 through an internal trigger |
| TBS Lucid H7 | TIM3 and TIM5 | started back to back in software, a few bus cycles apart |

Before each frame set the bank stops its timers, prepares all four streams,
stages every lane's first bit, and then starts the timers together: from one
timer's trigger output where the board chooses that and the chip wires it, or
back to back in software. A frame set
is complete only when all four lanes report transfer completion. A transfer or
direct-mode error, an unexpected or duplicate interrupt, or a frame set still
in flight after 1 ms faults the whole bank: the timers stop, the streams are
fenced, every output is forced low, and the bank requests disarm.

On Foxeer, motor 4 is TIM1's complementary output CH3N with the ordinary CH3
output disabled; the backend selects active-high polarity so it pulses like the
other lanes.

## Cadence and Leases

The DShot service is the only task that starts a frame set. Each motor command
the actuator output accepts updates the authorized four-lane vector, renews its
lease, and releases the service, so frames follow the control loop: 2 kHz on
both flight boards. A disarm releases it too, so stop frames go out at once.

When no command releases it - disarmed, during guarded idle, or if the control
loop stops - a fallback task releases the service whenever no frame set has
started for 2 ms, on absolute deadlines so that cadence does not drift. The ESCs always see
frames, at no less than 500 Hz. A release that finds the service already
pending or running folds into that run.

A nonzero command older than 20 ms is rejected, and if the lease is not renewed
the service selects four stops and requests disarm. Disarmed, it keeps sending
stop frames.

## Safety Ownership

```text
control loop
    -> bounded MotorCmd queue
    -> actuator_output validation
    -> leased four-lane DShot request
    -> DShot service and motor bank
    -> timers, DMA, motor pins
```

No control, RC, USB, OSD, configurator, or experimental task owns a motor
timer, pin, or DShot DMA stream.

Each app is `#![deny(unsafe_code)]`. The unsafe code under DShot is limited to
the DMA endpoint declarations of each chip backend: `ferrowasp-stm32f4::dshot`
and `ferrowasp-stm32h7::dma_endpoints`. Each proves that an endpoint addresses
the compare register of a timer the bank owns exclusively, with the right
transfer width and a valid stream mapping. HAL transfers own the active
buffers; there is no project `static mut` and no aliasing of a buffer that DMA
is using.

## ESC Telemetry

ESC telemetry is the BLHeli legacy UART protocol: one combined telemetry wire
from all four ESCs, at 115,200 baud 8N1, on the UART bound to the
`esc_telemetry` function. A frame is ten bytes: temperature, voltage, current,
consumption, eRPM/100, and a CRC-8 with polynomial `0x07`. The parser advances
one byte after a CRC failure, so it regains framing without a sync byte.

A frame carries no motor identity, so identity comes from which output asked.
A low-priority ESC manager rotates through the physical outputs, at most one
request every 20 ms with never more than one outstanding, which is 12.5 Hz per
ESC when all four respond. It passes each request to the DShot service through
a bounded queue; the service sets that output's telemetry bit and acknowledges
the request only once the frame carrying it has started. A response that
arrives before its acknowledgement is held back until the matching
acknowledgement arrives, and a mismatched acknowledgement cannot relabel it.
The service drops a request it has held unsent for 20 ms, the same time after
which the manager gives up on its acknowledgement, so a stale request never
reaches an ESC later. A response unanswered after 100 ms times out.

Telemetry identity is physical-output identity, not mixer motor order; the
board maps outputs to logical motors.

A timeout stops telemetry, so late traffic cannot be assigned to a later
request. Telemetry resumes only while disarmed, after 500 ms with no byte on
the wire and no acknowledgement pending, with every stored sample dropped.
Each retry that brings nothing back doubles that window, up to 4 s, so
unpowered ESCs are asked rarely and log one warning per outage; the first frame
back resets it.

The manager waits five seconds after boot before its first request. It owns no
motor peripheral or safety state: a UART, parser, queue, or timeout fault can
only withhold the samples arming needs, which fails closed. Once armed,
telemetry is observational; losing it does not by itself disarm.

## Bench Images

Bench features build images for props-off work and never ship as the flight
image. Every motor bench feature needs the explicit props-off gate
`bench_actuator_validation` as well, or the build fails:

```text
cargo build --release --locked --features "bench_actuator_validation bench_equal_motors"
```


- `bench_equal_motors` sends the same requested throttle to all four lanes,
  capped at 250 PWM-style counts, with normal RC, arming, freshness, lease,
  and actuator validation.
- `bench_logical_motorN_only`, added to it, spins only logical motor N, to
  confirm the logical-to-physical map and each motor's direction. Selecting
  two is a compile error.
- `bench_dshot_idle_output1_not_running` reports zero eRPM for physical output
  1 at the qualification input only, to prove that a missing motor aborts
  arming. It prints `FAULT INJECTION ACTIVE` at startup and must never be used
  with propellers fitted.

The idle command used for arming changes only in small reviewed steps, each
followed by repeated cold starts and the full props-off check; never tune it
with propellers fitted.

## Not Yet Measured

Pulse width, jitter, polarity margin, lane-to-lane phase, and physical stop
latency have not been measured with a logic analyzer; flights so far do not
establish them. Bidirectional DShot telemetry is not implemented.
