pub const DSHOT_BITS: usize = 16;
pub const RESET_SLOTS: usize = 6;
pub const FRAME_SLOTS: usize = DSHOT_BITS + RESET_SLOTS;
pub const COMPARE_DMA_SLOTS: usize = DSHOT_BITS;
pub const DSHOT_MAX_VALUE: u16 = 2047;
pub const DSHOT_MIN_THROTTLE: u16 = 48;
pub const DSHOT_MAX_THROTTLE: u16 = 2047;
pub const DSHOT600_BITRATE_HZ: u32 = 600_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotFrameError {
    ValueOutOfRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DshotPacket(pub u16);

impl DshotPacket {
    pub const fn try_from_value(value: u16, telemetry: bool) -> Result<Self, DshotFrameError> {
        if value > DSHOT_MAX_VALUE {
            return Err(DshotFrameError::ValueOutOfRange);
        }

        Ok(Self::from_value(value, telemetry))
    }

    pub const fn from_value(value: u16, telemetry: bool) -> Self {
        let payload = ((value & 0x07ff) << 1) | telemetry as u16;
        let crc = (payload ^ (payload >> 4) ^ (payload >> 8)) & 0x0f;

        Self((payload << 4) | crc)
    }
}

/// Packs one synchronized four-lane frame set and sets the telemetry bit on at
/// most one lane.
pub fn four_motor_packets(values: [u16; 4], telemetry_lane: Option<usize>) -> [DshotPacket; 4] {
    core::array::from_fn(|index| {
        DshotPacket::from_value(values[index], telemetry_lane == Some(index))
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotTimingError {
    ZeroTimerClock,
    ZeroBitrate,
    PeriodTooShort,
    PeriodTooLong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DshotTiming {
    pub arr: u16,
    pub zero_high: u16,
    pub one_high: u16,
}

impl DshotTiming {
    pub const fn from_clocks(timer_hz: u32, bitrate_hz: u32) -> Result<Self, DshotTimingError> {
        if timer_hz == 0 {
            return Err(DshotTimingError::ZeroTimerClock);
        }
        if bitrate_hz == 0 {
            return Err(DshotTimingError::ZeroBitrate);
        }

        let period = div_round(timer_hz, bitrate_hz);
        if period < 4 {
            return Err(DshotTimingError::PeriodTooShort);
        }
        if period > u16::MAX as u32 + 1 {
            return Err(DshotTimingError::PeriodTooLong);
        }

        let zero_high = div_round(period * 3, 8);
        let one_high = div_round(period * 3, 4);
        if zero_high == 0 || one_high >= period {
            return Err(DshotTimingError::PeriodTooShort);
        }

        Ok(Self {
            arr: (period - 1) as u16,
            zero_high: zero_high as u16,
            one_high: one_high as u16,
        })
    }

    #[inline]
    pub const fn duty_for_bit(self, set: bool) -> u16 {
        if set { self.one_high } else { self.zero_high }
    }
}

const fn div_round(numerator: u32, denominator: u32) -> u32 {
    ((numerator as u64 + denominator as u64 / 2) / denominator as u64) as u32
}

pub fn encode_packet(packet: DshotPacket, max_duty: u16, output: &mut [u16; FRAME_SLOTS]) {
    let duty_0 = (max_duty as u32 * 3 / 8) as u16;
    let duty_1 = (max_duty as u32 * 3 / 4) as u16;

    for (index, slot) in output[..DSHOT_BITS].iter_mut().enumerate() {
        let bit = (packet.0 >> (15 - index)) & 1;
        *slot = if bit == 0 { duty_0 } else { duty_1 };
    }

    for slot in &mut output[DSHOT_BITS..] {
        *slot = 0;
    }
}

/// Encodes the compare-event DMA sequence used by STM32 timer PWM preload.
///
/// The first frame duty is returned for the caller to write directly to CCR.
/// DMA owns the remaining 15 bit duties followed by a final zero, which makes
/// the timer output inactive after the last bit.
pub fn encode_compare_sequence(
    packet: DshotPacket,
    timing: DshotTiming,
    dma_output: &mut [u16; COMPARE_DMA_SLOTS],
) -> u16 {
    let first = timing.duty_for_bit(packet.0 & 0x8000 != 0);

    for (index, destination) in dma_output[..DSHOT_BITS - 1].iter_mut().enumerate() {
        let bit_number = 14 - index;
        *destination = timing.duty_for_bit(packet.0 & (1 << bit_number) != 0);
    }

    dma_output[COMPARE_DMA_SLOTS - 1] = 0;
    first
}

pub fn throttle_to_dshot(value: u16) -> u16 {
    if value == 0 {
        return 0;
    }

    let value = value.min(2000) as u32;
    let span = (DSHOT_MAX_THROTTLE - DSHOT_MIN_THROTTLE) as u32;
    (DSHOT_MIN_THROTTLE as u32 + (value * span) / 2000) as u16
}

pub fn throttles_to_dshot(values: [u16; 4]) -> [u16; 4] {
    values.map(throttle_to_dshot)
}

/// Returns true once a bounded command lease is older than its duration.
///
/// The wrapping subtraction keeps the comparison valid across a `u32`
/// millisecond-counter rollover, provided leases remain shorter than half the
/// counter range.
pub const fn command_lease_expired(started_ms: u32, duration_ms: u32, now_ms: u32) -> bool {
    now_ms.wrapping_sub(started_ms) > duration_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_crc_matches_known_values() {
        assert_eq!(DshotPacket::from_value(0, false).0, 0x0000);
        assert_eq!(DshotPacket::from_value(48, false).0, 0x0606);
        assert_eq!(DshotPacket::from_value(2047, true).0, 0xffff);
        assert_eq!(
            DshotPacket::try_from_value(2048, false),
            Err(DshotFrameError::ValueOutOfRange)
        );
    }

    #[test]
    fn four_motor_packet_set_requests_exactly_one_selected_lane() {
        let without = four_motor_packets([48, 49, 50, 51], None);
        let with = four_motor_packets([48, 49, 50, 51], Some(2));

        for index in 0..4 {
            // The telemetry bit is payload bit zero and therefore wire bit 4.
            assert_eq!((without[index].0 >> 4) & 1, 0);
            assert_eq!((with[index].0 >> 4) & 1, u16::from(index == 2));
        }
    }

    #[test]
    fn out_of_range_telemetry_lane_requests_none() {
        let packets = four_motor_packets([0; 4], Some(4));
        assert!(packets.iter().all(|packet| packet.0 == 0));
    }

    #[test]
    fn encoder_emits_duty_slots_and_reset_tail() {
        let mut slots = [u16::MAX; FRAME_SLOTS];
        encode_packet(DshotPacket(0b1000_0000_0000_0001), 1000, &mut slots);

        assert_eq!(slots[0], 750);
        assert_eq!(slots[1], 375);
        assert_eq!(slots[14], 375);
        assert_eq!(slots[15], 750);
        assert!(slots[16..].iter().all(|slot| *slot == 0));
    }

    #[test]
    fn encoder_accepts_the_full_u16_timer_range_without_overflow() {
        let mut slots = [0; FRAME_SLOTS];
        encode_packet(DshotPacket(0x8000), u16::MAX, &mut slots);

        assert_eq!(slots[0], 49_151);
        assert_eq!(slots[1], 24_575);
    }

    #[test]
    fn dshot600_timing_matches_tim1_at_168_mhz() {
        assert_eq!(
            DshotTiming::from_clocks(168_000_000, DSHOT600_BITRATE_HZ),
            Ok(DshotTiming {
                arr: 279,
                zero_high: 105,
                one_high: 210,
            })
        );
    }

    #[test]
    fn compare_sequence_stages_first_bit_and_terminating_zero() {
        let timing = DshotTiming::from_clocks(168_000_000, DSHOT600_BITRATE_HZ).unwrap();
        let packet = DshotPacket(0b1000_0000_0000_0001);
        let mut slots = [u16::MAX; COMPARE_DMA_SLOTS];

        let first = encode_compare_sequence(packet, timing, &mut slots);

        assert_eq!(first, timing.one_high);
        assert_eq!(slots[0], timing.zero_high);
        assert_eq!(slots[13], timing.zero_high);
        assert_eq!(slots[14], timing.one_high);
        assert_eq!(slots[15], 0);
    }

    #[test]
    fn timing_rejects_invalid_clock_inputs() {
        assert_eq!(
            DshotTiming::from_clocks(0, DSHOT600_BITRATE_HZ),
            Err(DshotTimingError::ZeroTimerClock)
        );
        assert_eq!(
            DshotTiming::from_clocks(168_000_000, 0),
            Err(DshotTimingError::ZeroBitrate)
        );
        assert_eq!(
            DshotTiming::from_clocks(1_000_000, 600_000),
            Err(DshotTimingError::PeriodTooShort)
        );
        assert_eq!(
            DshotTiming::from_clocks(u32::MAX, 1),
            Err(DshotTimingError::PeriodTooLong)
        );
    }

    #[test]
    fn throttle_mapping_is_bounded() {
        assert_eq!(throttle_to_dshot(0), 0);
        assert_eq!(throttle_to_dshot(1), DSHOT_MIN_THROTTLE);
        assert_eq!(throttle_to_dshot(2000), DSHOT_MAX_THROTTLE);
        assert_eq!(throttle_to_dshot(2500), DSHOT_MAX_THROTTLE);
    }

    #[test]
    fn unequal_bench_commands_have_distinct_expected_wire_values() {
        assert_eq!(
            throttles_to_dshot([80, 100, 140, 120]),
            [127, 147, 187, 167]
        );
    }

    #[test]
    fn mixed_control_vector_preserves_physical_lane_order() {
        assert_eq!(
            throttles_to_dshot([1100, 1100, 900, 900]),
            [1147, 1147, 947, 947]
        );
        assert_eq!(
            throttles_to_dshot([0, 1, 2000, u16::MAX]),
            [
                0,
                DSHOT_MIN_THROTTLE,
                DSHOT_MAX_THROTTLE,
                DSHOT_MAX_THROTTLE
            ]
        );
    }

    #[test]
    fn command_lease_expires_after_the_inclusive_deadline() {
        assert!(!command_lease_expired(100, 20, 119));
        assert!(!command_lease_expired(100, 20, 120));
        assert!(command_lease_expired(100, 20, 121));
    }

    #[test]
    fn command_lease_expiry_handles_millisecond_counter_rollover() {
        let started = u32::MAX - 9;

        assert!(!command_lease_expired(started, 20, 10));
        assert!(command_lease_expired(started, 20, 11));
    }

    #[test]
    fn periodically_renewed_command_lease_covers_a_long_guarded_hold() {
        const HOLD_MS: u32 = 500;
        const REFRESH_MS: u32 = 10;
        const LEASE_MS: u32 = 20;

        let mut now_ms = 0;

        while now_ms < HOLD_MS {
            let lease_started_ms = now_ms;
            now_ms += REFRESH_MS;
            assert!(!command_lease_expired(lease_started_ms, LEASE_MS, now_ms));
        }

        let lease_started_ms = now_ms;
        assert!(!command_lease_expired(
            lease_started_ms,
            LEASE_MS,
            now_ms + LEASE_MS
        ));
        assert!(command_lease_expired(
            lease_started_ms,
            LEASE_MS,
            now_ms + LEASE_MS + 1
        ));
    }
}
