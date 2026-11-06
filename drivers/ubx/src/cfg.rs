//! The M10 configuration database: `CFG-VALSET` and `CFG-VALGET` requests,
//! `CFG-VALGET` responses, and the `ACK-ACK` or `ACK-NAK` that answers each
//! request.
//!
//! Citations name files under each reference crate's `src/`: ublox 0.10.0's
//! key list is `ubx_packets/cfg_val.rs` and its message layouts
//! `ubx_packets/packets/cfg_val.rs`; kinavis-ubx 0.1.1's are both in
//! `config.rs`.

use core::fmt;

use crate::frame::{EncodeError, Frame, encode_with};
use crate::id::{ACK_ACK, ACK_NAK, CFG_VALGET, CFG_VALSET, CLASS_ACK, CLASS_CFG};

/// Most keys one request carries: ublox `ubx_packets/packets/cfg_val.rs:34`
/// and kinavis-ubx `config.rs:23`.
pub const MAX_KEYS: usize = 64;
/// The longest `CFG-VALSET` payload: four header bytes and 64 keys of the
/// largest size, as ublox `ubx_packets/packets/cfg_val.rs:21` sizes it.
pub const MAX_VALSET_PAYLOAD: usize = 4 + (4 + 8) * MAX_KEYS;
/// The longest `CFG-VALGET` request payload: ublox
/// `ubx_packets/packets/cfg_val.rs:40`.
pub const MAX_VALGET_REQUEST_PAYLOAD: usize = 4 + 4 * MAX_KEYS;

/// Storage size of a key's value, from bits 28-30 of its ID: ublox
/// `ubx_packets/cfg_val.rs:38-47` and kinavis-ubx `config.rs:66-87`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ValueSize {
    /// One bit, carried in one byte as 0 or 1.
    Bit,
    Byte,
    Two,
    Four,
    Eight,
}

impl ValueSize {
    const fn from_key(raw: u32) -> Option<Self> {
        match (raw >> 28) & 0x7 {
            1 => Some(Self::Bit),
            2 => Some(Self::Byte),
            3 => Some(Self::Two),
            4 => Some(Self::Four),
            5 => Some(Self::Eight),
            _ => None,
        }
    }

    /// Bytes the value takes on the wire.
    pub const fn bytes(self) -> usize {
        match self {
            Self::Bit | Self::Byte => 1,
            Self::Two => 2,
            Self::Four => 4,
            Self::Eight => 8,
        }
    }

    /// Whether `value` fits: 0 or 1 for a bit, otherwise the size's width.
    pub const fn fits(self, value: u64) -> bool {
        match self {
            Self::Bit => value <= 1,
            Self::Byte => value <= 0xFF,
            Self::Two => value <= 0xFFFF,
            Self::Four => value <= 0xFFFF_FFFF,
            Self::Eight => true,
        }
    }
}

/// A configuration key ID whose size bits name a storage size.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Key(u32);

