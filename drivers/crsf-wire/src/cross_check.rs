//! Cross-checks against `uf-crsf`, an independent CRSF implementation used
//! here as a test oracle only: this crate's frames parsed by it, and its
//! frames parsed by this crate's streaming parser.
//!
//! Where the two differ, the test says which side follows the
//! specification and why.

use uf_crsf::packets::{
    Attitude, BaroAltitude, Battery, FlightMode as UfFlightMode, Gps, LinkStatistics as UfLink,
    RcChannelsPacked, VariometerSensor,
};
use uf_crsf::{CrsfParser, Packet, PacketAddress, write_packet_to_buffer};

use crate::channels::CHANNEL_COUNT;
use crate::frame::{
    FRAME_TYPE_ATTITUDE, FRAME_TYPE_BARO_ALTITUDE, FRAME_TYPE_BATTERY_SENSOR,
    FRAME_TYPE_FLIGHT_MODE, FRAME_TYPE_GPS, FRAME_TYPE_VARIO, MAX_FRAME_LEN, is_frame_address,
};
use crate::parser::{Frame, LinkStatistics, ParseResult, StreamingParser};
use crate::telemetry::{
    AttitudeTelemetry, BaroAltitudeTelemetry, BatteryTelemetry, Centidegrees, CentimetresPerSecond,
    Deciamps, Decimetres, Decivolts, FlightMode, GpsTelemetry, Latitude, Longitude, Metres,
    MilliampHours, Percent, RadiansE4, encode_attitude, encode_baro_altitude, encode_battery,
    encode_flight_mode, encode_gps, encode_vario, pack_baro_altitude, pack_vertical_speed,
};

/// A deterministic xorshift generator, so a failure reproduces.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// The one packet uf-crsf finds in `frame`.
fn oracle(frame: &[u8]) -> Packet {
    let mut parser = CrsfParser::new();
    let mut input = frame;
    let packet = parser.next_packet(&mut input);
    assert!(input.is_empty(), "uf-crsf left {} bytes", input.len());
    match packet {
        Some(Ok(packet)) => packet,
        other => panic!("uf-crsf did not parse the frame: {other:?}"),
    }
}

fn ours(frame: &[u8]) -> Option<ParseResult> {
    let mut parser = StreamingParser::new();
    let mut last = None;
    for byte in frame {
        if let Some(result) = parser.push_byte(*byte) {
            last = Some(result);
        }
    }
    last
}

fn build<T: uf_crsf::packets::CrsfPacket>(address: PacketAddress, packet: &T) -> ([u8; 64], usize) {
    let mut buffer = [0_u8; MAX_FRAME_LEN];
    let len =
        write_packet_to_buffer(&mut buffer, address, packet).expect("uf-crsf builds the frame");
    (buffer, len)
}

#[test]
fn battery_frames_parse_in_uf_crsf_with_the_same_fields() {
    let mut noise = Noise(0x1111_2222_3333_4444);
    for _ in 0..5_000 {
        let telemetry = BatteryTelemetry {
            voltage: Decivolts(noise.next() as u16),
            current: Deciamps(noise.next() as u16),
            used: MilliampHours(noise.next() as u32),
            remaining: Percent(noise.next() as u8),
        };
        let frame = encode_battery(telemetry);
        let Packet::Battery(battery) = oracle(&frame) else {
            panic!("not a battery packet");
        };
        // Saturating at i16::MAX means uf-crsf's signed fields and an
        // unsigned reader agree on every value sent.
        assert_eq!(battery.voltage, telemetry.voltage.0.min(0x7FFF) as i16);
        assert_eq!(battery.current, telemetry.current.0.min(0x7FFF) as i16);
        assert_eq!(battery.capacity_used, telemetry.used.0.min(0x00FF_FFFF));
        assert_eq!(battery.remaining, telemetry.remaining.0.min(100));
        assert_eq!(battery.id, 0);
    }
}

