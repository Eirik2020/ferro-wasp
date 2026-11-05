//! `NAV-PVT`, the navigation solution: a fixed 92-byte payload.
//!
//! Offsets agree in both references. ublox declares the fields in order in
//! `ubx_packets/packets/nav_pvt/proto27.rs:27-140` (identical in `proto33.rs`),
//! so an offset is the sum of the sizes before it; kinavis-ubx reads each at
//! an explicit offset in `nav.rs:243-294`. Each field below cites both;
//! ublox's `proto27.rs`, `proto31.rs` and `nav_pvt/common.rs` are under its
//! `ubx_packets/packets/`, and kinavis-ubx's `nav.rs` under its `src/`.
//! Values stay integers in the protocol's units, named in the field.

use core::fmt;

use crate::frame::Frame;
use crate::id::{CLASS_NAV, NAV_PVT};

/// The payload length, the only one accepted: ublox `proto27.rs:17`
/// (`fixed_payload_len = 92`), kinavis-ubx `nav.rs:244` (which also takes the
/// pre-protocol-15 84-byte form; an M10 sends 92).
pub const NAV_PVT_LEN: usize = 92;

/// Why a payload is not a `NAV-PVT`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PvtError {
    Length {
        found: usize,
    },
    /// A frame of another message.
    Message {
        class: u8,
        id: u8,
    },
}

impl fmt::Display for PvtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length { found } => write!(f, "NAV-PVT is {NAV_PVT_LEN} bytes, not {found}"),
            Self::Message { class, id } => write!(f, "UBX {class:02X}-{id:02X} is not NAV-PVT"),
        }
    }
}

impl core::error::Error for PvtError {}

/// `fixType`, offset 20: ublox `ubx_packets/packets.rs:231-238`, kinavis-ubx
/// `nav.rs:171-177`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FixType {
    NoFix,
    DeadReckoningOnly,
    Fix2D,
    Fix3D,
    GnssDeadReckoning,
    TimeOnly,
    /// A value the protocol does not define.
    Other(u8),
}

impl FixType {
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::NoFix,
            1 => Self::DeadReckoningOnly,
            2 => Self::Fix2D,
            3 => Self::Fix3D,
            4 => Self::GnssDeadReckoning,
            5 => Self::TimeOnly,
            other => Self::Other(other),
        }
    }
}

/// `carrSoln`, bits 6-7 of `flags`: ublox `nav_pvt/common.rs:17,19`,
/// kinavis-ubx `nav.rs:270`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CarrierSolution {
    None,
    Float,
    Fixed,
    /// The reserved fourth value.
    Reserved,
}

/// `flags`, offset 21.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PvtFlags(pub u8);

impl PvtFlags {
    /// `gnssFixOK`, bit 0: the fix is within the receiver's DOP and accuracy
    /// masks. ublox `nav_pvt/common.rs:11`, kinavis-ubx `nav.rs:268`.
    pub const fn gnss_fix_ok(self) -> bool {
        self.0 & 0x01 != 0
    }

    /// `diffSoln`, bit 1: ublox `nav_pvt/common.rs:13`, kinavis-ubx
    /// `nav.rs:269`.
    pub const fn diff_soln(self) -> bool {
        self.0 & 0x02 != 0
    }

    /// `headVehValid`, bit 5: ublox `nav_pvt/common.rs:15`, kinavis-ubx
    /// `nav.rs:250`.
    pub const fn head_veh_valid(self) -> bool {
        self.0 & 0x20 != 0
    }

    pub const fn carrier_solution(self) -> CarrierSolution {
        match self.0 >> 6 {
            0 => CarrierSolution::None,
            1 => CarrierSolution::Float,
            2 => CarrierSolution::Fixed,
            _ => CarrierSolution::Reserved,
        }
    }
}