impl Key {
    /// `CFG-RATE-MEAS`, the measurement period in ms (U2): ublox
    /// `ubx_packets/cfg_val.rs:603`, kinavis-ubx `config.rs:32`.
    pub const RATE_MEAS: Self = Self(0x3021_0001);
    /// `CFG-RATE-NAV`, measurements per navigation solution (U2): ublox
    /// `ubx_packets/cfg_val.rs:605`, kinavis-ubx `config.rs:34`.
    pub const RATE_NAV: Self = Self(0x3021_0002);
    /// `CFG-RATE-TIMEREF`, the time system measurements align to (E1): ublox
    /// `ubx_packets/cfg_val.rs:607`, kinavis-ubx `config.rs:36`.
    pub const RATE_TIMEREF: Self = Self(0x2021_0003);
    /// `CFG-UART1-BAUDRATE` (U4): ublox `ubx_packets/cfg_val.rs:491`,
    /// kinavis-ubx `config.rs:40`.
    pub const UART1_BAUDRATE: Self = Self(0x4052_0001);
    /// `CFG-UART1OUTPROT-UBX` (L): ublox `ubx_packets/cfg_val.rs:503`,
    /// kinavis-ubx `config.rs:42`.
    pub const UART1OUTPROT_UBX: Self = Self(0x1074_0001);
    /// `CFG-UART1OUTPROT-NMEA` (L): ublox `ubx_packets/cfg_val.rs:504`,
    /// kinavis-ubx `config.rs:44`.
    pub const UART1OUTPROT_NMEA: Self = Self(0x1074_0002);
    /// `CFG-I2COUTPROT-UBX` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:566`.
    pub const I2COUTPROT_UBX: Self = Self(0x1072_0001);
    /// `CFG-I2COUTPROT-NMEA` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:567`.
    pub const I2COUTPROT_NMEA: Self = Self(0x1072_0002);
    /// `CFG-SPIOUTPROT-UBX` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:584`.
    pub const SPIOUTPROT_UBX: Self = Self(0x107A_0001);
    /// `CFG-SPIOUTPROT-NMEA` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:585`.
    pub const SPIOUTPROT_NMEA: Self = Self(0x107A_0002);
    /// `CFG-MSGOUT-UBX_NAV_PVT_UART1`, NAV-PVT per navigation solution on
    /// UART1 (U1): ublox `ubx_packets/cfg_val.rs:1176`, kinavis-ubx
    /// `config.rs:46`.
    pub const MSGOUT_UBX_NAV_PVT_UART1: Self = Self(0x2091_0007);
    /// `CFG-MSGOUT-UBX_NAV_PVT_I2C` (U1): single-source, ublox
    /// `ubx_packets/cfg_val.rs:1172`.
    pub const MSGOUT_UBX_NAV_PVT_I2C: Self = Self(0x2091_0006);
    /// `CFG-MSGOUT-UBX_NAV_PVT_SPI` (U1): single-source, ublox
    /// `ubx_packets/cfg_val.rs:1174`.
    pub const MSGOUT_UBX_NAV_PVT_SPI: Self = Self(0x2091_000A);
    /// `CFG-NAVSPG-DYNMODEL`, the dynamic platform model (E1): ublox
    /// `ubx_packets/cfg_val.rs:1528`, kinavis-ubx `config.rs:38`. Its values
    /// are [`DynamicModel`].
    pub const NAVSPG_DYNMODEL: Self = Self(0x2011_0021);
    /// `CFG-SIGNAL-GPS_ENA` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:1423`.
    pub const SIGNAL_GPS_ENA: Self = Self(0x1031_001F);
    /// `CFG-SIGNAL-GAL_ENA` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:1426`.
    pub const SIGNAL_GAL_ENA: Self = Self(0x1031_0021);
    /// `CFG-SIGNAL-BDS_ENA` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:1429`.
    pub const SIGNAL_BDS_ENA: Self = Self(0x1031_0022);
    /// `CFG-SIGNAL-QZSS_ENA` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:1432`.
    pub const SIGNAL_QZSS_ENA: Self = Self(0x1031_0024);
    /// `CFG-SIGNAL-GLO_ENA` (L): single-source, ublox
    /// `ubx_packets/cfg_val.rs:1435`.
    pub const SIGNAL_GLO_ENA: Self = Self(0x1031_0025);
    /// `CFG-SIGNAL-SBAS_ENA` (L): single-source, PX4 `src/ubx.h`
    /// (`UBX_CFG_KEY_SIGNAL_SBAS_ENA 0x10310020`); neither reference crate
    /// carries it.
    pub const SIGNAL_SBAS_ENA: Self = Self(0x1031_0020);

    /// Every key this crate names, for tests and tables.
    pub const KNOWN: [Self; 20] = [
        Self::RATE_MEAS,
        Self::RATE_NAV,
        Self::RATE_TIMEREF,
        Self::UART1_BAUDRATE,
        Self::UART1OUTPROT_UBX,
        Self::UART1OUTPROT_NMEA,
        Self::I2COUTPROT_UBX,
        Self::I2COUTPROT_NMEA,
        Self::SPIOUTPROT_UBX,
        Self::SPIOUTPROT_NMEA,
        Self::MSGOUT_UBX_NAV_PVT_UART1,
        Self::MSGOUT_UBX_NAV_PVT_I2C,
        Self::MSGOUT_UBX_NAV_PVT_SPI,
        Self::NAVSPG_DYNMODEL,
        Self::SIGNAL_GPS_ENA,
        Self::SIGNAL_GAL_ENA,
        Self::SIGNAL_BDS_ENA,
        Self::SIGNAL_QZSS_ENA,
        Self::SIGNAL_GLO_ENA,
        Self::SIGNAL_SBAS_ENA,
    ];

