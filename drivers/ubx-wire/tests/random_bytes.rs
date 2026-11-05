//! Random input, from a fixed-seed generator so a failure reproduces: no
//! panic, bounded state, every frame returned checks, and valid frames buried
//! in noise all come back.

use ubx_wire::cfg::{Pending, ValGet};
use ubx_wire::{Deframer, Event, NavPvt, checksum, encode};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn byte(&mut self) -> u8 {
        self.next().to_le_bytes()[0]
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Noise that leans on sync bytes, so candidate frames are common.
fn noise(rng: &mut XorShift) -> u8 {
    match rng.below(8) {
        0 => 0xB5,
        1 => 0x62,
        _ => rng.byte(),
    }
}

fn check_event<const BUF: usize>(event: &Event<'_>) {
    if let Event::Frame(frame) = event {
        assert!(frame.payload.len() <= Deframer::<BUF>::MAX_PAYLOAD);
        let mut out = vec![0_u8; frame.payload.len() + 8];
        encode(frame.class, frame.id, frame.payload, &mut out).unwrap();
        assert_eq!(
            checksum(&out[2..out.len() - 2]),
            [out[out.len() - 2], out[out.len() - 1]]
        );
        // Whatever the payload, decoding it never panics.
        let _ = NavPvt::decode(frame.payload);
        if let Ok(valget) = ValGet::parse(frame.payload) {
            assert!(valget.values().count() <= frame.payload.len() / 4 + 1);
        }
        let _ = Pending::VALSET.answer(frame);
    }
}

fn hammer<const BUF: usize>(seed: u64, bytes: usize) {
    let mut rng = XorShift(seed);
    let mut deframer = Deframer::<BUF>::new();
    for _ in 0..bytes {
        let byte = noise(&mut rng);
        if let Some(event) = deframer.push(byte) {
            check_event::<BUF>(&event);
        }
        assert!(deframer.held() <= BUF);
        // Drain some of the time, not always, so a backlog also builds.
        if rng.below(2) == 0 {
            let mut drained = 0;
            while let Some(event) = deframer.poll() {
                check_event::<BUF>(&event);
                drained += 1;
                assert!(drained <= BUF, "poll never ran dry");
            }
        }
    }
    assert_eq!(deframer.stats().overrun, 0);
}

#[test]
fn random_bytes_never_panic_and_state_stays_bounded() {
    hammer::<8>(1, 50_000);
    hammer::<9>(2, 50_000);
    hammer::<16>(3, 100_000);
    hammer::<100>(4, 200_000);
    hammer::<1024>(5, 200_000);
}

#[test]
fn random_payloads_never_panic_the_decoders() {
    let mut rng = XorShift(6);
    for _ in 0..20_000 {
        let len = rng.below(200);
        let payload: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
        let _ = NavPvt::decode(&payload);
        if let Ok(pvt) = NavPvt::decode(&payload) {
            let _ = (pvt.fix(), pvt.utc());
        }
        if let Ok(valget) = ValGet::parse(&payload) {
            let mut seen = 0;
            for pair in valget.values() {
                seen += 1;
                if pair.is_err() {
                    break;
                }
            }
            assert!(seen <= len / 4 + 1);
        }
    }
}

#[test]
fn valid_frames_buried_in_noise_all_come_back_in_order() {
    let mut rng = XorShift(7);
    let mut stream = Vec::new();
    let mut sent = Vec::new();
    for _ in 0..2_000 {
        for _ in 0..rng.below(40) {
            stream.push(noise(&mut rng));
        }
        let len = rng.below(56);
        let payload: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
        let (class, id) = (rng.byte(), rng.byte());
        let mut out = [0_u8; 64];
        let n = encode(class, id, &payload, &mut out).unwrap();
        stream.extend_from_slice(&out[..n]);
        sent.push((class, id, payload));
    }
    let mut deframer = Deframer::<64>::new();
    let mut got = Vec::new();
    for byte in &stream {
        let mut event = deframer.push(*byte).map(owned);
        while let Some(found) = event {
            got.extend(found);
            event = deframer.poll().map(owned);
        }
    }
    // Noise can forge a frame that passes its checksum, so what came back is
    // a superset; every frame sent must be in it, in order.
    let mut wanted = sent.iter().peekable();
    for frame in &got {
        if wanted.peek() == Some(&frame) {
            wanted.next();
        }
    }
    assert!(wanted.peek().is_none(), "lost a frame");
}

fn owned(event: Event<'_>) -> Option<(u8, u8, Vec<u8>)> {
    match event {
        Event::Frame(frame) => Some((frame.class, frame.id, frame.payload.to_vec())),
        Event::Error(_) => None,
    }
}
