//! Bidirectional DShot replies (ADR-0045): what an ESC answers on its motor
//! wire after each frame, from the sampled pin to eRPM or a telemetry reading.
//!
//! After a command frame with an inverted checksum, the ESC drives the line
//! itself about 30 us after the frame ends. It sends 21 bits at 5/4 of the
//! command bit rate, 750 kbit/s at DShot600. The line idles high, and every
//! bit that is 1 is a transition. The first bit is always 1: the falling start
//! edge. The 20 after it are four 5-bit GCR symbols, which decode to 16 bits:
//! a 12-bit value and a 4-bit checksum. The value is an eRPM period or, with
//! extended telemetry (EDT) on, a typed reading.
//!
//! Each layer works alone:
//! - [`reply_transitions`] turns pin samples, [`REPLY_SAMPLES_PER_BIT`] per
//!   bit, into the 21 transition bits;
//! - [`decode_gcr`] turns those into the checked 12-bit value;
//! - [`DshotReply::from_value`] turns the value into eRPM or a reading.
//!
//! The sampling layer follows Betaflight's `drivers/dshot_bitbang_decode.c`,
//! and the value layer its `drivers/dshot.c`, at commit `41a5b43`.

/// Bits in a reply, the start edge included.
pub const REPLY_BITS: u32 = 21;

/// Pin samples per reply bit: 2.25 MHz at DShot600.
pub const REPLY_SAMPLES_PER_BIT: usize = 3;

/// A start edge needs this many samples after it, two bits short of a whole
/// reply, to be taken as one.
const MIN_REPLY_SAMPLES: usize = (REPLY_BITS as usize - 2) * REPLY_SAMPLES_PER_BIT;

/// How far after the start edge a reply is read: two bits past its end, so
/// the edge back to idle closes its last bit.
const MAX_REPLY_SAMPLES: usize = (REPLY_BITS as usize + 2) * REPLY_SAMPLES_PER_BIT;

/// Fewer bits than this between edges and no reply was seen. The last bits
/// of a reply may carry no edge, so the remainder up to 21 is filled in.
const MIN_EDGE_BITS: u32 = 18;

/// The ESC's answer to the EDT-enable command (13) before EDT is on.
pub const EDT_ENABLE_ACK: u16 = 0x0e00;

/// The value an ESC sends for a stopped motor.
const ERPM_STOPPED: u16 = 0x0fff;

/// Why a reply could not be decoded. Every one is dropped and counted, never
/// treated as a reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotReplyError {
    /// No start edge with room for a whole reply after it, or too few edges
    /// after one.
    NoReply,
    /// More edges than a reply holds.
    Overlong,
    /// A 5-bit group that is not a GCR symbol.
    InvalidSymbol,
    /// The checksum does not match.
    Checksum,
    /// An eRPM period of zero, which no ESC sends.
    ZeroPeriod,
}

/// Turns pin samples into a reply's 21 transition bits, most significant
/// first. `pin_mask` selects the motor's pin in each sample, so a sample can
/// be a whole GPIO input register.
pub fn reply_transitions(samples: &[u16], pin_mask: u16) -> Result<u32, DshotReplyError> {
    let high = |sample: &u16| sample & pin_mask != 0;
    let last_start = samples
        .len()
        .checked_sub(MIN_REPLY_SAMPLES)
        .ok_or(DshotReplyError::NoReply)?;
    let start = samples[..=last_start]
        .iter()
        .position(|sample| !high(sample))
        .ok_or(DshotReplyError::NoReply)?;
    let end = samples.len().min(start + MAX_REPLY_SAMPLES);

    let mut level = false;
    let mut run_start = start;
    let mut bits = 0;
    let mut transitions = 0u32;
    for (index, sample) in samples.iter().enumerate().take(end).skip(start + 1) {
        if high(sample) == level {
            continue;
        }
        // The run's samples, rounded to the nearest bit and never under one.
        let run = ((index - run_start + 1) / REPLY_SAMPLES_PER_BIT).max(1) as u32;
        bits += run;
        if bits > REPLY_BITS {
            return Err(DshotReplyError::Overlong);
        }
        transitions = (transitions << run) | (1 << (run - 1));
        run_start = index;
        level = !level;
    }

    if bits < MIN_EDGE_BITS {
        return Err(DshotReplyError::NoReply);
    }
    let rest = REPLY_BITS - bits;
    if rest > 0 {
        transitions = (transitions << rest) | (1 << (rest - 1));
    }
    Ok(transitions)
}

