//! One configuration slot: a flash page holding a versioned, CRC-protected
//! payload, so a slot cut short by power loss never decodes.

pub const FLASH_PAGE_LEN: usize = 256;
pub const CONFIG_PAYLOAD_MAX: usize = 224;

const CONFIG_MAGIC: [u8; 4] = *b"FWCF";
const CONFIG_VERSION: u8 = 1;

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

#[cfg(test)]
mod tests {
    use super::*;

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