/// What the receiver says about its UTC date and time.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TimeValidity {
    /// `valid` bit 0: ublox `nav_pvt/common.rs:49`, kinavis-ubx `nav.rs:264`.
    pub date: bool,
    /// `valid` bit 1: ublox `nav_pvt/common.rs:51`, kinavis-ubx `nav.rs:265`.
    pub time: bool,
    /// `valid` bit 2, `fullyResolved`: single-source, ublox
    /// `nav_pvt/common.rs:54`.
    pub fully_resolved: bool,
    /// `flags2` bit 5, `confirmedAvai`: single-source, ublox
    /// `nav_pvt/common.rs:32`.
    pub confirmed_available: bool,
    /// `flags2` bit 6: single-source, ublox `nav_pvt/common.rs:35`.
    pub confirmed_date: bool,
    /// `flags2` bit 7: single-source, ublox `nav_pvt/common.rs:38`.
    pub confirmed_time: bool,
}

/// UTC as the receiver writes it, without checking it forms a date.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct UtcTime {
    /// Offset 4: ublox `proto27.rs:30`, kinavis-ubx `nav.rs:262`.
    pub year: u16,
    /// Offset 6: ublox `proto27.rs:32`, kinavis-ubx `nav.rs:262`.
    pub month: u8,
    /// Offset 7: ublox `proto27.rs:34`, kinavis-ubx `nav.rs:262`.
    pub day: u8,
    /// Offset 8: ublox `proto27.rs:36`, kinavis-ubx `nav.rs:262`.
    pub hour: u8,
    /// Offset 9: ublox `proto27.rs:38`, kinavis-ubx `nav.rs:262`.
    pub minute: u8,
    /// Offset 10: ublox `proto27.rs:40`, kinavis-ubx `nav.rs:262`.
    pub second: u8,
    /// `nano`, offset 16, -1e9..1e9: ublox `proto27.rs:50`, kinavis-ubx
    /// `nav.rs:263`.
    pub nano_ns: i32,
    /// `tAcc`, offset 12: ublox `proto27.rs:47`, kinavis-ubx `nav.rs:266`.
    pub accuracy_ns: u32,
}

/// Position, velocity and their accuracy, as the receiver reports them.
/// Reached checked through [`NavPvt::fix`], or unchecked for logging through
/// [`NavPvt::solution_unchecked`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Solution {
    /// `lon`, offset 24: ublox `proto27.rs:69`, kinavis-ubx `nav.rs:249`.
    pub lon_e7_deg: i32,
    /// `lat`, offset 28: ublox `proto27.rs:73`, kinavis-ubx `nav.rs:248`.
    pub lat_e7_deg: i32,
    /// `height` above the ellipsoid, offset 32: ublox `proto27.rs:77`,
    /// kinavis-ubx `nav.rs:273`.
    pub height_mm: i32,
    /// `hMSL`, offset 36: ublox `proto27.rs:81`, kinavis-ubx `nav.rs:274`.
    pub height_msl_mm: i32,
    /// `hAcc`, offset 40: ublox `proto27.rs:85`, kinavis-ubx `nav.rs:275`.
    pub h_acc_mm: u32,
    /// `vAcc`, offset 44: ublox `proto27.rs:89`, kinavis-ubx `nav.rs:276`.
    pub v_acc_mm: u32,
    /// `velN`, offset 48: ublox `proto27.rs:93`, kinavis-ubx `nav.rs:278`.
    pub vel_n_mm_s: i32,
    /// `velE`, offset 52: ublox `proto27.rs:97`, kinavis-ubx `nav.rs:279`.
    pub vel_e_mm_s: i32,
    /// `velD`, offset 56: ublox `proto27.rs:101`, kinavis-ubx `nav.rs:280`.
    pub vel_d_mm_s: i32,
    /// `gSpeed`, offset 60: ublox `proto27.rs:105`, kinavis-ubx `nav.rs:282`.
    pub g_speed_mm_s: i32,
    /// `headMot`, offset 64: ublox `proto27.rs:109`, kinavis-ubx
    /// `nav.rs:283`.
    pub head_mot_e5_deg: i32,
    /// `sAcc`, offset 68: ublox `proto27.rs:113`, kinavis-ubx `nav.rs:285`.
    pub s_acc_mm_s: u32,
    /// `headAcc`, offset 72: ublox `proto27.rs:117`, kinavis-ubx
    /// `nav.rs:287`.
    pub head_acc_e5_deg: u32,
    /// `pDOP`, offset 76: ublox `proto27.rs:121`, kinavis-ubx `nav.rs:289`.
    pub p_dop_e2: u16,
    /// `headVeh`, offset 84, meaningful only with
    /// [`PvtFlags::head_veh_valid`]: ublox `proto27.rs:132`, kinavis-ubx
    /// `nav.rs:251`.
    pub head_veh_e5_deg: i32,
    /// `magDec`, offset 88: ublox `proto27.rs:136`, kinavis-ubx `nav.rs:256`.
    pub mag_dec_e2_deg: i16,
    /// `magAcc`, offset 90: single-source, ublox `proto27.rs:140`.
    pub mag_acc_e2_deg: u16,
}

