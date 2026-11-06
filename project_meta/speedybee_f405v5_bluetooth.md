# SpeedyBee F405 V5 Bluetooth Bridge

The onboard Bluetooth module of the SpeedyBee F405 V5 flight controller
board, as a maintainer's interface reference: what it is, what rides it, how a
host uses it, and what it exposes. Every fact below was measured on a
production board over its USB and BLE interfaces; nothing is inferred from the
vendor's marketing. This chapter owns this topic. FerroWasp does not run on
this board yet; the host tool that speaks this link is
[`tools/ble_console.py`](../tools/README.md).

## The module

A BLE-to-UART bridge chip soldered on the board, running Feasycom-derived
firmware, powered from the flight controller's 3.3 V rail and wired to
**UART1 (PA9/PA10, the TX1/RX1 pads) at 115200 8N1**. It is independent of the
flight MCU: an MCU reset or bootloader entry leaves the Bluetooth link and the
UART bridge alive.

It advertises as **"F405 V5"** with **no pairing, no PIN and no encryption**:
any BLE central in range may connect at any time. The bridge never gates,
filters or interprets traffic in either direction — whatever the host writes
appears on the UART, and whatever the UART emits is relayed to the connected
host, byte-exact. The vendor's phone app is just one central speaking the
flight controller's own protocol through this pipe.

## GATT interface

One custom service carries the bridge:

| Characteristic | UUID | Direction | Meaning |
|---|---|---|---|
| UART data | `ABF1` | write (host → UART) | each write's bytes are forwarded to UART1 unchanged |
| UART data | `ABF2` | notify (UART → host) | UART1 bytes, relayed unchanged |
| Command | `ABF3`, `ABF4` | write/notify | a vendor command channel, inert to the standard Feasycom AT command set; purpose unknown |

The base UUID is `0000ABF0-0000-1000-8000-00805F9B34FB` with the
characteristics under it (same pattern, `ABF1`…`ABF4`). BlueZ exposes these as
`org.bluez.GattCharacteristic1`; the host writes with `WriteValue` on ABF1 and
subscribes to ABF2 notifications (`AcquireNotify` or the `PropertiesChanged`
signal on `Value`).

A notification carries a *chunk* of the UART stream, not a whole protocol
frame: a response longer than one notification arrives split across several of
them, in stream order, and the host must reassemble by position. There is no
length, sequence or CRC added by the bridge.

## What rides it: MSPv2

The board's stock firmware (Betaflight 4.5.x) serves MSP on UART1, so the
bridge carries **MSPv2 framing** with MSPv1 function numbers. The bridge
itself is protocol-agnostic: a FerroWasp build on this board could serve any
byte protocol through the same pipe.

An MSPv2 frame is:

```text
request:  $ X <  flags:u8  function:u16le  length:u16le  payload  crc:u8
response: $ X >  flags:u8  function:u16le  length:u16le  payload  crc:u8
```

- Header: `24 58 3C` (`$X<`) for a request, `24 58 3E` (`$X>`) for a response.
- `crc` is **CRC-8/DVB-S2** (poly `0xD5`, init `0x00`), over every byte from
  `flags` through the end of the payload. It is *not* the XOR checksum of
  MSPv1 — a frame with an XOR checksum is silently dropped.
- Function numbers are the MSPv1 command codes: `1` API_VERSION, `3`
  FC_VERSION, `101` STATUS, `119` BOXIDS, `160` UID, and so on.
- Captured test vector, the module's own poll (see below):
  `24 58 3C 00 77 00 01 00 00 FD` — function 0x77 (`MSP_BOXIDS`), length 1,
  payload `00`, CRC `FD`.
- A command the firmware does not implement gets silence on the MSPv2
  path and an empty `$M>` frame on the MSPv1 path; observed with
  `MSP_IDENT` (100), which modern Betaflight dropped. Hosts should read
  both as "unsupported", not as a link failure.

Hosts should parse as a bounded byte stream: resynchronise on the `$X>`
preamble, honour `length`, verify the CRC, and discard a corrupt frame rather
than partially accepting it.

## Module behaviour

While **no** central is connected, the module's own firmware polls the flight
controller with `MSP_BOXIDS` (function 0x77, one zero payload byte) about
twice per second — its "a flight controller is attached" heartbeat. The
answer is the build's permanent box-ID list, a fixed string of small
integers, which makes it a cheap liveness probe. While a central *is*
connected it stops polling and the pipe is quiet unless one side talks.
The flight controller may ignore the poll; the module keeps polling either
way, so a firmware that does not answer MSP on UART1 is unaffected.

## Using it from a host

On Linux, BlueZ handles the radio; the host needs only D-Bus (the
`dbus`/`gi` Python modules, or `bluetoothctl` for a quick look):

1. Find the advertising bridge by name ("F405 V5") or address.
2. Connect the `org.bluez.Device1` object; pairing is neither needed nor
   offered.
3. Write request bytes to ABF1; read response chunks from ABF2 notifications
   and reassemble.
4. Expect request/response round trips of tens of milliseconds, and a
   throughput of a few KB/s — fine for configuration and status, not for bulk
   transfers. Budget per-command timeouts generously (≥1 s).

`tools/ble_console.py` wraps all of this: it scans, connects, frames MSPv2
queries and decodes the answers, with a raw-bytes mode for anything else.

## Security posture

The link is **unauthenticated**: no pairing, no encryption, no access
control on the bridge, and the module accepts any central. Whatever the flight
controller serves on UART1 is therefore reachable by anyone in radio range,
with the same authority as a USB cable. A build for this board should treat
the Bluetooth path accordingly: expose the minimum command surface, gate
writes behind the same disarmed/readback discipline as USB, and keep
arming-adjacent or bootloader-entry commands off the wireless path unless a
later decision changes this.

## Board-support implications

- **UART1 is booked.** The module shares the TX1/RX1 pads; anything wired to
  them collides with Bluetooth traffic, and the serial bindings must reserve
  the port for the bridge.
- **Betaflight's `serialpassthrough`** (4.5.x syntax: `serialpassthrough 0
  115200`) turns the USB CDC port into a live tap of the module's UART — the
  built-in way to watch this link while developing, with no extra hardware.
- The STM32F405 ROM bootloader also listens on USART1, so a bootloader entry
  is reachable through the bridge in principle; see the FAQ entry on why
  flashing over Bluetooth is not pursued.
