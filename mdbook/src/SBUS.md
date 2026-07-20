# SBUS

SBUS is the active FerroWasp RC input path. The FCU3 and Foxeer profiles route
it to USART2 RX on PA3; PA2 is the corresponding TX pin but is not required for
receive-only SBUS.

## UART Configuration

- Baud rate: 100 000 bps
- Word length: 9 bits
- Parity: Even
- Stop bits: 2
- Logic: Inverted

## Frame Structure

| Byte | Description     | Content                                                   |
|------|-----------------|-----------------------------------------------------------|
| 0    | Header          | `0x0F`                                                    |
| 1-22 | RC channel data | 16 channels, 11 bits each *                               |
| 23   | Flags           | Channel 17 and 18, Frame lost and Failsafe **             |
| 24   | Footer          | `0x00`                                                    |

\* See bit-packing below. \
\*\* Digital channels and link-status flags.

## Bit-Packing Map

Sixteen 11-bit channels occupy 176 bits, exactly 22 bytes. The fields are
packed contiguously, least-significant bit first, across bytes 1 through 22.

### Channel Bytes

- Byte 1: Channel 1 bits 0-7
- Byte 2: Channel 1 bits 8-10, then Channel 2 bits 0-4
- Byte 3: Channel 2 bits 5-10, then Channel 3 bits 0-1
- Byte 4: Channel 3 bits 2-9
- The same contiguous packing continues through Channel 16 in byte 22.

### Flags (Byte 23)

- Bit 0: Channel 17 (digital)
- Bit 1: Channel 18 (digital)
- Bit 2: Frame lost
- Bit 3: Failsafe active
- Bits 4-7: Reserved

FerroWasp treats either frame-lost or failsafe as an immediate RC-link
invalidation. It also expires the link after 100 ms without a healthy frame,
and recovery requires three healthy frames plus a newly observed low arm state
before a later high transition may request arming.