/// A valid 3D fix: what [`NavPvt::fix`] returns.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Fix {
    pub fix_type: FixType,
    pub num_sv: u8,
    pub solution: Solution,
}

/// A decoded `NAV-PVT`.
///
/// The position and velocity fields are private: [`Self::fix`] returns them
/// only when the receiver reports a 3D fix (`fixType` 3, or 4 with dead
/// reckoning), sets `gnssFixOK`, and does not set `invalidLlh`. A 2D fix, a
/// dead-reckoning-only or time-only solution, and no fix all read as `None`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NavPvt {
    /// `iTOW`, GPS time of week of the epoch, offset 0: ublox `proto27.rs:27`,
    /// kinavis-ubx `nav.rs:261`.
    pub itow_ms: u32,
    /// Offset 20.
    pub fix_type: FixType,
    /// Offset 21: ublox `proto27.rs:58`, kinavis-ubx `nav.rs:246`.
    pub flags: PvtFlags,
    /// `numSV`, offset 23: ublox `proto27.rs:65`, kinavis-ubx `nav.rs:271`.
    pub num_sv: u8,
    /// `flags3` bit 0, offset 78: ublox `proto27.rs:125` with
    /// `proto31.rs:165`, kinavis-ubx `nav.rs:290`.
    pub invalid_llh: bool,
    /// `valid` (offset 11: ublox `proto27.rs:44`, kinavis-ubx `nav.rs:245`)
    /// and `flags2` (offset 22: single-source, ublox `proto27.rs:62`).
    pub time_validity: TimeValidity,
    /// `valid` bit 3, `validMag`: ublox `nav_pvt/common.rs:56`, kinavis-ubx
    /// `nav.rs:255`.
    pub valid_mag: bool,
    time: UtcTime,
    solution: Solution,
}

fn array<const N: usize>(payload: &[u8; NAV_PVT_LEN], at: usize) -> [u8; N] {
    payload
        .get(at..at.saturating_add(N))
        .and_then(|bytes| <[u8; N]>::try_from(bytes).ok())
        .unwrap_or([0; N])
}

fn u8_at(payload: &[u8; NAV_PVT_LEN], at: usize) -> u8 {
    payload.get(at).copied().unwrap_or(0)
}

fn u16_at(payload: &[u8; NAV_PVT_LEN], at: usize) -> u16 {
    u16::from_le_bytes(array(payload, at))
}

fn i16_at(payload: &[u8; NAV_PVT_LEN], at: usize) -> i16 {
    i16::from_le_bytes(array(payload, at))
}

fn u32_at(payload: &[u8; NAV_PVT_LEN], at: usize) -> u32 {
    u32::from_le_bytes(array(payload, at))
}

fn i32_at(payload: &[u8; NAV_PVT_LEN], at: usize) -> i32 {
    i32::from_le_bytes(array(payload, at))
}

impl NavPvt {
    pub fn from_frame(frame: &Frame<'_>) -> Result<Self, PvtError> {
        if (frame.class, frame.id) != (CLASS_NAV, NAV_PVT) {
            return Err(PvtError::Message {
                class: frame.class,
                id: frame.id,
            });
        }
        Self::decode(frame.payload)
    }