#[test]
fn gps_frames_parse_in_uf_crsf_with_the_same_fields() {
    let mut noise = Noise(0x5555_6666_7777_8888);
    for _ in 0..5_000 {
        let latitude = (noise.next() % 1_800_000_001) as i32 - 900_000_000;
        let longitude = (noise.next() % 3_600_000_001) as i64 - 1_800_000_000;
        let telemetry = GpsTelemetry {
            latitude: Latitude::from_degrees_e7(latitude).unwrap(),
            longitude: Longitude::from_degrees_e7(longitude as i32).unwrap(),
            ground_speed: CentimetresPerSecond((noise.next() % 200_000) as i32 - 1_000),
            heading: Centidegrees(noise.next() as i32),
            altitude: Metres((noise.next() % 70_000) as i32 - 2_000),
            satellites: noise.next() as u8,
        };
        let frame = encode_gps(telemetry);
        let Packet::Gps(gps) = oracle(&frame) else {
            panic!("not a GPS packet");
        };
        assert_eq!(gps.latitude, latitude);
        assert_eq!(i64::from(gps.longitude), longitude);
        // uf-crsf documents this field as 0.01 km/h, but the specification
        // and every flight controller that sends it use 0.1 km/h; the raw
        // value is the same either way, so only the unit's name differs.
        let speed = i64::from(telemetry.ground_speed.0.max(0));
        assert_eq!(
            i64::from(gps.groundspeed),
            ((speed * 36 + 50) / 100).min(0xFFFF)
        );
        assert_eq!(gps.heading, telemetry.heading.0.rem_euclid(36_000) as u16);
        let altitude = i64::from(telemetry.altitude.0) + 1_000;
        assert_eq!(i64::from(gps.altitude), altitude.clamp(0, 0xFFFF));
        assert_eq!(gps.satellites, telemetry.satellites);
    }
}

#[test]
fn vario_frames_parse_in_uf_crsf_with_the_same_speed() {
    for speed in (-40_000..=40_000).step_by(7) {
        let frame = encode_vario(CentimetresPerSecond(speed));
        let Packet::Vario(vario) = oracle(&frame) else {
            panic!("not a vario packet");
        };
        assert_eq!(i32::from(vario.v_speed), speed.clamp(-32_768, 32_767));
    }
}

#[test]
fn baro_altitude_frames_parse_in_uf_crsf_and_pack_as_it_packs() {
    for dm in (-12_000..=340_000).step_by(3) {
        assert_eq!(
            pack_baro_altitude(Decimetres(dm)),
            BaroAltitude::get_altitude_packed(dm),
            "{dm} dm"
        );
    }
    for speed in -20..=20 {
        let telemetry = BaroAltitudeTelemetry {
            altitude: Decimetres(speed * 400),
            vertical_speed: CentimetresPerSecond(speed * 150),
        };
        let frame = encode_baro_altitude(telemetry);
        let Packet::BaroAltitude(baro) = oracle(&frame) else {
            panic!("not a baro altitude packet");
        };
        assert_eq!(baro.altitude_packed, pack_baro_altitude(telemetry.altitude));
        assert_eq!(
            baro.vertical_speed_packed,
            pack_vertical_speed(telemetry.vertical_speed)
        );
        assert_eq!(baro.get_altitude_dm(), speed * 400);
    }
}

/// uf-crsf packs the vertical speed in `f32`, this crate from a table of the
/// formula's step boundaries, checked against the formula in `f64` in the
/// telemetry tests. uf-crsf takes `i16`, and `i16::MIN` overflows its `abs`,
/// so the sweep stops one short of it.
#[test]
fn vertical_speed_packing_agrees_with_uf_crsf() {
    for speed in -32_767_i16..=i16::MAX {
        assert_eq!(
            pack_vertical_speed(CentimetresPerSecond(i32::from(speed))),
            BaroAltitude::get_vertical_speed_packed(speed),
            "{speed} cm/s"
        );
    }
}

