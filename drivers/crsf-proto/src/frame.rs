//! The CRSF frame layout: addresses, frame types, length bounds and the
//! CRC-8/DVB-S2 every frame ends with.
//!
//! A frame is an address byte, a length byte counting the type, payload and
//! CRC, a type byte, the payload, and a CRC-8/DVB-S2 over the type and
//! payload.
//!
//! Requirements: CRSF-001, CRSF-002, CRSF-006, CRSF-010
//! Safety-critical.

/// Longest frame on the wire: address, length, and up to 62 counted bytes.
pub const MAX_FRAME_LEN: usize = 64;
/// Shortest frame on the wire: address, length, type and CRC.
pub const MIN_FRAME_LEN: usize = 4;
/// Longest payload: a frame less its address, length, type and CRC.
pub const MAX_PAYLOAD_LEN: usize = MAX_FRAME_LEN - 4;

/// Address a receiver uses for frames sent to the flight controller, and the
/// sync byte a flight controller starts its own telemetry frames with.
pub const ADDRESS_FLIGHT_CONTROLLER: u8 = 0xC8;
/// Address of the receiver, seen on the flight-controller link.
pub const ADDRESS_RECEIVER: u8 = 0xEC;
/// Address of the transmitter module, seen on the flight-controller link.
pub const ADDRESS_TRANSMITTER: u8 = 0xEE;

/// GPS position, ground speed, heading, altitude and satellites.
pub const FRAME_TYPE_GPS: u8 = 0x02;
/// Vertical speed.
pub const FRAME_TYPE_VARIO: u8 = 0x07;
/// Battery voltage, current, charge drawn and charge remaining.
pub const FRAME_TYPE_BATTERY_SENSOR: u8 = 0x08;
/// Barometric altitude and vertical speed, packed into three bytes.
pub const FRAME_TYPE_BARO_ALTITUDE: u8 = 0x09;
/// Link statistics from the receiver.
pub const FRAME_TYPE_LINK_STATISTICS: u8 = 0x14;
/// Sixteen packed RC channels.
pub const FRAME_TYPE_RC_CHANNELS_PACKED: u8 = 0x16;
/// Pitch, roll and yaw.
pub const FRAME_TYPE_ATTITUDE: u8 = 0x1E;
/// The flight mode as a null-terminated string.
pub const FRAME_TYPE_FLIGHT_MODE: u8 = 0x21;

/// The length byte counts the type and CRC, so two is the empty frame.
pub(crate) const MIN_LENGTH_FIELD: u8 = 2;
pub(crate) const MAX_LENGTH_FIELD: u8 = (MAX_FRAME_LEN - 2) as u8;

/// CRC-8/DVB-S2 (polynomial 0xD5), as CRSF uses over type and payload.
pub fn crc8_dvb_s2(bytes: &[u8]) -> u8 {
    let mut crc = 0_u8;
    for byte in bytes {
        crc ^= *byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0xD5
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Whether `byte` is one of the three addresses a frame on the
/// flight-controller link may start with.
pub const fn is_frame_address(byte: u8) -> bool {
    matches!(
        byte,
        ADDRESS_FLIGHT_CONTROLLER | ADDRESS_RECEIVER | ADDRESS_TRANSMITTER
    )
}

/// Whether `frame` is exactly one well-formed frame: a known address, a
/// length byte within bounds that matches the slice, and a matching CRC.
pub fn is_well_formed(frame: &[u8]) -> bool {
    let (Some(&address), Some(&length)) = (frame.first(), frame.get(1)) else {
        return false;
    };
    if !is_frame_address(address) || !(MIN_LENGTH_FIELD..=MAX_LENGTH_FIELD).contains(&length) {
        return false;
    }
    if frame.len() != usize::from(length) + 2 {
        return false;
    }
    match frame.split_last() {
        Some((crc, rest)) => rest.get(2..).is_some_and(|body| crc8_dvb_s2(body) == *crc),
        None => false,
    }
}

/// Write the address, length, type and CRC around a payload already in
/// `frame[3..frame.len() - 1]`. `frame` is always at least a minimal frame,
/// because every caller passes a fixed array or a bounded slice of one.
pub(crate) fn seal(frame: &mut [u8], frame_type: u8) {
    let len = frame.len();
    if !(MIN_FRAME_LEN..=MAX_FRAME_LEN).contains(&len) {
        return;
    }
    frame[0] = ADDRESS_FLIGHT_CONTROLLER;
    frame[1] = (len - 2) as u8;
    frame[2] = frame_type;
    frame[len - 1] = crc8_dvb_s2(&frame[2..len - 1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_dvb_s2_check_value() {
        assert_eq!(crc8_dvb_s2(b"123456789"), 0xBC);
    }

    #[test]
    fn only_the_three_link_addresses_start_a_frame() {
        let accepted: usize = (0..=u8::MAX).filter(|byte| is_frame_address(*byte)).count();
        assert_eq!(accepted, 3);
        assert!(is_frame_address(0xC8) && is_frame_address(0xEC) && is_frame_address(0xEE));
        assert!(!is_frame_address(0x00));
    }

    #[test]
    fn a_sealed_frame_is_well_formed_and_any_corruption_is_not() {
        let mut frame = [0_u8; 7];
        frame[3..6].copy_from_slice(&[1, 2, 3]);
        seal(&mut frame, 0x09);
        assert_eq!(frame[..3], [ADDRESS_FLIGHT_CONTROLLER, 5, 0x09]);
        assert!(is_well_formed(&frame));

        for index in 0..frame.len() {
            for bit in 0..8 {
                let mut corrupt = frame;
                corrupt[index] ^= 1 << bit;
                assert!(!is_well_formed(&corrupt), "byte {index} bit {bit}");
            }
        }
        assert!(!is_well_formed(&frame[..6]));
        assert!(!is_well_formed(&[]));
        assert!(!is_well_formed(&[ADDRESS_FLIGHT_CONTROLLER]));
        assert!(!is_well_formed(&[ADDRESS_FLIGHT_CONTROLLER, 2, 0]));
    }
}