    /// Decode a payload of exactly [`NAV_PVT_LEN`] bytes.
    pub fn decode(payload: &[u8]) -> Result<Self, PvtError> {
        let p: &[u8; NAV_PVT_LEN] = payload.try_into().map_err(|_| PvtError::Length {
            found: payload.len(),
        })?;
        let valid = u8_at(p, 11);
        let flags2 = u8_at(p, 22);
        Ok(Self {
            itow_ms: u32_at(p, 0),
            fix_type: FixType::from_raw(u8_at(p, 20)),
            flags: PvtFlags(u8_at(p, 21)),
            num_sv: u8_at(p, 23),
            invalid_llh: u8_at(p, 78) & 0x01 != 0,
            time_validity: TimeValidity {
                date: valid & 0x01 != 0,
                time: valid & 0x02 != 0,
                fully_resolved: valid & 0x04 != 0,
                confirmed_available: flags2 & 0x20 != 0,
                confirmed_date: flags2 & 0x40 != 0,
                confirmed_time: flags2 & 0x80 != 0,
            },
            valid_mag: valid & 0x08 != 0,
            time: UtcTime {
                year: u16_at(p, 4),
                month: u8_at(p, 6),
                day: u8_at(p, 7),
                hour: u8_at(p, 8),
                minute: u8_at(p, 9),
                second: u8_at(p, 10),
                accuracy_ns: u32_at(p, 12),
                nano_ns: i32_at(p, 16),
            },
            solution: Solution {
                lon_e7_deg: i32_at(p, 24),
                lat_e7_deg: i32_at(p, 28),
                height_mm: i32_at(p, 32),
                height_msl_mm: i32_at(p, 36),
                h_acc_mm: u32_at(p, 40),
                v_acc_mm: u32_at(p, 44),
                vel_n_mm_s: i32_at(p, 48),
                vel_e_mm_s: i32_at(p, 52),
                vel_d_mm_s: i32_at(p, 56),
                g_speed_mm_s: i32_at(p, 60),
                head_mot_e5_deg: i32_at(p, 64),
                s_acc_mm_s: u32_at(p, 68),
                head_acc_e5_deg: u32_at(p, 72),
                p_dop_e2: u16_at(p, 76),
                head_veh_e5_deg: i32_at(p, 84),
                mag_dec_e2_deg: i16_at(p, 88),
                mag_acc_e2_deg: u16_at(p, 90),
            },
        })
    }

    /// The solution, only for a valid 3D fix.
    pub fn fix(&self) -> Option<Fix> {
        let three_d = matches!(self.fix_type, FixType::Fix3D | FixType::GnssDeadReckoning);
        (three_d && self.flags.gnss_fix_ok() && !self.invalid_llh).then_some(Fix {
            fix_type: self.fix_type,
            num_sv: self.num_sv,
            solution: self.solution,
        })
    }

    /// UTC, only when the receiver marks both date and time valid.
    pub fn utc(&self) -> Option<UtcTime> {
        (self.time_validity.date && self.time_validity.time).then_some(self.time)
    }

    /// The solution whatever the fix: for logging, never for navigation.
    pub const fn solution_unchecked(&self) -> &Solution {
        &self.solution
    }

