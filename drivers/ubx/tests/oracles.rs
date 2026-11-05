//! Cross-checks against two independent UBX implementations, ublox 0.10.0
//! and kinavis-ubx 0.1.1: the key IDs and values this crate names, frames
//! each side builds parsing on the other, and decodings agreeing field by
//! field. Where a reference disagrees, the test says which is right.

use std::panic;

use kinavis_ubx::{ConfigKey, Message, PvtFix, config as kv};
use ublox::cfg_val::{CfgKey, CfgVal};
use ublox::packets::cfg_nav5::NavDynamicModel;
use ublox::packets::cfg_val::{
    CfgLayerGet, CfgLayerSet, CfgValGetRequestBuilder, CfgValSetBuilder,
};
use ublox::proto27::{PacketRef, Proto27};
use ublox::{ParserBuilder, UbxPacket};
use ubx::cfg::{
    DynamicModel, Key, Layers, PollLayer, Setting, Transaction, ValGet, valget_request, valset,
};
use ubx::{Deframer, Event, NavPvt, encode, id};

/// Every frame our deframer finds in `bytes`, as (class, id, payload).
fn deframe(bytes: &[u8]) -> Vec<(u8, u8, Vec<u8>)> {
    let mut deframer = Deframer::<1024>::new();
    let mut frames = Vec::new();
    for byte in bytes {
        let mut event = deframer.push(*byte).map(owned);
        while let Some(found) = event {
            if let Some(frame) = found {
                frames.push(frame);
            }
            event = deframer.poll().map(owned);
        }
    }
    frames
}

fn owned(event: Event<'_>) -> Option<(u8, u8, Vec<u8>)> {
    match event {
        Event::Frame(frame) => Some((frame.class, frame.id, frame.payload.to_vec())),
        Event::Error(_) => None,
    }
}

/// The frames kinavis-ubx's deframer finds in `bytes`.
fn kinavis_deframe(bytes: &[u8]) -> Vec<(u8, u8, Vec<u8>)> {
    let mut deframer = kinavis_ubx::Deframer::<1024>::new();
    let mut frames = Vec::new();
    for byte in bytes {
        if let Some(kinavis_ubx::Event::Frame(frame)) = deframer.push(*byte) {
            frames.push((frame.class, frame.id, frame.payload.to_vec()));
        }
    }
    frames
}

fn ours(settings: &[Setting]) -> Vec<u8> {
    let mut out = [0_u8; 1024];
    let len = valset(
        Layers::RAM.and(Layers::BBR),
        Transaction::None,
        settings,
        &mut out,
    )
    .unwrap();
    out[..len].to_vec()
}

#[test]
fn key_ids_both_references_carry_agree_with_ours() {
    // kinavis-ubx config.rs:32-46.
    let kinavis = [
        (Key::RATE_MEAS, ConfigKey::RATE_MEAS),
        (Key::RATE_NAV, ConfigKey::RATE_NAV),
        (Key::RATE_TIMEREF, ConfigKey::RATE_TIMEREF),
        (Key::NAVSPG_DYNMODEL, ConfigKey::NAVSPG_DYNMODEL),
        (Key::UART1_BAUDRATE, ConfigKey::UART1_BAUDRATE),
        (Key::UART1OUTPROT_UBX, ConfigKey::UART1_OUTPUT_UBX),
        (Key::UART1OUTPROT_NMEA, ConfigKey::UART1_OUTPUT_NMEA),
        (
            Key::MSGOUT_UBX_NAV_PVT_UART1,
            ConfigKey::MSGOUT_NAV_PVT_UART1,
        ),
    ];
    for (ours, theirs) in kinavis {
        assert_eq!(ours.raw(), theirs.raw(), "{ours:?}");
        assert_eq!(ours.size().bytes(), theirs.value_size(), "{ours:?}");
    }
    // ublox ubx_packets/cfg_val.rs:491-1435.
    let ublox = [
        (Key::RATE_MEAS, CfgKey::RateMeas),
        (Key::RATE_NAV, CfgKey::RateNav),
        (Key::RATE_TIMEREF, CfgKey::RateTimeref),
        (Key::UART1_BAUDRATE, CfgKey::Uart1Baudrate),
        (Key::UART1OUTPROT_UBX, CfgKey::Uart1OutProtUbx),
        (Key::UART1OUTPROT_NMEA, CfgKey::Uart1OutProtNmea),
        (Key::I2COUTPROT_UBX, CfgKey::I2cOutProtUbx),
        (Key::I2COUTPROT_NMEA, CfgKey::I2cOutProtNmea),
        (Key::SPIOUTPROT_UBX, CfgKey::SpiOutProtUbx),
        (Key::SPIOUTPROT_NMEA, CfgKey::SpiOutProtNmea),
        (Key::MSGOUT_UBX_NAV_PVT_UART1, CfgKey::MsgOutUbxNavPvtUart1),
        (Key::MSGOUT_UBX_NAV_PVT_I2C, CfgKey::MsgOutUbxNavPvtI2c),
        (Key::MSGOUT_UBX_NAV_PVT_SPI, CfgKey::MsgOutUbxNavPvtSpi),
        (Key::NAVSPG_DYNMODEL, CfgKey::NavSpgDynModel),
        (Key::SIGNAL_GPS_ENA, CfgKey::SignalGpsEna),
        (Key::SIGNAL_GAL_ENA, CfgKey::SignalGalEna),
        (Key::SIGNAL_BDS_ENA, CfgKey::SignalBdsEna),
        (Key::SIGNAL_QZSS_ENA, CfgKey::SignalQzssEna),
        (Key::SIGNAL_GLO_ENA, CfgKey::SignalGloEna),
    ];
    assert_eq!(ublox.len(), Key::KNOWN.len());
    for (ours, theirs) in ublox {
        assert_eq!(ours.raw(), theirs as u32, "{ours:?}");
    }
}

