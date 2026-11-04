//! Packing and unpacking of the sixteen 11-bit RC channels.
//!
//! Channels are packed least-significant bit first, on the 172..=1811 scale
//! SBUS also uses, so channel consumers need not know which protocol
//! delivered them.
//!
//! Requirements: CRSF-005
//! Safety-critical.

/// Channels carried by one RC channels frame.
pub const CHANNEL_COUNT: usize = 16;
/// Payload length of an RC channels frame: sixteen 11-bit values.
pub const RC_CHANNELS_PAYLOAD_LEN: usize = 22;

/// Unpack sixteen 11-bit channels, least-significant bit first.
pub fn unpack_channels(payload: &[u8; RC_CHANNELS_PAYLOAD_LEN]) -> [u16; CHANNEL_COUNT] {
    let mut channels = [0_u16; CHANNEL_COUNT];
    let mut accumulator = 0_u32;
    let mut bits = 0_u32;
    let mut index = 0;
    for byte in payload {
        accumulator |= u32::from(*byte) << bits;
        bits += 8;
        if bits >= 11 {
            if let Some(channel) = channels.get_mut(index) {
                *channel = (accumulator & 0x07FF) as u16;
            }
            accumulator >>= 11;
            bits -= 11;
            index += 1;
        }
    }
    channels
}

/// Pack sixteen channels into a payload; values are truncated to 11 bits.
pub fn pack_channels(channels: &[u16; CHANNEL_COUNT]) -> [u8; RC_CHANNELS_PAYLOAD_LEN] {
    let mut payload = [0_u8; RC_CHANNELS_PAYLOAD_LEN];
    let mut accumulator = 0_u32;
    let mut bits = 0_u32;
    let mut index = 0;
    for channel in channels {
        accumulator |= u32::from(*channel & 0x07FF) << bits;
        bits += 11;
        while bits >= 8 {
            if let Some(byte) = payload.get_mut(index) {
                *byte = accumulator as u8;
            }
            accumulator >>= 8;
            bits -= 8;
            index += 1;
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_round_trip_through_the_packed_payload() {
        let mut channels = [0_u16; CHANNEL_COUNT];
        for (index, channel) in channels.iter_mut().enumerate() {
            *channel = 172 + (index as u16) * 109;
        }
        assert_eq!(unpack_channels(&pack_channels(&channels)), channels);
    }

    #[test]
    fn first_channel_occupies_the_low_bits_of_the_first_byte() {
        let mut channels = [0_u16; CHANNEL_COUNT];
        channels[0] = 0x07FF;
        let payload = pack_channels(&channels);
        assert_eq!(payload[0], 0xFF);
        assert_eq!(payload[1], 0x07);
        assert!(payload[2..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn the_last_channel_occupies_the_high_bits_of_the_last_byte() {
        let mut channels = [0_u16; CHANNEL_COUNT];
        channels[CHANNEL_COUNT - 1] = 0x07FF;
        let payload = pack_channels(&channels);
        assert_eq!(payload[RC_CHANNELS_PAYLOAD_LEN - 1], 0xFF);
        assert_eq!(payload[RC_CHANNELS_PAYLOAD_LEN - 2], 0xE0);
        assert!(
            payload[..RC_CHANNELS_PAYLOAD_LEN - 2]
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    #[test]
    fn bits_above_eleven_are_dropped_when_packing() {
        let channels = [0xF800_u16 | 992; CHANNEL_COUNT];
        assert_eq!(
            unpack_channels(&pack_channels(&channels)),
            [992; CHANNEL_COUNT]
        );
    }
}