    /// A key from its 32-bit ID; refused when bits 28-30 name no size.
    pub const fn new(raw: u32) -> Result<Self, CfgError> {
        match ValueSize::from_key(raw) {
            Some(_) => Ok(Self(raw)),
            None => Err(CfgError::BadKeySize { key: raw }),
        }
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn size(self) -> ValueSize {
        match ValueSize::from_key(self.0) {
            Some(size) => size,
            // A Key is only built with valid size bits; the KNOWN table is
            // tested. Eight bytes is the widest, so a value never truncates.
            None => ValueSize::Eight,
        }
    }
}

/// `CFG-NAVSPG-DYNMODEL` values: single-source, ublox
/// `ubx_packets/packets/cfg_nav5.rs:120-129`. The flight controller's choice
/// is the caller's; this crate sets none.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum DynamicModel {
    Portable = 0,
    Stationary = 2,
    Pedestrian = 3,
    Automotive = 4,
    Sea = 5,
    AirborneBelow1g = 6,
    AirborneBelow2g = 7,
    AirborneBelow4g = 8,
}

impl DynamicModel {
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Portable),
            2 => Some(Self::Stationary),
            3 => Some(Self::Pedestrian),
            4 => Some(Self::Automotive),
            5 => Some(Self::Sea),
            6 => Some(Self::AirborneBelow1g),
            7 => Some(Self::AirborneBelow2g),
            8 => Some(Self::AirborneBelow4g),
            _ => None,
        }
    }
}

/// A key and a value that fits it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Setting {
    key: Key,
    value: u64,
}

impl Setting {
    /// Refused when `value` does not fit the key's size.
    pub const fn new(key: Key, value: u64) -> Result<Self, CfgError> {
        if key.size().fits(value) {
            Ok(Self { key, value })
        } else {
            Err(CfgError::ValueTooLarge {
                key: key.raw(),
                value,
            })
        }
    }

    /// A one-bit key set on or off; refused for a key of another size.
    pub const fn flag(key: Key, on: bool) -> Result<Self, CfgError> {
        match key.size() {
            ValueSize::Bit => Ok(Self {
                key,
                value: on as u64,
            }),
            _ => Err(CfgError::NotAFlag { key: key.raw() }),
        }
    }

    pub const fn dynamic_model(model: DynamicModel) -> Self {
        Self {
            key: Key::NAVSPG_DYNMODEL,
            value: model as u64,
        }
    }

    pub const fn key(self) -> Key {
        self.key
    }

    pub const fn value(self) -> u64 {
        self.value
    }
}

/// Layers a `CFG-VALSET` writes, any union of the three: ublox
/// `ubx_packets/packets/cfg_val.rs:137-139`, kinavis-ubx `config.rs:97-101`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Layers(u8);

impl Layers {
    /// Effective at once, lost at power-off.
    pub const RAM: Self = Self(0x01);
    /// Battery-backed RAM.
    pub const BBR: Self = Self(0x02);
    pub const FLASH: Self = Self(0x04);

    pub const fn and(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// The transaction a `CFG-VALSET` belongs to.
///
/// *Inferred*: neither reference crate encodes `CFG-VALSET` version 1, so
/// this layout has no second implementation behind it. With
/// [`Transaction::None`] the message is version 0, as both references write
/// it (ublox `ubx_packets/packets/cfg_val.rs:24-29`, kinavis-ubx
/// `config.rs:255`); otherwise it is version 1 with the action in bits 0-1 of
/// the third byte: 1 begins (or restarts) a transaction, 2 continues it, 3
/// applies it and ends it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Transaction {
    None,
    Begin,
    Continue,
    Apply,
}

impl Transaction {
    const fn header(self) -> (u8, u8) {
        match self {
            Self::None => (0, 0),
            Self::Begin => (1, 1),
            Self::Continue => (1, 2),
            Self::Apply => (1, 3),
        }
    }
}

/// The layer a `CFG-VALGET` reads: ublox
/// `ubx_packets/packets/cfg_val.rs:87-94`, kinavis-ubx `config.rs:124-130`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PollLayer {
    Ram = 0,
    Bbr = 1,
    Flash = 2,
    /// The factory default.
    Default = 7,
}