#[test]
fn dynamic_model_values_agree_with_ublox() {
    // Single-source: ublox ubx_packets/packets/cfg_nav5.rs:120-129.
    let pairs = [
        (DynamicModel::Portable, NavDynamicModel::Portable),
        (DynamicModel::Stationary, NavDynamicModel::Stationary),
        (DynamicModel::Pedestrian, NavDynamicModel::Pedestrian),
        (DynamicModel::Automotive, NavDynamicModel::Automotive),
        (DynamicModel::Sea, NavDynamicModel::Sea),
        (
            DynamicModel::AirborneBelow1g,
            NavDynamicModel::AirborneWithLess1gAcceleration,
        ),
        (
            DynamicModel::AirborneBelow2g,
            NavDynamicModel::AirborneWithLess2gAcceleration,
        ),
        (
            DynamicModel::AirborneBelow4g,
            NavDynamicModel::AirborneWithLess4gAcceleration,
        ),
    ];
    for (ours, theirs) in pairs {
        assert_eq!(ours as u8, theirs as u8, "{ours:?}");
        assert_eq!(DynamicModel::from_raw(theirs as u8), Some(ours));
    }
}

#[test]
fn valset_frames_are_byte_identical_to_both_references() {
    let ours = ours(&[
        Setting::new(Key::RATE_MEAS, 100).unwrap(),
        Setting::new(Key::UART1_BAUDRATE, 230_400).unwrap(),
        Setting::flag(Key::UART1OUTPROT_NMEA, false).unwrap(),
        Setting::new(Key::MSGOUT_UBX_NAV_PVT_UART1, 1).unwrap(),
        Setting::dynamic_model(DynamicModel::AirborneBelow4g),
    ]);

    let mut out = [0_u8; 256];
    let len = kv::valset(
        kv::Layers::RAM.and(kv::Layers::BBR),
        &[
            (ConfigKey::RATE_MEAS, 100),
            (ConfigKey::UART1_BAUDRATE, 230_400),
            (ConfigKey::UART1_OUTPUT_NMEA, 0),
            (ConfigKey::MSGOUT_NAV_PVT_UART1, 1),
            (ConfigKey::NAVSPG_DYNMODEL, 8),
        ],
        &mut out,
    )
    .unwrap();
    assert_eq!(ours, &out[..len], "kinavis-ubx");

    let mut theirs = Vec::new();
    CfgValSetBuilder {
        version: 0,
        layers: CfgLayerSet::RAM | CfgLayerSet::BBR,
        reserved1: 0,
        cfg_data: &[
            CfgVal::RateMeas(100),
            CfgVal::Uart1Baudrate(230_400),
            CfgVal::Uart1OutProtNmea(false),
            CfgVal::MsgOutUbxNavPvtUart1(1),
            CfgVal::NavSpgDynModel(NavDynamicModel::AirborneWithLess4gAcceleration),
        ],
    }
    .extend_to(&mut theirs);
    assert_eq!(ours, theirs, "ublox");
}