#[test]
fn attitude_frames_parse_in_uf_crsf_with_the_same_angles() {
    let mut noise = Noise(0x9999_AAAA_BBBB_CCCC);
    for _ in 0..5_000 {
        let angle = |noise: &mut Noise| RadiansE4((noise.next() % 200_000) as i32 - 100_000);
        let telemetry = AttitudeTelemetry {
            pitch: angle(&mut noise),
            roll: angle(&mut noise),
            yaw: angle(&mut noise),
        };
        let frame = encode_attitude(telemetry);
        let Packet::Attitude(attitude) = oracle(&frame) else {
            panic!("not an attitude packet");
        };
        let wrap =
            |angle: RadiansE4| ((i64::from(angle.0) + 31_416).rem_euclid(62_832) - 31_416) as i16;
        assert_eq!(attitude.pitch, wrap(telemetry.pitch));
        assert_eq!(attitude.roll, wrap(telemetry.roll));
        assert_eq!(attitude.yaw, wrap(telemetry.yaw));
    }
}

#[test]
fn flight_mode_frames_parse_in_uf_crsf_with_the_same_name() {
    for name in ["", "ACRO", "!FS!", "ANGLE*", "WAIT", &"M".repeat(59)] {
        let frame = encode_flight_mode(&FlightMode::new(name).unwrap());
        let Packet::FlightMode(mode) = oracle(frame.as_bytes()) else {
            panic!("not a flight mode packet");
        };
        assert_eq!(mode.flight_mode(), name);
    }
}

#[test]
fn rc_channel_frames_from_uf_crsf_decode_to_the_same_channels() {
    let mut noise = Noise(0x0F0F_F0F0_1234_5678);
    for round in 0..5_000 {
        let mut channels = [0_u16; CHANNEL_COUNT];
        for channel in &mut channels {
            *channel = (noise.next() as u16) & 0x07FF;
        }
        let address = [
            PacketAddress::FlightController,
            PacketAddress::Receiver,
            PacketAddress::Transmitter,
        ][round % 3];
        let (frame, len) = build(address, &RcChannelsPacked(channels));
        assert_eq!(ours(&frame[..len]), Some(Ok(Frame::RcChannels(channels))));
    }
}

#[test]
fn link_statistics_from_uf_crsf_decode_to_the_same_fields() {
    let mut noise = Noise(0xABCD_EF01_2345_6789);
    for _ in 0..5_000 {
        let bytes = noise.next().to_le_bytes();
        let extra = noise.next().to_le_bytes();
        let packet = UfLink::new(
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3] as i8,
            bytes[4],
            bytes[5],
            bytes[6],
            bytes[7],
            extra[0],
            extra[1] as i8,
        )
        .unwrap();
        let (frame, len) = build(PacketAddress::FlightController, &packet);
        assert_eq!(
            ours(&frame[..len]),
            Some(Ok(Frame::LinkStatistics(LinkStatistics {
                uplink_rssi_ant1: bytes[0],
                uplink_rssi_ant2: bytes[1],
                uplink_link_quality: bytes[2],
                uplink_snr: bytes[3] as i8,
                active_antenna: bytes[4],
                rf_mode: bytes[5],
                uplink_tx_power: bytes[6],
                downlink_rssi: bytes[7],
                downlink_link_quality: extra[0],
                downlink_snr: extra[1] as i8,
            })))
        );
    }
}