impl PollLayer {
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Ram),
            1 => Some(Self::Bbr),
            2 => Some(Self::Flash),
            7 => Some(Self::Default),
            _ => None,
        }
    }
}

/// Why a configuration message was not written or not read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CfgError {
    TooManyKeys {
        count: usize,
    },
    /// A `CFG-VALGET` request with no keys.
    NoKeys,
    /// A `CFG-VALSET` naming no layer, which a receiver cannot apply.
    NoLayer,
    ValueTooLarge {
        key: u32,
        value: u64,
    },
    NotAFlag {
        key: u32,
    },
    /// A key whose bits 28-30 name no storage size.
    BadKeySize {
        key: u32,
    },
    /// A response that ends inside a key or a value; `at` is the payload
    /// offset of the pair that does not fit.
    Truncated {
        at: usize,
    },
    /// A response of another version; only version 1 is a response.
    Version {
        found: u8,
    },
    /// A frame that is not a `CFG-VALGET`.
    Message {
        class: u8,
        id: u8,
    },
    Encode(EncodeError),
}

impl fmt::Display for CfgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyKeys { count } => write!(f, "{count} keys, at most {MAX_KEYS}"),
            Self::NoKeys => f.write_str("a CFG-VALGET request needs a key"),
            Self::NoLayer => f.write_str("a CFG-VALSET needs a layer"),
            Self::ValueTooLarge { key, value } => {
                write!(f, "value {value} does not fit key {key:#010X}")
            }
            Self::NotAFlag { key } => write!(f, "key {key:#010X} is not one bit"),
            Self::BadKeySize { key } => write!(f, "key {key:#010X} names no value size"),
            Self::Truncated { at } => write!(f, "CFG-VALGET response ends inside the pair at {at}"),
            Self::Version { found } => write!(f, "CFG-VALGET version {found} is not a response"),
            Self::Message { class, id } => write!(f, "UBX {class:02X}-{id:02X} is not CFG-VALGET"),
            Self::Encode(error) => error.fmt(f),
        }
    }
}

impl core::error::Error for CfgError {}

impl From<EncodeError> for CfgError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

/// Copy `bytes` into `out` from `*at`, advancing it; bytes past the end are
/// dropped, which the callers' length arithmetic rules out.
fn put(out: &mut [u8], at: &mut usize, bytes: &[u8]) {
    for byte in bytes {
        if let Some(slot) = out.get_mut(*at) {
            *slot = *byte;
        }
        *at = at.saturating_add(1);
    }
}

/// Write a `CFG-VALSET` frame setting each of `settings` in `layers`, and
/// return its length.
pub fn valset(
    layers: Layers,
    transaction: Transaction,
    settings: &[Setting],
    out: &mut [u8],
) -> Result<usize, CfgError> {
    if settings.len() > MAX_KEYS {
        return Err(CfgError::TooManyKeys {
            count: settings.len(),
        });
    }
    if layers.0 & 0x07 == 0 {
        return Err(CfgError::NoLayer);
    }
    // At most 4 + 64 * 12 = 772.
    let payload_len = settings.iter().fold(4_usize, |sum, setting| {
        sum.saturating_add(4)
            .saturating_add(setting.key.size().bytes())
    });
    let (version, action) = transaction.header();
    Ok(encode_with(
        CLASS_CFG,
        CFG_VALSET,
        payload_len,
        out,
        |payload| {
            let mut at = 0;
            put(payload, &mut at, &[version, layers.0, action, 0]);
            for setting in settings {
                put(payload, &mut at, &setting.key.raw().to_le_bytes());
                let value = setting.value.to_le_bytes();
                put(
                    payload,
                    &mut at,
                    value.get(..setting.key.size().bytes()).unwrap_or(&[]),
                );
            }
        },
    )?)
}