#[test]
fn valset_frames_from_either_side_deframe_on_the_other() {
    let ours = ours(&[
        Setting::new(Key::RATE_MEAS, 200).unwrap(),
        Setting::flag(Key::I2COUTPROT_NMEA, false).unwrap(),
    ]);
    let found = kinavis_deframe(&ours);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].0, found[0].1), (id::CLASS_CFG, id::CFG_VALSET));
    assert_eq!(found[0].2, &ours[6..ours.len() - 2]);

    let mut theirs = Vec::new();
    CfgValSetBuilder {
        version: 0,
        layers: CfgLayerSet::RAM,
        reserved1: 0,
        cfg_data: &[CfgVal::SignalGloEna(true), CfgVal::RateNav(1)],
    }
    .extend_to(&mut theirs);
    let found = deframe(&theirs);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].2,
        &[
            0, 1, 0, 0, 0x25, 0x00, 0x31, 0x10, 1, 0x02, 0x00, 0x21, 0x30, 1, 0
        ]
    );
}

#[test]
fn valget_requests_are_byte_identical_to_both_references() {
    let mut out = [0_u8; 64];
    let len = valget_request(
        PollLayer::Flash,
        3,
        &[Key::RATE_MEAS, Key::UART1_BAUDRATE, Key::NAVSPG_DYNMODEL],
        &mut out,
    )
    .unwrap();
    let ours = &out[..len];

    let mut kinavis = [0_u8; 64];
    let kinavis_len = kv::valget(
        kv::PollLayer::Flash,
        3,
        &[
            ConfigKey::RATE_MEAS,
            ConfigKey::UART1_BAUDRATE,
            ConfigKey::NAVSPG_DYNMODEL,
        ],
        &mut kinavis,
    )
    .unwrap();
    assert_eq!(ours, &kinavis[..kinavis_len], "kinavis-ubx");

    let mut ublox = Vec::new();
    CfgValGetRequestBuilder {
        version: 0,
        layers: CfgLayerGet::Flash,
        position: 3,
        cfg_keys: &[
            CfgKey::RateMeas,
            CfgKey::Uart1Baudrate,
            CfgKey::NavSpgDynModel,
        ],
    }
    .extend_to(&mut ublox);
    assert_eq!(ours, ublox, "ublox");
}

fn response_frame(pairs: &[u8]) -> Vec<u8> {
    let mut payload = vec![1, 0, 0, 0];
    payload.extend_from_slice(pairs);
    let mut out = [0_u8; 512];
    let len = encode(id::CLASS_CFG, id::CFG_VALGET, &payload, &mut out).unwrap();
    out[..len].to_vec()
}

#[test]
fn a_valget_response_reads_the_same_in_all_three() {
    let pairs = [
        0x02, 0x00, 0x74, 0x10, 1, // UART1OUTPROT-NMEA, bit
        0x21, 0x00, 0x11, 0x20, 8, // NAVSPG-DYNMODEL, byte
        0x01, 0x00, 0x21, 0x30, 0xE8, 0x03, // RATE-MEAS, two bytes
        0x01, 0x00, 0x52, 0x40, 0x00, 0xC2, 0x01, 0x00, // UART1-BAUDRATE, four bytes
    ];
    let bytes = response_frame(&pairs);
    let expected = [
        (Key::UART1OUTPROT_NMEA.raw(), 1_u64),
        (Key::NAVSPG_DYNMODEL.raw(), 8),
        (Key::RATE_MEAS.raw(), 1000),
        (Key::UART1_BAUDRATE.raw(), 115_200),
    ];

    let frames = deframe(&bytes);
    let ours: Vec<(u32, u64)> = ValGet::parse(&frames[0].2)
        .unwrap()
        .values()
        .map(|pair| pair.map(|(key, value)| (key.raw(), value)).unwrap())
        .collect();
    assert_eq!(ours, expected);

    let mut deframer = kinavis_ubx::Deframer::<1024>::new();
    let mut kinavis = Vec::new();
    for byte in &bytes {
        if let Some(kinavis_ubx::Event::Frame(frame)) = deframer.push(*byte) {
            let Ok(Message::CfgValget(valget)) = Message::decode(&frame) else {
                panic!("kinavis-ubx did not read a CFG-VALGET");
            };
            kinavis = valget
                .values()
                .map(|pair| pair.map(|(key, value)| (key.raw(), value)).unwrap())
                .collect();
        }
    }
    assert_eq!(kinavis, expected);

    let mut parser = ParserBuilder::new()
        .with_protocol::<Proto27>()
        .with_fixed_buffer::<1024>();
    let mut it = parser.consume_ubx(&bytes);
    let Some(Ok(UbxPacket::Proto27(PacketRef::CfgValGetResponse(response)))) = it.next() else {
        panic!("ublox did not read a CFG-VALGET");
    };
    let ublox: Vec<CfgVal> = response.cfg_data().collect();
    assert_eq!(
        ublox,
        [
            CfgVal::Uart1OutProtNmea(true),
            CfgVal::NavSpgDynModel(NavDynamicModel::AirborneWithLess4gAcceleration),
            CfgVal::RateMeas(1000),
            CfgVal::Uart1Baudrate(115_200),
        ]
    );
}

