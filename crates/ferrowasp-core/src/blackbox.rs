//! Fixed-size, allocation-free records for the onboard flight blackbox.
//!
//! Five 48-byte control records occupy the first 240 bytes of one NOR flash
//! page. A compact trailer identifies the flight/page and covers the complete
//! page with CRC-32, so a partially programmed page is rejected after a power
//! loss.

pub const FLASH_PAGE_LEN: usize = 256;
pub const RECORD_LEN: usize = 48;
pub const RECORDS_PER_PAGE: usize = 5;
pub const PAGE_DATA_LEN: usize = RECORD_LEN * RECORDS_PER_PAGE;
pub const PAGE_TRAILER_LEN: usize = FLASH_PAGE_LEN - PAGE_DATA_LEN;
pub const CONFIG_PAYLOAD_MAX: usize = 224;
/// Marks the first stored record of the first recorded flight after MCU boot.
///
/// Bits 0 and 1 retain their BB2 armed and fresh-IMU meanings. This additional
/// bit is metadata only and grants no runtime or actuator authority.
pub const FLIGHT_RECORD_FLAG_BOOT_SESSION_START: u16 = 1 << 2;

const PAGE_MAGIC: [u8; 2] = *b"FB";
const PAGE_VERSION: u8 = 1;
const CONFIG_MAGIC: [u8; 4] = *b"FWCF";
const CONFIG_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlightRecord {
    pub timestamp_us: u32,
    pub control_sequence: u32,
    pub imu_sequence: u32,
    pub flags: u16,
    pub raw_gyro_dps10: [i16; 3],
    pub filtered_gyro_dps10: [i16; 3],
    pub command_dps10: [i16; 3],
    pub pid: [i16; 3],
    pub throttle: u16,
    pub motors: [u16; 4],
}

impl FlightRecord {
    pub fn encode(self, output: &mut [u8; RECORD_LEN]) {
        let mut cursor = 0;
        put_u32(output, &mut cursor, self.timestamp_us);
        put_u32(output, &mut cursor, self.control_sequence);
        put_u32(output, &mut cursor, self.imu_sequence);
        put_u16(output, &mut cursor, self.flags);
        for value in self.raw_gyro_dps10 {
            put_i16(output, &mut cursor, value);
        }
        for value in self.filtered_gyro_dps10 {
            put_i16(output, &mut cursor, value);
        }
        for value in self.command_dps10 {
            put_i16(output, &mut cursor, value);
        }
        for value in self.pid {
            put_i16(output, &mut cursor, value);
        }
        put_u16(output, &mut cursor, self.throttle);
        for value in self.motors {
            put_u16(output, &mut cursor, value);
        }
        debug_assert_eq!(cursor, RECORD_LEN);
    }