    /// UTC whatever its validity: for logging.
    pub const fn utc_unchecked(&self) -> &UtcTime {
        &self.time
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A payload with a distinct value in every field, so a wrong offset
    /// reads a wrong value. Positions from kinavis-ubx's reference table,
    /// `tests/data/ubx_reference.txt:42` (navisys-gr8013.log, first NAV-PVT).
    pub(crate) fn sample() -> [u8; NAV_PVT_LEN] {
        let mut p = [0_u8; NAV_PVT_LEN];
        let mut put = |at: usize, bytes: &[u8]| p[at..at + bytes.len()].copy_from_slice(bytes);
        put(0, &403_075_000_u32.to_le_bytes());
        put(4, &2026_u16.to_le_bytes());
        put(6, &[11, 4, 13, 37, 59, 0x07]);
        put(12, &25_u32.to_le_bytes());
        put(16, &(-123_456_i32).to_le_bytes());
        put(20, &[3, 0x01 | 0x02, 0x20 | 0x40, 10]);
        put(24, &(-1_170_207_302_i32).to_le_bytes());
        put(28, &464_045_956_i32.to_le_bytes());
        put(32, &247_522_i32.to_le_bytes());
        put(36, &265_694_i32.to_le_bytes());
        put(40, &7_915_u32.to_le_bytes());
        put(44, &11_179_u32.to_le_bytes());
        put(48, &46_i32.to_le_bytes());
        put(52, &249_i32.to_le_bytes());
        put(56, &(-69_i32).to_le_bytes());
        put(60, &253_i32.to_le_bytes());
        put(64, &6_753_761_i32.to_le_bytes());
        put(68, &321_u32.to_le_bytes());
        put(72, &1_234_567_u32.to_le_bytes());
        put(76, &230_u16.to_le_bytes());
        put(78, &[0x00]);
        put(84, &(-2_000_000_i32).to_le_bytes());
        put(88, &(-345_i16).to_le_bytes());
        put(90, &77_u16.to_le_bytes());
        p
    }

    #[test]
    fn every_field_decodes_from_its_offset() {
        let pvt = NavPvt::decode(&sample()).unwrap();
        assert_eq!(pvt.itow_ms, 403_075_000);
        assert_eq!(pvt.fix_type, FixType::Fix3D);
        assert!(pvt.flags.gnss_fix_ok() && pvt.flags.diff_soln());
        assert_eq!(pvt.flags.carrier_solution(), CarrierSolution::None);
        assert_eq!(pvt.num_sv, 10);
        assert_eq!(
            pvt.time_validity,
            TimeValidity {
                date: true,
                time: true,
                fully_resolved: true,
                confirmed_available: true,
                confirmed_date: true,
                confirmed_time: false,
            }
        );
        assert!(!pvt.valid_mag);
        assert_eq!(
            pvt.utc(),
            Some(UtcTime {
                year: 2026,
                month: 11,
                day: 4,
                hour: 13,
                minute: 37,
                second: 59,
                nano_ns: -123_456,
                accuracy_ns: 25,
            })
        );
        let fix = pvt.fix().unwrap();
        assert_eq!(fix.num_sv, 10);
        assert_eq!(
            fix.solution,
            Solution {
                lon_e7_deg: -1_170_207_302,
                lat_e7_deg: 464_045_956,
                height_mm: 247_522,
                height_msl_mm: 265_694,
                h_acc_mm: 7_915,
                v_acc_mm: 11_179,
                vel_n_mm_s: 46,
                vel_e_mm_s: 249,
                vel_d_mm_s: -69,
                g_speed_mm_s: 253,
                head_mot_e5_deg: 6_753_761,
                s_acc_mm_s: 321,
                head_acc_e5_deg: 1_234_567,
                p_dop_e2: 230,
                head_veh_e5_deg: -2_000_000,
                mag_dec_e2_deg: -345,
                mag_acc_e2_deg: 77,
            }
        );
    }

    #[test]
    fn any_length_but_92_is_refused() {
        let p = sample();
        for len in [0, 84, 91] {
            assert_eq!(
                NavPvt::decode(&p[..len]),
                Err(PvtError::Length { found: len })
            );
        }
        let mut long = [0_u8; 93];
        long[..92].copy_from_slice(&p);
        assert_eq!(NavPvt::decode(&long), Err(PvtError::Length { found: 93 }));
    }

    #[test]
    fn no_fix_reads_as_no_position() {
        let base = sample();
        let cases: [(usize, u8, &str); 6] = [
            (20, 0, "no fix"),
            (20, 1, "dead reckoning only"),
            (20, 2, "2D"),
            (20, 5, "time only"),
            (21, 0x02, "gnssFixOK clear"),
            (78, 0x01, "invalidLlh"),
        ];
        for (at, value, what) in cases {
            let mut p = base;
            p[at] = value;
            let pvt = NavPvt::decode(&p).unwrap();
            assert_eq!(pvt.fix(), None, "{what}");
            // The raw solution is still there for a log.
            assert_eq!(pvt.solution_unchecked().lat_e7_deg, 464_045_956);
        }
        let mut p = base;
        p[20] = 4;
        assert!(
            NavPvt::decode(&p).unwrap().fix().is_some(),
            "GNSS + dead reckoning"
        );
    }

    #[test]
    fn invalid_utc_reads_as_no_time() {
        for valid in [0x00, 0x01, 0x02, 0x04] {
            let mut p = sample();
            p[11] = valid;
            assert_eq!(NavPvt::decode(&p).unwrap().utc(), None);
        }
    }

    #[test]
    fn a_frame_of_another_message_is_refused() {
        let p = sample();
        let frame = Frame {
            class: 0x01,
            id: 0x03,
            payload: &p,
        };
        assert_eq!(
            NavPvt::from_frame(&frame),
            Err(PvtError::Message { class: 1, id: 3 })
        );
    }
}