/// Run `f`, reporting whether it panicked, without the panic message.
fn panics(f: impl FnOnce() + panic::UnwindSafe) -> bool {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(f);
    panic::set_hook(hook);
    result.is_err()
}

#[test]
fn where_ublox_panics_on_a_valget_response_ours_and_kinavis_return_an_error() {
    // A four-byte value cut to one byte, and a one-bit value of 2. A receiver
    // never sends either, but a corrupt link can deliver a frame whose
    // checksum still matches. ublox indexes past the end
    // (ubx_packets/cfg_val.rs:404-409) and reaches unreachable! (:70-76): a
    // panic, which W2 forbids. Ours returns a typed error, which is right.
    // kinavis-ubx returns Truncated for the first and accepts 2 for the
    // second; refusing it, as ours does, is the stricter reading of a
    // one-bit value.
    let truncated = [0x01, 0x00, 0x52, 0x40, 0x00];
    let bad_bit = [0x02, 0x00, 0x74, 0x10, 2];
    for (pairs, kinavis_errors) in [(&truncated[..], true), (&bad_bit[..], false)] {
        let bytes = response_frame(pairs);
        let frames = deframe(&bytes);
        let first = ValGet::parse(&frames[0].2).unwrap().values().next();
        assert!(matches!(first, Some(Err(_))), "ours: {first:?}");

        let mut deframer = kinavis_ubx::Deframer::<1024>::new();
        for byte in &bytes {
            if let Some(kinavis_ubx::Event::Frame(frame)) = deframer.push(*byte) {
                let Ok(Message::CfgValget(valget)) = Message::decode(&frame) else {
                    panic!("kinavis-ubx did not read a CFG-VALGET");
                };
                let first = valget.values().next();
                assert_eq!(matches!(first, Some(Err(_))), kinavis_errors, "{first:?}");
            }
        }

        let owned = bytes.clone();
        assert!(
            panics(move || {
                let mut parser = ParserBuilder::new()
                    .with_protocol::<Proto27>()
                    .with_fixed_buffer::<1024>();
                let mut it = parser.consume_ubx(&owned);
                if let Some(Ok(UbxPacket::Proto27(PacketRef::CfgValGetResponse(response)))) =
                    it.next()
                {
                    let _ = response.cfg_data().count();
                }
            }),
            "ublox no longer panics on {pairs:?}; revisit this test"
        );
    }
}

/// A NAV-PVT payload with a distinct value in every field. Positions from
/// kinavis-ubx's reference table, tests/data/ubx_reference.txt:42.
fn nav_pvt_payload() -> [u8; 92] {
    let mut p = [0_u8; 92];
    let mut put = |at: usize, bytes: &[u8]| p[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, &403_075_000_u32.to_le_bytes());
    put(4, &2026_u16.to_le_bytes());
    put(6, &[11, 4, 13, 37, 59, 0x03]);
    put(12, &25_u32.to_le_bytes());
    put(16, &(-123_456_i32).to_le_bytes());
    put(20, &[3, 0x03, 0x00, 10]);
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
    put(84, &(-2_000_000_i32).to_le_bytes());
    put(88, &(-345_i16).to_le_bytes());
    put(90, &77_u16.to_le_bytes());
    p
}