/// Write a `CFG-VALGET` request (version 0) for `keys` in `layer`, skipping
/// the first `position` pairs of the answer, and return its length: ublox
/// `ubx_packets/packets/cfg_val.rs:48-55`, kinavis-ubx `config.rs:269-303`.
pub fn valget_request(
    layer: PollLayer,
    position: u16,
    keys: &[Key],
    out: &mut [u8],
) -> Result<usize, CfgError> {
    if keys.len() > MAX_KEYS {
        return Err(CfgError::TooManyKeys { count: keys.len() });
    }
    if keys.is_empty() {
        return Err(CfgError::NoKeys);
    }
    let payload_len = keys.len().saturating_mul(4).saturating_add(4);
    let [low, high] = position.to_le_bytes();
    Ok(encode_with(
        CLASS_CFG,
        CFG_VALGET,
        payload_len,
        out,
        |payload| {
            let mut at = 0;
            put(payload, &mut at, &[0, layer as u8, low, high]);
            for key in keys {
                put(payload, &mut at, &key.raw().to_le_bytes());
            }
        },
    )?)
}

/// A `CFG-VALGET` response: version 1, the layer, the position, and
/// key-value pairs. The version check is kinavis-ubx's (`config.rs:351`);
/// ublox reads any version (`ubx_packets/packets/cfg_val.rs:64-77`), but
/// version 0 is the request, so a version-0 frame from the receiver is not an
/// answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValGet<'a> {
    /// Layer as the receiver reports it; see [`PollLayer::from_raw`].
    pub layer: u8,
    pub position: u16,
    pairs: &'a [u8],
}

impl<'a> ValGet<'a> {
    pub fn from_frame(frame: &Frame<'a>) -> Result<Self, CfgError> {
        if (frame.class, frame.id) != (CLASS_CFG, CFG_VALGET) {
            return Err(CfgError::Message {
                class: frame.class,
                id: frame.id,
            });
        }
        Self::parse(frame.payload)
    }

    /// Read the response header. The pairs are checked as
    /// [`Self::values`] reaches them.
    pub fn parse(payload: &'a [u8]) -> Result<Self, CfgError> {
        let Some((header, pairs)) = payload.split_first_chunk::<4>() else {
            return Err(CfgError::Truncated { at: 0 });
        };
        let [version, layer, low, high] = *header;
        if version != 1 {
            return Err(CfgError::Version { found: version });
        }
        Ok(Self {
            layer,
            position: u16::from_le_bytes([low, high]),
            pairs,
        })
    }

    /// The pairs in order, each value little-endian and zero-extended. A
    /// one-bit key's value must be 0 or 1. The first error ends the
    /// iteration; no pair after it is trusted.
    pub fn values(&self) -> Values<'a> {
        Values {
            rest: self.pairs,
            at: 4,
        }
    }

    /// The value of `key`, if the response carries it. An error before the
    /// key is reached is returned, not skipped.
    pub fn get(&self, key: Key) -> Result<Option<u64>, CfgError> {
        for pair in self.values() {
            let (found, value) = pair?;
            if found == key {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }
}

/// The pairs of a [`ValGet`]: at most one per four payload bytes.
#[derive(Clone, Debug)]
pub struct Values<'a> {
    rest: &'a [u8],
    at: usize,
}

impl Iterator for Values<'_> {
    type Item = Result<(Key, u64), CfgError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let at = self.at;
        let fail = |this: &mut Self, error| {
            this.rest = &[];
            Some(Err(error))
        };
        let Some((raw, after_key)) = self.rest.split_first_chunk::<4>() else {
            return fail(self, CfgError::Truncated { at });
        };
        let key = match Key::new(u32::from_le_bytes(*raw)) {
            Ok(key) => key,
            Err(error) => return fail(self, error),
        };
        let size = key.size().bytes();
        let Some((bytes, rest)) = after_key.split_at_checked(size) else {
            return fail(self, CfgError::Truncated { at });
        };
        let mut value = [0_u8; 8];
        for (slot, byte) in value.iter_mut().zip(bytes) {
            *slot = *byte;
        }
        let value = u64::from_le_bytes(value);
        if !key.size().fits(value) {
            // Only a one-bit key can carry a value its size does not allow.
            return fail(
                self,
                CfgError::ValueTooLarge {
                    key: key.raw(),
                    value,
                },
            );
        }
        self.rest = rest;
        self.at = at.saturating_add(4).saturating_add(size);
        Some(Ok((key, value)))
    }
}

/// A request waiting for its acknowledgement, identified by the class and
/// identifier an `ACK-ACK` or `ACK-NAK` echoes: ublox
/// `ubx_packets/packets/ack.rs:13-36`, kinavis-ubx `message.rs:125-137`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Pending {
    pub class: u8,
    pub id: u8,
}