/// GCR symbol to nibble; `INVALID` marks the 16 codes that are not symbols.
const INVALID: u8 = 0xff;
const GCR_DECODE: [u8; 32] = [
    INVALID, INVALID, INVALID, INVALID, INVALID, INVALID, INVALID, INVALID, //
    INVALID, 0x9, 0xa, 0xb, INVALID, 0xd, 0xe, 0xf, //
    INVALID, INVALID, 0x2, 0x3, INVALID, 0x5, 0x6, 0x7, //
    INVALID, 0x0, 0x8, 0x1, INVALID, 0x4, 0xc, INVALID,
];

/// Decodes a reply's transition bits to its 12-bit value, checking every
/// symbol and the checksum. The start edge, bit 20, is ignored.
pub fn decode_gcr(transitions: u32) -> Result<u16, DshotReplyError> {
    let mut decoded = 0u16;
    for symbol in 0..4 {
        let nibble = GCR_DECODE[((transitions >> (5 * symbol)) & 0x1f) as usize];
        if nibble == INVALID {
            return Err(DshotReplyError::InvalidSymbol);
        }
        decoded |= u16::from(nibble) << (4 * symbol);
    }
    // The nibbles of value and checksum XOR to all ones.
    let check = decoded ^ (decoded >> 8);
    if (check ^ (check >> 4)) & 0x0f != 0x0f {
        return Err(DshotReplyError::Checksum);
    }
    Ok(decoded >> 4)
}

/// One decoded reply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotReply {
    /// Electrical RPM divided by 100, as in the legacy UART frame; zero for a
    /// stopped motor.
    Erpm { erpm_div100: u32 },
    /// ESC temperature in degrees Celsius.
    Temperature { celsius: u8 },
    /// ESC supply voltage, 0.25 V per count.
    Voltage { quarter_volts: u8 },
    /// ESC current, 1 A per count.
    Current { amps: u8 },
    /// An ESC debug value, channel 1 to 3.
    Debug { channel: u8, value: u8 },
    /// An EDT state or event report.
    StateEvent { value: u8 },
    /// The ESC acknowledged the EDT-enable command.
    ExtendedTelemetryEnabled,
}

impl DshotReply {
    /// Interprets a checked 12-bit value. With `extended_telemetry` off every
    /// value is an eRPM period, except the EDT-enable acknowledgement. With it
    /// on, a value whose type nibble is even and nonzero is a typed reading.
    pub const fn from_value(value: u16, extended_telemetry: bool) -> Result<Self, DshotReplyError> {
        let value = value & 0x0fff;
        if !extended_telemetry && value == EDT_ENABLE_ACK {
            return Ok(Self::ExtendedTelemetryEnabled);
        }

        let kind = value >> 8;
        if !extended_telemetry || kind & 1 == 1 || kind == 0 {
            if value == ERPM_STOPPED {
                return Ok(Self::Erpm { erpm_div100: 0 });
            }
            // eee mmmmmmmmm: the period in microseconds is m << e.
            let period_us = ((value & 0x01ff) as u32) << (value >> 9);
            if period_us == 0 {
                return Err(DshotReplyError::ZeroPeriod);
            }
            // 60,000,000 us per minute, over 100, rounded.
            return Ok(Self::Erpm {
                erpm_div100: (600_000 + period_us / 2) / period_us,
            });
        }

        let data = (value & 0x00ff) as u8;
        Ok(match kind >> 1 {
            1 => Self::Temperature { celsius: data },
            2 => Self::Voltage {
                quarter_volts: data,
            },
            3 => Self::Current { amps: data },
            4..=6 => Self::Debug {
                channel: (kind >> 1) as u8 - 3,
                value: data,
            },
            _ => Self::StateEvent { value: data },
        })
    }
}