#[test]
fn nav_pvt_decodes_the_same_in_all_three() {
    let payload = nav_pvt_payload();
    let mut out = [0_u8; 128];
    let len = encode(id::CLASS_NAV, id::NAV_PVT, &payload, &mut out).unwrap();
    let bytes = &out[..len];

    let frames = deframe(bytes);
    let ours = NavPvt::decode(&frames[0].2).unwrap();
    let fix = ours.fix().unwrap();
    let s = fix.solution;
    let utc = ours.utc().unwrap();

    let mut parser = ParserBuilder::new()
        .with_protocol::<Proto27>()
        .with_fixed_buffer::<1024>();
    let mut it = parser.consume_ubx(bytes);
    let Some(Ok(UbxPacket::Proto27(PacketRef::NavPvt(u)))) = it.next() else {
        panic!("ublox did not read a NAV-PVT");
    };
    assert_eq!(ours.itow_ms, u.itow());
    assert_eq!(
        (
            utc.year, utc.month, utc.day, utc.hour, utc.minute, utc.second
        ),
        (u.year(), u.month(), u.day(), u.hour(), u.min(), u.sec())
    );
    assert_eq!(utc.accuracy_ns, u.time_accuracy());
    assert_eq!(utc.nano_ns, u.nanosec());
    assert_eq!(ours.flags.0, u.flags_raw());
    assert_eq!(ours.num_sv, u.num_satellites());
    assert_eq!(s.lon_e7_deg, u.longitude_raw());
    assert_eq!(s.lat_e7_deg, u.latitude_raw());
    assert_eq!(s.height_mm, u.height_above_ellipsoid_raw());
    assert_eq!(s.height_msl_mm, u.height_msl_raw());
    assert_eq!(s.h_acc_mm, u.horizontal_accuracy_raw());
    assert_eq!(s.v_acc_mm, u.vertical_accuracy_raw());
    assert_eq!(s.vel_n_mm_s, u.vel_north_raw());
    assert_eq!(s.vel_e_mm_s, u.vel_east_raw());
    assert_eq!(s.vel_d_mm_s, u.vel_down_raw());
    assert_eq!(s.g_speed_mm_s, u.ground_speed_2d_raw());
    assert_eq!(s.head_mot_e5_deg, u.heading_motion_raw());
    assert_eq!(s.s_acc_mm_s, u.speed_accuracy_raw());
    assert_eq!(s.head_acc_e5_deg, u.heading_accuracy_raw());
    assert_eq!(s.p_dop_e2, u.pdop_raw());
    assert_eq!(s.head_veh_e5_deg, u.heading_vehicle_raw());
    assert_eq!(s.mag_dec_e2_deg, u.magnetic_declination_raw());
    assert_eq!(s.mag_acc_e2_deg, u.magnetic_declination_accuracy_raw());

    let mut deframer = kinavis_ubx::Deframer::<1024>::new();
    let mut checked = false;
    for byte in bytes {
        if let Some(kinavis_ubx::Event::Frame(frame)) = deframer.push(*byte) {
            let Ok(Message::NavPvt(k)) = Message::decode(&frame) else {
                panic!("kinavis-ubx did not read a NAV-PVT");
            };
            assert_eq!(ours.itow_ms, k.itow);
            assert_eq!(
                (
                    utc.year, utc.month, utc.day, utc.hour, utc.minute, utc.second
                ),
                k.date_time
            );
            assert_eq!(utc.nano_ns, k.nanos);
            assert_eq!(utc.accuracy_ns, k.time_accuracy);
            assert!(k.valid_date && k.valid_time);
            assert_eq!(k.fix, PvtFix::ThreeD);
            assert!(k.fix_ok && k.differential);
            assert_eq!(ours.num_sv, k.satellites);
            assert!(!k.invalid_llh);
            let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
            assert!(close(
                f64::from(s.lat_e7_deg) * 1e-7,
                k.position.latitude().degrees()
            ));
            assert!(close(
                f64::from(s.lon_e7_deg) * 1e-7,
                k.position.longitude().degrees()
            ));
            assert!(close(f64::from(s.height_mm) / 1000.0, k.height.metres()));
            assert!(close(
                f64::from(s.height_msl_mm) / 1000.0,
                k.height_msl.metres()
            ));
            assert!(close(
                f64::from(s.h_acc_mm) / 1000.0,
                k.horizontal_accuracy.metres()
            ));
            assert!(close(
                f64::from(s.v_acc_mm) / 1000.0,
                k.vertical_accuracy.metres()
            ));
            assert!(close(
                f64::from(s.vel_n_mm_s) / 1000.0,
                k.velocity.north().metres_per_second()
            ));
            assert!(close(
                f64::from(s.vel_e_mm_s) / 1000.0,
                k.velocity.east().metres_per_second()
            ));
            assert!(close(
                f64::from(s.vel_d_mm_s) / 1000.0,
                k.velocity.down().metres_per_second()
            ));
            assert!(close(
                f64::from(s.g_speed_mm_s) / 1000.0,
                k.ground_speed.metres_per_second()
            ));
            assert!(close(
                f64::from(s.head_mot_e5_deg) * 1e-5,
                k.heading_of_motion.degrees()
            ));
            assert!(close(
                f64::from(s.s_acc_mm_s) / 1000.0,
                k.speed_accuracy.metres_per_second()
            ));
            assert!(close(
                f64::from(s.head_acc_e5_deg) * 1e-5,
                k.heading_accuracy.degrees()
            ));
            checked = true;
        }
    }
    assert!(checked);
}