    pub fn decode(input: &[u8; RECORD_LEN]) -> Self {
        let mut cursor = 0;
        let timestamp_us = take_u32(input, &mut cursor);
        let control_sequence = take_u32(input, &mut cursor);
        let imu_sequence = take_u32(input, &mut cursor);
        let flags = take_u16(input, &mut cursor);
        let raw_gyro_dps10 = take_i16x3(input, &mut cursor);
        let filtered_gyro_dps10 = take_i16x3(input, &mut cursor);
        let command_dps10 = take_i16x3(input, &mut cursor);
        let pid = take_i16x3(input, &mut cursor);
        let throttle = take_u16(input, &mut cursor);
        let motors = [
            take_u16(input, &mut cursor),
            take_u16(input, &mut cursor),
            take_u16(input, &mut cursor),
            take_u16(input, &mut cursor),
        ];
        debug_assert_eq!(cursor, RECORD_LEN);
        Self {
            timestamp_us,
            control_sequence,
            imu_sequence,
            flags,
            raw_gyro_dps10,
            filtered_gyro_dps10,
            command_dps10,
            pid,
            throttle,
            motors,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageMetadata {
    pub flight_id: u32,
    pub page_sequence: u32,
    pub record_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageError {
    InvalidRecordCount,
    InvalidMagic,
    UnsupportedVersion,
    CrcMismatch,
}

pub fn encode_page(
    flight_id: u32,
    page_sequence: u32,
    records: &[FlightRecord],
) -> Result<[u8; FLASH_PAGE_LEN], PageError> {
    if records.len() > RECORDS_PER_PAGE {
        return Err(PageError::InvalidRecordCount);
    }

    let mut page = [0xff; FLASH_PAGE_LEN];
    for (index, record) in records.iter().enumerate() {
        let mut encoded = [0u8; RECORD_LEN];
        record.encode(&mut encoded);
        let start = index * RECORD_LEN;
        page[start..start + RECORD_LEN].copy_from_slice(&encoded);
    }

    let trailer = PAGE_DATA_LEN;
    page[trailer..trailer + 2].copy_from_slice(&PAGE_MAGIC);
    page[trailer + 2] = PAGE_VERSION;
    page[trailer + 3] = records.len() as u8;
    page[trailer + 4..trailer + 8].copy_from_slice(&flight_id.to_le_bytes());
    page[trailer + 8..trailer + 12].copy_from_slice(&page_sequence.to_le_bytes());
    let crc = crc32(&page[..FLASH_PAGE_LEN - 4]);
    page[FLASH_PAGE_LEN - 4..].copy_from_slice(&crc.to_le_bytes());
    Ok(page)
}

pub fn decode_page(page: &[u8; FLASH_PAGE_LEN]) -> Result<PageMetadata, PageError> {
    let trailer = PAGE_DATA_LEN;
    if page[trailer..trailer + 2] != PAGE_MAGIC {
        return Err(PageError::InvalidMagic);
    }
    if page[trailer + 2] != PAGE_VERSION {
        return Err(PageError::UnsupportedVersion);
    }
    let record_count = page[trailer + 3];
    if record_count as usize > RECORDS_PER_PAGE {
        return Err(PageError::InvalidRecordCount);
    }
    let expected = u32::from_le_bytes(page[FLASH_PAGE_LEN - 4..].try_into().unwrap());
    if crc32(&page[..FLASH_PAGE_LEN - 4]) != expected {
        return Err(PageError::CrcMismatch);
    }
    Ok(PageMetadata {
        flight_id: u32::from_le_bytes(page[trailer + 4..trailer + 8].try_into().unwrap()),
        page_sequence: u32::from_le_bytes(page[trailer + 8..trailer + 12].try_into().unwrap()),
        record_count,
    })
}

pub fn record_from_page(
    page: &[u8; FLASH_PAGE_LEN],
    index: usize,
) -> Result<FlightRecord, PageError> {
    let metadata = decode_page(page)?;
    if index >= metadata.record_count as usize {
        return Err(PageError::InvalidRecordCount);
    }
    let start = index * RECORD_LEN;
    let encoded: &[u8; RECORD_LEN] = page[start..start + RECORD_LEN].try_into().unwrap();
    Ok(FlightRecord::decode(encoded))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    PayloadTooLarge,
    InvalidMagic,
    UnsupportedVersion,
    InvalidLength,
    CrcMismatch,
}

pub fn encode_config_page(
    sequence: u32,
    payload: &[u8],
) -> Result<[u8; FLASH_PAGE_LEN], ConfigError> {
    if payload.len() > CONFIG_PAYLOAD_MAX {
        return Err(ConfigError::PayloadTooLarge);
    }
    let mut page = [0xff; FLASH_PAGE_LEN];
    page[..4].copy_from_slice(&CONFIG_MAGIC);
    page[4] = CONFIG_VERSION;
    page[6..8].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    page[8..12].copy_from_slice(&sequence.to_le_bytes());
    page[16..16 + payload.len()].copy_from_slice(payload);
    let crc = crc32(&page[..FLASH_PAGE_LEN - 4]);
    page[FLASH_PAGE_LEN - 4..].copy_from_slice(&crc.to_le_bytes());
    Ok(page)
}

pub fn decode_config_page(page: &[u8; FLASH_PAGE_LEN]) -> Result<(u32, &[u8]), ConfigError> {
    if page[..4] != CONFIG_MAGIC {
        return Err(ConfigError::InvalidMagic);
    }
    if page[4] != CONFIG_VERSION {
        return Err(ConfigError::UnsupportedVersion);
    }
    let payload_len = u16::from_le_bytes(page[6..8].try_into().unwrap()) as usize;
    if payload_len > CONFIG_PAYLOAD_MAX {
        return Err(ConfigError::InvalidLength);
    }
    let expected = u32::from_le_bytes(page[FLASH_PAGE_LEN - 4..].try_into().unwrap());
    if crc32(&page[..FLASH_PAGE_LEN - 4]) != expected {
        return Err(ConfigError::CrcMismatch);
    }
    let sequence = u32::from_le_bytes(page[8..12].try_into().unwrap());
    Ok((sequence, &page[16..16 + payload_len]))
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn put_u32(output: &mut [u8], cursor: &mut usize, value: u32) {
    output[*cursor..*cursor + 4].copy_from_slice(&value.to_le_bytes());
    *cursor += 4;
}

fn put_u16(output: &mut [u8], cursor: &mut usize, value: u16) {
    output[*cursor..*cursor + 2].copy_from_slice(&value.to_le_bytes());
    *cursor += 2;
}

fn put_i16(output: &mut [u8], cursor: &mut usize, value: i16) {
    put_u16(output, cursor, value as u16);
}

fn take_u32(input: &[u8], cursor: &mut usize) -> u32 {
    let value = u32::from_le_bytes(input[*cursor..*cursor + 4].try_into().unwrap());
    *cursor += 4;
    value
}

fn take_u16(input: &[u8], cursor: &mut usize) -> u16 {
    let value = u16::from_le_bytes(input[*cursor..*cursor + 2].try_into().unwrap());
    *cursor += 2;
    value
}

fn take_i16x3(input: &[u8], cursor: &mut usize) -> [i16; 3] {
    [
        take_u16(input, cursor) as i16,
        take_u16(input, cursor) as i16,
        take_u16(input, cursor) as i16,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(seed: i16) -> FlightRecord {
        FlightRecord {
            timestamp_us: 123_456 + seed as u32,
            control_sequence: 42,
            imu_sequence: 105,
            flags: 3,
            raw_gyro_dps10: [seed, -2, 3],
            filtered_gyro_dps10: [4, -5, 6],
            command_dps10: [7, 8, -9],
            pid: [-10, 11, 12],
            throttle: 321,
            motors: [100, 101, 102, 103],
        }
    }

    #[test]
    fn record_round_trip_is_exact_and_fixed_size() {
        let expected = record(1);
        let mut encoded = [0; RECORD_LEN];
        expected.encode(&mut encoded);
        assert_eq!(FlightRecord::decode(&encoded), expected);
    }

    #[test]
    fn five_records_fill_one_crc_protected_page() {
        let records = [record(1), record(2), record(3), record(4), record(5)];
        let page = encode_page(7, 9, &records).unwrap();
        assert_eq!(
            decode_page(&page).unwrap(),
            PageMetadata {
                flight_id: 7,
                page_sequence: 9,
                record_count: 5,
            }
        );
        assert_eq!(record_from_page(&page, 4).unwrap(), records[4]);
    }

    #[test]
    fn partial_or_corrupt_page_is_rejected() {
        let mut page = encode_page(1, 0, &[record(1)]).unwrap();
        page[17] ^= 0x80;
        assert_eq!(decode_page(&page), Err(PageError::CrcMismatch));
    }

    #[test]
    fn config_slots_round_trip_and_reject_corruption() {
        let mut page = encode_config_page(14, b"pitch_p=0.30").unwrap();
        let (sequence, payload) = decode_config_page(&page).unwrap();
        assert_eq!(sequence, 14);
        assert_eq!(payload, b"pitch_p=0.30");
        page[16] ^= 1;
        assert_eq!(decode_config_page(&page), Err(ConfigError::CrcMismatch));
    }

    #[test]
    fn crc_matches_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