#[test]
fn telemetry_frames_from_uf_crsf_pass_through_as_other() {
    let frames = [
        build(
            PacketAddress::FlightController,
            &Battery::new(168, 12, 1_000, 80).unwrap(),
        ),
        build(
            PacketAddress::FlightController,
            &Gps::new(1, 2, 3, 4, 5, 6).unwrap(),
        ),
        build(
            PacketAddress::FlightController,
            &VariometerSensor::new(-5).unwrap(),
        ),
        build(
            PacketAddress::FlightController,
            &BaroAltitude::new(10_000, 3).unwrap(),
        ),
        build(
            PacketAddress::FlightController,
            &Attitude::new(1, 2, 3).unwrap(),
        ),
        build(
            PacketAddress::FlightController,
            &UfFlightMode::new("ACRO").unwrap(),
        ),
    ];
    let types = [
        FRAME_TYPE_BATTERY_SENSOR,
        FRAME_TYPE_GPS,
        FRAME_TYPE_VARIO,
        FRAME_TYPE_BARO_ALTITUDE,
        FRAME_TYPE_ATTITUDE,
        FRAME_TYPE_FLIGHT_MODE,
    ];
    for ((frame, len), frame_type) in frames.iter().zip(types) {
        assert_eq!(ours(&frame[..*len]), Some(Ok(Frame::Other { frame_type })));
    }
}

/// uf-crsf accepts every address it knows as a frame start, 0x00 among
/// them; this crate accepts only the three that appear on a receiver's
/// link to a flight controller. Fewer accepted start bytes means noise
/// starts fewer false frames for the 8-bit CRC to catch, so the narrower
/// set is deliberate.
#[test]
fn frames_at_other_addresses_are_ignored_here_and_accepted_by_uf_crsf() {
    let channels = [992_u16; CHANNEL_COUNT];
    for address in [
        PacketAddress::Broadcast,
        PacketAddress::Usb,
        PacketAddress::Gps,
        PacketAddress::Handset,
        PacketAddress::Esc1,
    ] {
        assert!(!is_frame_address(address as u8));
        let (frame, len) = build(address, &RcChannelsPacked(channels));
        assert!(matches!(oracle(&frame[..len]), Packet::RCChannels(_)));
        assert_eq!(ours(&frame[..len]), None, "{address:?}");
    }
}

#[test]
fn a_stream_of_valid_frames_gives_the_same_frames_to_both_parsers() {
    let mut noise = Noise(0x7777_1111_9999_3333);
    let mut stream = [0_u8; 4_096];
    let mut len = 0;
    let mut expected = 0;
    while len + MAX_FRAME_LEN <= stream.len() {
        let mut channels = [0_u16; CHANNEL_COUNT];
        for channel in &mut channels {
            *channel = (noise.next() as u16) & 0x07FF;
        }
        let (frame, frame_len) = if noise.next().is_multiple_of(2) {
            build(PacketAddress::Receiver, &RcChannelsPacked(channels))
        } else {
            build(
                PacketAddress::FlightController,
                &Attitude::new(1, 2, 3).unwrap(),
            )
        };
        stream[len..len + frame_len].copy_from_slice(&frame[..frame_len]);
        len += frame_len;
        expected += 1;
    }

    let mut mine = [0_u8; 256];
    let mut mine_count = 0;
    let mut parser = StreamingParser::new();
    for byte in &stream[..len] {
        if let Some(result) = parser.push_byte(*byte) {
            mine[mine_count] = match result {
                Ok(Frame::RcChannels(_)) => 0x16,
                Ok(Frame::Other { frame_type }) => frame_type,
                other => panic!("{other:?}"),
            };
            mine_count += 1;
        }
    }

    let mut theirs = [0_u8; 256];
    let mut theirs_count = 0;
    let mut oracle = CrsfParser::new();
    let mut input = &stream[..len];
    while let Some(result) = oracle.next_packet(&mut input) {
        theirs[theirs_count] = match result {
            Ok(Packet::RCChannels(_)) => 0x16,
            Ok(Packet::Attitude(_)) => FRAME_TYPE_ATTITUDE,
            other => panic!("{other:?}"),
        };
        theirs_count += 1;
    }

    assert_eq!(mine_count, expected);
    assert_eq!(theirs_count, expected);
    assert_eq!(mine[..mine_count], theirs[..theirs_count]);
}