#[test]
fn a_no_fix_nav_pvt_is_no_position_here_and_no_fix_in_both_references() {
    let mut payload = nav_pvt_payload();
    payload[20] = 0;
    payload[21] = 0;
    let mut out = [0_u8; 128];
    let len = encode(id::CLASS_NAV, id::NAV_PVT, &payload, &mut out).unwrap();
    let bytes = &out[..len];
    let frames = deframe(bytes);
    assert_eq!(NavPvt::decode(&frames[0].2).unwrap().fix(), None);

    let mut deframer = kinavis_ubx::Deframer::<1024>::new();
    for byte in bytes {
        if let Some(kinavis_ubx::Event::Frame(frame)) = deframer.push(*byte) {
            let Ok(Message::NavPvt(k)) = Message::decode(&frame) else {
                panic!("kinavis-ubx did not read a NAV-PVT");
            };
            assert_eq!(k.fix, PvtFix::NoFix);
            assert!(!k.fix_ok);
        }
    }
}

#[test]
fn acks_built_by_ublox_tests_match_their_requests_here() {
    use ubx::cfg::{Answer, Pending};
    // ublox tests/parser_tests.rs:102: an ACK-ACK of CFG-MSG (06 01).
    let ack_cfg_msg = [0xB5, 0x62, 0x05, 0x01, 0x02, 0x00, 0x06, 0x01, 0x0F, 0x38];
    let frames = deframe(&ack_cfg_msg);
    let frame = ubx::Frame {
        class: frames[0].0,
        id: frames[0].1,
        payload: &frames[0].2,
    };
    assert_eq!(
        Pending {
            class: 0x06,
            id: 0x01
        }
        .answer(&frame),
        Answer::Accepted
    );
    assert_eq!(Pending::VALSET.answer(&frame), Answer::Unrelated);
    let found = kinavis_deframe(&ack_cfg_msg);
    let decoded = Message::decode(&kinavis_ubx::Frame {
        class: found[0].0,
        id: found[0].1,
        payload: &found[0].2,
    });
    assert!(matches!(
        decoded,
        Ok(Message::Ack {
            class: 0x06,
            id: 0x01
        })
    ));
}

#[test]
fn a_frame_hidden_in_a_corrupt_one_is_found_here_and_lost_by_kinavis() {
    // A header announcing 30 bytes, then a valid ACK inside them, then a bad
    // checksum. kinavis-ubx resynchronises only after the dropped frame
    // (frame.rs:197-212), so it loses the ACK; ours scans the dropped bytes
    // again and returns it. A receiver's answer lost to one corrupt byte
    // before it is the case this crate's resync exists for.
    let mut ack = [0_u8; 16];
    let ack_len = encode(id::CLASS_ACK, id::ACK_ACK, &[0x06, 0x8A], &mut ack).unwrap();
    let mut stream = vec![0xB5, 0x62, 0x01, 0x07, 30, 0];
    stream.extend_from_slice(&ack[..ack_len]);
    stream.resize(6 + 30, 0);
    stream.extend_from_slice(&[0, 0]);
    assert_eq!(
        deframe(&stream),
        [(id::CLASS_ACK, id::ACK_ACK, vec![0x06, 0x8A])]
    );
    assert!(kinavis_deframe(&stream).is_empty());
}