/// Every layer at once: pin samples to a reply.
pub fn decode_sampled_reply(
    samples: &[u16],
    pin_mask: u16,
    extended_telemetry: bool,
) -> Result<DshotReply, DshotReplyError> {
    let value = decode_gcr(reply_transitions(samples, pin_mask)?)?;
    DshotReply::from_value(value, extended_telemetry)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec::Vec;

    const GCR_ENCODE: [u32; 16] = [
        0x19, 0x1b, 0x12, 0x13, 0x1d, 0x15, 0x16, 0x17, 0x1a, 0x09, 0x0a, 0x0b, 0x1e, 0x0d, 0x0e,
        0x0f,
    ];

    /// What an ESC sends for `value`: the checksum, the GCR symbols, and the
    /// start edge, as transition bits.
    fn encode(value: u16) -> u32 {
        let value = value & 0x0fff;
        let checksum = !(value ^ (value >> 4) ^ (value >> 8)) & 0x0f;
        transitions_for((value << 4) | checksum)
    }

    fn transitions_for(word: u16) -> u32 {
        let mut gcr = 0;
        for nibble in 0..4 {
            gcr |= GCR_ENCODE[usize::from((word >> (4 * nibble)) & 0x0f)] << (5 * nibble);
        }
        (1 << 20) | gcr
    }

    /// The pin as sampled: `lead` idle samples, the reply at
    /// `num / den` samples per bit, then idle. Other pins carry noise.
    fn wire(transitions: u32, lead: usize, num: usize, den: usize, pin: u16) -> Vec<u16> {
        let noise = |index: usize| (index as u16).wrapping_mul(0x9e37) & !pin;
        let mut samples = Vec::new();
        let mut level = true;
        for _ in 0..lead {
            samples.push(pin | noise(samples.len()));
        }
        for bit in 0..REPLY_BITS as usize {
            if transitions & (1 << (20 - bit)) != 0 {
                level = !level;
            }
            let count = (bit + 1) * num / den - bit * num / den;
            for _ in 0..count {
                let sample = if level { pin } else { 0 };
                samples.push(sample | noise(samples.len()));
            }
        }
        for _ in 0..30 {
            samples.push(pin | noise(samples.len()));
        }
        samples
    }

    #[test]
    fn every_value_survives_the_wire_at_the_nominal_rate() {
        let pin = 1 << 15;
        for value in 0..=0x0fff {
            let samples = wire(encode(value), 7, 3, 1, pin);
            assert_eq!(
                reply_transitions(&samples, pin).and_then(decode_gcr),
                Ok(value),
                "value {value:#05x}"
            );
        }
    }

    #[test]
    fn every_value_survives_a_tenth_fast_or_slow_esc_clock() {
        let pin = 1 << 8;
        for (num, den) in [(27, 10), (33, 10)] {
            for value in 0..=0x0fff {
                let samples = wire(encode(value), 4, num, den, pin);
                assert_eq!(
                    reply_transitions(&samples, pin).and_then(decode_gcr),
                    Ok(value),
                    "value {value:#05x} at {num}/{den} samples per bit"
                );
            }
        }
    }

    #[test]
    fn an_idle_line_or_a_short_buffer_is_no_reply() {
        let pin = 1;
        assert_eq!(
            reply_transitions(&[pin; 200], pin),
            Err(DshotReplyError::NoReply)
        );
        assert_eq!(reply_transitions(&[], pin), Err(DshotReplyError::NoReply));
        let samples = wire(encode(0x123), 0, 3, 1, pin);
        assert_eq!(
            reply_transitions(&samples[..MIN_REPLY_SAMPLES - 1], pin),
            Err(DshotReplyError::NoReply)
        );
    }

    #[test]
    fn a_line_held_low_is_no_reply() {
        assert_eq!(
            reply_transitions(&[0; 200], 1),
            Err(DshotReplyError::NoReply)
        );
    }

    #[test]
    fn a_reply_starting_too_late_in_the_buffer_is_no_reply() {
        let pin = 1;
        let samples = wire(encode(0x123), 0, 3, 1, pin);
        // The start edge with fewer than a reply's worth of samples left.
        let late = [&[pin; 100][..], &samples[..MIN_REPLY_SAMPLES - 1]].concat();
        assert_eq!(reply_transitions(&late, pin), Err(DshotReplyError::NoReply));
    }

    #[test]
    fn extra_edges_after_a_reply_are_overlong() {
        let pin = 1;
        let mut samples = wire(encode(0x0aa), 0, 3, 1, pin);
        // A glitch inside the read window, after the reply's 63 samples.
        samples[64] = 0;
        samples[65] = 0;
        assert_eq!(
            reply_transitions(&samples, pin),
            Err(DshotReplyError::Overlong)
        );
    }

    #[test]
    fn a_wrong_checksum_is_rejected() {
        let value = 0x3f4;
        let good = !(value ^ (value >> 4) ^ (value >> 8)) & 0x0f;
        let bad = transitions_for((value << 4) | (good ^ 0x1));
        assert_eq!(decode_gcr(bad), Err(DshotReplyError::Checksum));
        assert_eq!(decode_gcr(encode(value)), Ok(value));
    }

    #[test]
    fn a_code_that_is_not_a_symbol_is_rejected() {
        // 0b00000 is not a GCR symbol, in any of the four places.
        for symbol in 0..4 {
            let transitions = encode(0x3f4) & !(0x1f << (5 * symbol));
            assert_eq!(decode_gcr(transitions), Err(DshotReplyError::InvalidSymbol));
        }
    }

    #[test]
    fn erpm_values_decode_from_their_period() {
        // 1,000 us is 500 << 1: 60,000 eRPM.
        assert_eq!(
            DshotReply::from_value((1 << 9) | 500, false),
            Ok(DshotReply::Erpm { erpm_div100: 600 })
        );
        // The shortest period, 1 us.
        assert_eq!(
            DshotReply::from_value(1, false),
            Ok(DshotReply::Erpm {
                erpm_div100: 600_000
            })
        );
        assert_eq!(
            DshotReply::from_value(0x0fff, false),
            Ok(DshotReply::Erpm { erpm_div100: 0 })
        );
        assert_eq!(
            DshotReply::from_value(0x0200, false),
            Err(DshotReplyError::ZeroPeriod)
        );
    }

    #[test]
    fn without_extended_telemetry_every_value_is_erpm() {
        // Type nibble 2 would be a temperature with EDT on.
        assert_eq!(
            DshotReply::from_value(0x250, false),
            Ok(DshotReply::Erpm { erpm_div100: 3750 })
        );
    }

    #[test]
    fn extended_telemetry_values_decode_by_type() {
        let cases = [
            (0x250, DshotReply::Temperature { celsius: 80 }),
            (0x430, DshotReply::Voltage { quarter_volts: 48 }),
            (0x612, DshotReply::Current { amps: 18 }),
            (
                0x801,
                DshotReply::Debug {
                    channel: 1,
                    value: 1,
                },
            ),
            (
                0xa02,
                DshotReply::Debug {
                    channel: 2,
                    value: 2,
                },
            ),
            (
                0xc03,
                DshotReply::Debug {
                    channel: 3,
                    value: 3,
                },
            ),
            (0xe05, DshotReply::StateEvent { value: 5 }),
        ];
        for (value, reply) in cases {
            assert_eq!(DshotReply::from_value(value, true), Ok(reply));
        }
    }

    #[test]
    fn with_extended_telemetry_odd_and_zero_types_stay_erpm() {
        assert_eq!(
            DshotReply::from_value(0x3f4, true),
            Ok(DshotReply::Erpm { erpm_div100: 600 })
        );
        // Exponent zero: type nibble 0 or 1.
        assert_eq!(
            DshotReply::from_value(0x0f0, true),
            Ok(DshotReply::Erpm { erpm_div100: 2500 })
        );
        assert_eq!(
            DshotReply::from_value(0x0fff, true),
            Ok(DshotReply::Erpm { erpm_div100: 0 })
        );
    }

    #[test]
    fn the_edt_acknowledgement_is_recognised_only_before_edt_is_on() {
        assert_eq!(
            DshotReply::from_value(EDT_ENABLE_ACK, false),
            Ok(DshotReply::ExtendedTelemetryEnabled)
        );
        assert_eq!(
            DshotReply::from_value(EDT_ENABLE_ACK, true),
            Ok(DshotReply::StateEvent { value: 0 })
        );
    }

    #[test]
    fn random_lines_never_panic_and_rarely_pass_the_checksum() {
        let mut state = 0x2545_f491_u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let mut accepted = 0;
        for length in [0, 1, 56, 57, 69, 100, 140] {
            for _ in 0..2_000 {
                // Runs of random length, so some buffers look like replies.
                let mut samples = Vec::with_capacity(length);
                let mut level = 1u16;
                while samples.len() < length {
                    for _ in 0..(next() % 9) {
                        samples.push(level);
                    }
                    level ^= 1;
                }
                samples.truncate(length);
                if decode_sampled_reply(&samples, 1, true).is_ok() {
                    accepted += 1;
                }
            }
        }
        // A random 20-bit word passes the symbol checks one time in 16 and
        // the checksum one in 16 of those; random runs are rarely even 21
        // bits long. 15 of these 14,000 pass.
        assert!(accepted < 140, "{accepted} random buffers accepted");
    }

    #[test]
    fn a_sampled_reply_decodes_end_to_end() {
        let pin = 1 << 3;
        let samples = wire(encode(0x430), 5, 3, 1, pin);
        assert_eq!(
            decode_sampled_reply(&samples, pin, true),
            Ok(DshotReply::Voltage { quarter_volts: 48 })
        );
        assert_eq!(
            decode_sampled_reply(&samples, pin, false),
            Ok(DshotReply::Erpm { erpm_div100: 3125 })
        );
    }
}