/// How a frame bears on a [`Pending`] request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Answer {
    /// Not an acknowledgement of this request: another message, or one
    /// acknowledging another request.
    Unrelated,
    /// `ACK-ACK`: the receiver accepted the request.
    Accepted,
    /// `ACK-NAK`: the receiver refused it.
    Refused,
    /// An `ACK-ACK` or `ACK-NAK` whose payload is not two bytes; it answers
    /// nothing reliably.
    Malformed,
}

impl Pending {
    pub const VALSET: Self = Self {
        class: CLASS_CFG,
        id: CFG_VALSET,
    };
    pub const VALGET: Self = Self {
        class: CLASS_CFG,
        id: CFG_VALGET,
    };

    pub fn answer(&self, frame: &Frame<'_>) -> Answer {
        if frame.class != CLASS_ACK || !matches!(frame.id, ACK_ACK | ACK_NAK) {
            return Answer::Unrelated;
        }
        let Ok([class, id]) = <[u8; 2]>::try_from(frame.payload) else {
            return Answer::Malformed;
        };
        if (class, id) != (self.class, self.id) {
            Answer::Unrelated
        } else if frame.id == ACK_ACK {
            Answer::Accepted
        } else {
            Answer::Refused
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Deframer, Event, checksum};

    fn payload_of(frame: &[u8]) -> &[u8] {
        &frame[6..frame.len() - 2]
    }

    #[test]
    fn every_named_key_has_valid_size_bits() {
        for key in Key::KNOWN {
            assert_eq!(Key::new(key.raw()), Ok(key));
        }
        assert_eq!(Key::RATE_MEAS.size(), ValueSize::Two);
        assert_eq!(Key::UART1_BAUDRATE.size(), ValueSize::Four);
        assert_eq!(Key::UART1OUTPROT_NMEA.size(), ValueSize::Bit);
        assert_eq!(Key::NAVSPG_DYNMODEL.size(), ValueSize::Byte);
    }

    #[test]
    fn size_bits_naming_no_size_are_refused() {
        for bits in [0_u32, 6, 7] {
            let raw = (bits << 28) | 0x0021_0001;
            assert_eq!(Key::new(raw), Err(CfgError::BadKeySize { key: raw }));
        }
    }

    #[test]
    fn valset_encodes_each_key_size() {
        let eight = Key::new(0x5011_0062).unwrap();
        let settings = [
            Setting::flag(Key::UART1OUTPROT_NMEA, false).unwrap(),
            Setting::dynamic_model(DynamicModel::AirborneBelow4g),
            Setting::new(Key::RATE_MEAS, 100).unwrap(),
            Setting::new(Key::UART1_BAUDRATE, 230_400).unwrap(),
            Setting::new(eight, 0x0102_0304_0506_0708).unwrap(),
        ];
        let mut out = [0_u8; 64];
        let len = valset(
            Layers::RAM.and(Layers::BBR),
            Transaction::None,
            &settings,
            &mut out,
        )
        .unwrap();
        let payload = payload_of(&out[..len]);
        assert_eq!(
            payload,
            &[
                0, 3, 0, 0, //
                0x02, 0x00, 0x74, 0x10, 0, //
                0x21, 0x00, 0x11, 0x20, 8, //
                0x01, 0x00, 0x21, 0x30, 100, 0, //
                0x01, 0x00, 0x52, 0x40, 0x00, 0x84, 0x03, 0x00, //
                0x62, 0x00, 0x11, 0x50, 8, 7, 6, 5, 4, 3, 2, 1,
            ]
        );
        assert_eq!(&out[..6], &[0xB5, 0x62, 0x06, 0x8A, payload.len() as u8, 0]);
        assert_eq!(checksum(&out[2..len - 2]), [out[len - 2], out[len - 1]]);
    }

    #[test]
    fn valset_refuses_what_a_receiver_could_not_apply() {
        let mut out = [0_u8; 1024];
        assert_eq!(
            Setting::new(Key::RATE_MEAS, 0x1_0000),
            Err(CfgError::ValueTooLarge {
                key: 0x3021_0001,
                value: 0x1_0000
            })
        );
        assert_eq!(
            Setting::new(Key::UART1OUTPROT_UBX, 2),
            Err(CfgError::ValueTooLarge {
                key: 0x1074_0001,
                value: 2
            })
        );
        assert_eq!(
            Setting::flag(Key::RATE_MEAS, true),
            Err(CfgError::NotAFlag { key: 0x3021_0001 })
        );
        let one = Setting::new(Key::RATE_MEAS, 100).unwrap();
        assert_eq!(
            valset(Layers(0), Transaction::None, &[one], &mut out),
            Err(CfgError::NoLayer)
        );
        assert_eq!(
            valset(Layers::RAM, Transaction::None, &[one; 65], &mut out),
            Err(CfgError::TooManyKeys { count: 65 })
        );
        assert_eq!(
            valset(Layers::RAM, Transaction::None, &[one], &mut out[..17]),
            Err(CfgError::Encode(EncodeError::BufferTooSmall { needed: 18 }))
        );
        assert_eq!(
            valset(Layers::RAM, Transaction::None, &[one; 64], &mut out),
            Ok(8 + 4 + 64 * 6)
        );
    }

    #[test]
    fn valset_transactions_set_version_and_action() {
        let one = Setting::new(Key::RATE_MEAS, 100).unwrap();
        let mut out = [0_u8; 32];
        for (transaction, header) in [
            (Transaction::None, [0, 1, 0, 0]),
            (Transaction::Begin, [1, 1, 1, 0]),
            (Transaction::Continue, [1, 1, 2, 0]),
            (Transaction::Apply, [1, 1, 3, 0]),
        ] {
            let len = valset(Layers::RAM, transaction, &[one], &mut out).unwrap();
            assert_eq!(&payload_of(&out[..len])[..4], &header);
        }
        let len = valset(Layers::RAM, Transaction::Apply, &[], &mut out).unwrap();
        assert_eq!(payload_of(&out[..len]), &[1, 1, 3, 0]);
    }

    #[test]
    fn valget_request_lists_its_keys() {
        let mut out = [0_u8; 32];
        let len = valget_request(
            PollLayer::Flash,
            2,
            &[Key::RATE_MEAS, Key::NAVSPG_DYNMODEL],
            &mut out,
        )
        .unwrap();
        assert_eq!(
            payload_of(&out[..len]),
            &[0, 2, 2, 0, 0x01, 0x00, 0x21, 0x30, 0x21, 0x00, 0x11, 0x20]
        );
        assert_eq!(
            valget_request(PollLayer::Ram, 0, &[], &mut out),
            Err(CfgError::NoKeys)
        );
    }

    fn response(pairs: &[u8]) -> ([u8; 64], usize) {
        let mut payload = [0_u8; 60];
        payload[..4].copy_from_slice(&[1, 0, 0, 0]);
        payload[4..4 + pairs.len()].copy_from_slice(pairs);
        let mut out = [0_u8; 64];
        let len =
            crate::encode(CLASS_CFG, CFG_VALGET, &payload[..4 + pairs.len()], &mut out).unwrap();
        (out, len)
    }

    #[test]
    fn valget_response_reads_each_size() {
        let pairs = [
            0x02, 0x00, 0x74, 0x10, 1, //
            0x21, 0x00, 0x11, 0x20, 8, //
            0x01, 0x00, 0x21, 0x30, 0xE8, 0x03, //
            0x01, 0x00, 0x52, 0x40, 0x00, 0xC2, 0x01, 0x00,
        ];
        let (bytes, len) = response(&pairs);
        let mut deframer = Deframer::<128>::new();
        let mut checked = false;
        for byte in &bytes[..len] {
            if let Some(Event::Frame(frame)) = deframer.push(*byte) {
                let valget = ValGet::from_frame(&frame).unwrap();
                let mut values = valget.values();
                assert_eq!(values.next(), Some(Ok((Key::UART1OUTPROT_NMEA, 1))));
                assert_eq!(values.next(), Some(Ok((Key::NAVSPG_DYNMODEL, 8))));
                assert_eq!(values.next(), Some(Ok((Key::RATE_MEAS, 1000))));
                assert_eq!(values.next(), Some(Ok((Key::UART1_BAUDRATE, 115_200))));
                assert_eq!(values.next(), None);
                assert_eq!(valget.get(Key::RATE_MEAS), Ok(Some(1000)));
                assert_eq!(valget.get(Key::RATE_NAV), Ok(None));
                checked = true;
            }
        }
        assert!(checked);
    }

    #[test]
    fn valget_response_truncated_inside_a_value_or_key_is_an_error() {
        let full = [
            0x01, 0x00, 0x21, 0x30, 0xE8, 0x03, 0x01, 0x00, 0x52, 0x40, 0x00, 0xC2,
        ];
        let mut payload = [0_u8; 16];
        payload[0] = 1;
        payload[4..].copy_from_slice(&full);
        let valget = ValGet::parse(&payload).unwrap();
        let mut values = valget.values();
        assert_eq!(values.next(), Some(Ok((Key::RATE_MEAS, 1000))));
        assert_eq!(values.next(), Some(Err(CfgError::Truncated { at: 10 })));
        assert_eq!(values.next(), None);
        assert_eq!(
            valget.get(Key::UART1_BAUDRATE),
            Err(CfgError::Truncated { at: 10 })
        );
        // Two bytes of a key.
        assert_eq!(
            ValGet::parse(&payload[..6]).unwrap().values().next(),
            Some(Err(CfgError::Truncated { at: 4 }))
        );
        assert_eq!(
            ValGet::parse(&payload[..3]),
            Err(CfgError::Truncated { at: 0 })
        );
    }

    #[test]
    fn valget_response_with_an_unknown_key_size_stops_at_it() {
        let payload = [1, 0, 0, 0, 0x01, 0x00, 0x21, 0x70, 0, 0, 0, 0];
        let valget = ValGet::parse(&payload).unwrap();
        let mut values = valget.values();
        assert_eq!(
            values.next(),
            Some(Err(CfgError::BadKeySize { key: 0x7021_0001 }))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn a_one_bit_value_other_than_0_or_1_is_an_error() {
        let payload = [1, 0, 0, 0, 0x02, 0x00, 0x74, 0x10, 2];
        assert_eq!(
            ValGet::parse(&payload).unwrap().values().next(),
            Some(Err(CfgError::ValueTooLarge {
                key: 0x1074_0002,
                value: 2
            }))
        );
    }

    #[test]
    fn a_request_is_not_a_response() {
        assert_eq!(
            ValGet::parse(&[0, 0, 0, 0, 1, 0, 0x21, 0x30]),
            Err(CfgError::Version { found: 0 })
        );
        let frame = Frame {
            class: CLASS_CFG,
            id: CFG_VALSET,
            payload: &[1, 0, 0, 0],
        };
        assert_eq!(
            ValGet::from_frame(&frame),
            Err(CfgError::Message {
                class: CLASS_CFG,
                id: CFG_VALSET
            })
        );
    }

    #[test]
    fn acknowledgements_match_the_request_they_echo() {
        let ack = |id, payload: &'static [u8]| Frame {
            class: CLASS_ACK,
            id,
            payload,
        };
        assert_eq!(
            Pending::VALSET.answer(&ack(ACK_ACK, &[0x06, 0x8A])),
            Answer::Accepted
        );
        assert_eq!(
            Pending::VALSET.answer(&ack(ACK_NAK, &[0x06, 0x8A])),
            Answer::Refused
        );
        assert_eq!(
            Pending::VALGET.answer(&ack(ACK_ACK, &[0x06, 0x8A])),
            Answer::Unrelated
        );
        assert_eq!(
            Pending::VALGET.answer(&ack(ACK_NAK, &[0x06, 0x8B])),
            Answer::Refused
        );
        assert_eq!(
            Pending::VALSET.answer(&ack(ACK_ACK, &[0x06])),
            Answer::Malformed
        );
        assert_eq!(
            Pending::VALSET.answer(&ack(ACK_ACK, &[0x06, 0x8A, 0])),
            Answer::Malformed
        );
        assert_eq!(
            Pending::VALSET.answer(&ack(0x02, &[0x06, 0x8A])),
            Answer::Unrelated
        );
        let other = Frame {
            class: CLASS_CFG,
            id: CFG_VALSET,
            payload: &[0x05, 0x01],
        };
        assert_eq!(Pending::VALSET.answer(&other), Answer::Unrelated);
    }
}
