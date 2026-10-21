//! The observer (ADR-0056, ADR-0061): the one way data leaves the
//! safety-critical tasks. Each run it takes what every link has brought,
//! publishes it on the ledger, and passes every control tick on to the
//! blackbox stream. A producer never waits for it: a link it falls behind on
//! refuses and counts, and the refusals show as gaps in the sequence.
//!
//! The ledger is a shared resource the observer and its readers lock, all
//! below the core. Its readers can only copy a topic out; only this module
//! publishes.

use ferrowasp_boundary_contracts::messages::{
    BatteryStatus, ControlTick, DrdyStatus, EscStatus, ImuStatus, OutputStatus, RcStatus,
    SafetyStatus, StorageStatus,
};
use ferrowasp_boundary_contracts::topics::{Identity, ObserverStatus};
use ferrowasp_io_contracts::link::{LinkConsumer, LinkProducer, Stamped};
use ferrowasp_io_contracts::time::TimestampMicros;

/// The latest of every topic. A topic's sequence is 0 until it is first
/// published.
#[derive(Clone, Debug)]
pub struct Ledger {
    identity: Identity,
    observer: Stamped<ObserverStatus>,
    control: Stamped<ControlTick>,
    imu: Stamped<ImuStatus>,
    drdy: Stamped<DrdyStatus>,
    rc: Stamped<RcStatus>,
    safety: Stamped<SafetyStatus>,
    output: Stamped<OutputStatus>,
    esc: Stamped<EscStatus>,
    battery: Stamped<BatteryStatus>,
    storage: Stamped<StorageStatus>,
}

impl Ledger {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            observer: Stamped::default(),
            control: Stamped::default(),
            imu: Stamped::default(),
            drdy: Stamped::default(),
            rc: Stamped::default(),
            safety: Stamped::default(),
            output: Stamped::default(),
            esc: Stamped::default(),
            battery: Stamped::default(),
            storage: Stamped::default(),
        }
    }

    pub fn identity(&self) -> Identity {
        self.identity
    }

    pub fn observer(&self) -> Stamped<ObserverStatus> {
        self.observer
    }

    pub fn control(&self) -> Stamped<ControlTick> {
        self.control
    }

    pub fn imu(&self) -> Stamped<ImuStatus> {
        self.imu
    }

    pub fn drdy(&self) -> Stamped<DrdyStatus> {
        self.drdy
    }

    pub fn rc(&self) -> Stamped<RcStatus> {
        self.rc
    }

    pub fn safety(&self) -> Stamped<SafetyStatus> {
        self.safety
    }

    pub fn output(&self) -> Stamped<OutputStatus> {
        self.output
    }

    pub fn esc(&self) -> Stamped<EscStatus> {
        self.esc
    }

    pub fn battery(&self) -> Stamped<BatteryStatus> {
        self.battery
    }

    pub fn storage(&self) -> Stamped<StorageStatus> {
        self.storage
    }
}

/// The consuming end of every link into the observer.
pub struct ObserverLinks<'a> {
    pub control: LinkConsumer<'a, ControlTick>,
    pub imu: LinkConsumer<'a, ImuStatus>,
    pub drdy: LinkConsumer<'a, DrdyStatus>,
    pub rc: LinkConsumer<'a, RcStatus>,
    pub safety: LinkConsumer<'a, SafetyStatus>,
    pub output: LinkConsumer<'a, OutputStatus>,
    pub esc: LinkConsumer<'a, EscStatus>,
    pub battery: LinkConsumer<'a, BatteryStatus>,
    pub storage: LinkConsumer<'a, StorageStatus>,
}

pub struct Observer<'a> {
    links: ObserverLinks<'a>,
    stream: LinkProducer<'a, ControlTick>,
    runs: u32,
    last_control_seq: u32,
    control_ticks_lost: u32,
}

impl<'a> Observer<'a> {
    /// `stream` is the blackbox's `event` link, which receives every control
    /// tick with its original stamp.
    pub fn new(links: ObserverLinks<'a>, stream: LinkProducer<'a, ControlTick>) -> Self {
        Self {
            links,
            stream,
            runs: 0,
            last_control_seq: 0,
            control_ticks_lost: 0,
        }
    }

    /// One run at `now`: take at most each link's capacity, publish the
    /// newest of every `state` link and the last control tick, keep a
    /// topic's previous value when its link brought nothing, and forward
    /// every control tick to the stream in order.
    pub fn run_once(&mut self, now: TimestampMicros, ledger: &mut Ledger) {
        for _ in 0..self.links.control.capacity() {
            let Some(tick) = self.links.control.oldest() else {
                break;
            };
            self.control_ticks_lost = self
                .control_ticks_lost
                .wrapping_add(messages_lost(self.last_control_seq, tick.seq));
            self.last_control_seq = tick.seq;
            self.stream.forward(tick);
            ledger.control = tick;
        }

        let links = &mut self.links;
        publish_newest(&mut links.imu, &mut ledger.imu);
        publish_newest(&mut links.drdy, &mut ledger.drdy);
        publish_newest(&mut links.rc, &mut ledger.rc);
        publish_newest(&mut links.safety, &mut ledger.safety);
        publish_newest(&mut links.output, &mut ledger.output);
        publish_newest(&mut links.esc, &mut ledger.esc);
        publish_newest(&mut links.battery, &mut ledger.battery);
        publish_newest(&mut links.storage, &mut ledger.storage);

        self.runs = next_run(self.runs);
        ledger.observer = Stamped {
            seq: self.runs,
            at: now,
            value: ObserverStatus {
                control_ticks_lost: self.control_ticks_lost,
                stream_refused: self.stream.refused(),
            },
        };
    }
}

fn publish_newest<T>(link: &mut LinkConsumer<'_, T>, topic: &mut Stamped<T>) {
    if let Some(message) = link.newest() {
        *topic = message;
    }
}

/// Messages missing between two consecutive sequences read from a link,
/// whose producer numbers from 1 and skips 0 when it wraps.
fn messages_lost(last: u32, seq: u32) -> u32 {
    let skipped_zero = u32::from(seq < last);
    seq.wrapping_sub(last)
        .wrapping_sub(1)
        .wrapping_sub(skipped_zero)
}

const fn next_run(runs: u32) -> u32 {
    match runs.wrapping_add(1) {
        0 => 1,
        next => next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_io_contracts::link::{Pushed, link};
    use heapless::spsc::Queue;

    struct Queues {
        control: Queue<Stamped<ControlTick>, 5>,
        imu: Queue<Stamped<ImuStatus>, 4>,
        drdy: Queue<Stamped<DrdyStatus>, 4>,
        rc: Queue<Stamped<RcStatus>, 4>,
        safety: Queue<Stamped<SafetyStatus>, 4>,
        output: Queue<Stamped<OutputStatus>, 4>,
        esc: Queue<Stamped<EscStatus>, 4>,
        battery: Queue<Stamped<BatteryStatus>, 4>,
        storage: Queue<Stamped<StorageStatus>, 4>,
        stream: Queue<Stamped<ControlTick>, 3>,
    }

    impl Queues {
        fn new() -> Self {
            Self {
                control: Queue::new(),
                imu: Queue::new(),
                drdy: Queue::new(),
                rc: Queue::new(),
                safety: Queue::new(),
                output: Queue::new(),
                esc: Queue::new(),
                battery: Queue::new(),
                storage: Queue::new(),
                stream: Queue::new(),
            }
        }
    }

    struct Producers<'a> {
        control: LinkProducer<'a, ControlTick>,
        imu: LinkProducer<'a, ImuStatus>,
        rc: LinkProducer<'a, RcStatus>,
        safety: LinkProducer<'a, SafetyStatus>,
        battery: LinkProducer<'a, BatteryStatus>,
    }

    fn wire(queues: &mut Queues) -> (Producers<'_>, Observer<'_>, LinkConsumer<'_, ControlTick>) {
        let (control, control_rx) = link(&mut queues.control);
        let (imu, imu_rx) = link(&mut queues.imu);
        let (_, drdy_rx) = link(&mut queues.drdy);
        let (rc, rc_rx) = link(&mut queues.rc);
        let (safety, safety_rx) = link(&mut queues.safety);
        let (_, output_rx) = link(&mut queues.output);
        let (_, esc_rx) = link(&mut queues.esc);
        let (battery, battery_rx) = link(&mut queues.battery);
        let (_, storage_rx) = link(&mut queues.storage);
        let (stream_tx, stream_rx) = link(&mut queues.stream);
        let observer = Observer::new(
            ObserverLinks {
                control: control_rx,
                imu: imu_rx,
                drdy: drdy_rx,
                rc: rc_rx,
                safety: safety_rx,
                output: output_rx,
                esc: esc_rx,
                battery: battery_rx,
                storage: storage_rx,
            },
            stream_tx,
        );
        (
            Producers {
                control,
                imu,
                rc,
                safety,
                battery,
            },
            observer,
            stream_rx,
        )
    }

    fn tick(control_seq: u32) -> ControlTick {
        ControlTick {
            control_seq,
            armed: true,
            attitude_deg: [1.0, -2.0, 3.0],
            ..ControlTick::default()
        }
    }

    #[test]
    fn every_state_link_publishes_its_newest_message_with_its_stamp() {
        let mut queues = Queues::new();
        let (mut producers, mut observer, _stream) = wire(&mut queues);
        let mut ledger = Ledger::new(Identity::default());

        let imu = |sample_seq| ImuStatus {
            sample_seq,
            gyro_raw: [1, -2, 3],
            ..ImuStatus::default()
        };
        producers.imu.push(TimestampMicros(10), imu(1));
        producers.imu.push(TimestampMicros(20), imu(2));
        producers.rc.push(
            TimestampMicros(30),
            RcStatus {
                valid: true,
                channels_us: [1500; 16],
                ..RcStatus::default()
            },
        );
        producers.battery.push(
            TimestampMicros(40),
            BatteryStatus {
                voltage_dv: 168,
                ..BatteryStatus::default()
            },
        );
        observer.run_once(TimestampMicros(100), &mut ledger);

        assert_eq!(
            ledger.imu(),
            Stamped {
                seq: 2,
                at: TimestampMicros(20),
                value: imu(2)
            }
        );
        assert!(ledger.rc().value.valid);
        assert_eq!(ledger.rc().value.channels_us, [1500; 16]);
        assert_eq!(ledger.battery().value.voltage_dv, 168);
        assert_eq!(ledger.safety().seq, 0);
    }

    #[test]
    fn a_link_that_brought_nothing_keeps_its_topic() {
        let mut queues = Queues::new();
        let (mut producers, mut observer, _stream) = wire(&mut queues);
        let mut ledger = Ledger::new(Identity::default());

        producers
            .safety
            .push(TimestampMicros(10), SafetyStatus { armed: true });
        observer.run_once(TimestampMicros(100), &mut ledger);
        observer.run_once(TimestampMicros(200), &mut ledger);

        assert_eq!(ledger.safety().seq, 1);
        assert!(ledger.safety().value.armed);
    }

    #[test]
    fn every_control_tick_reaches_the_stream_in_order_and_the_last_is_published() {
        let mut queues = Queues::new();
        let (mut producers, mut observer, mut stream) = wire(&mut queues);
        let mut ledger = Ledger::new(Identity::default());

        for control_seq in 1..=2 {
            producers.control.push(
                TimestampMicros(u64::from(control_seq) * 500),
                tick(control_seq),
            );
        }
        observer.run_once(TimestampMicros(1_000), &mut ledger);

        let first = stream.oldest().unwrap();
        let second = stream.oldest().unwrap();
        assert_eq!(
            (first.seq, first.at, first.value),
            (1, TimestampMicros(500), tick(1))
        );
        assert_eq!((second.seq, second.value.control_seq), (2, 2));
        assert_eq!(stream.oldest(), None);
        assert_eq!(ledger.control(), second);
    }

    #[test]
    fn one_run_takes_at_most_a_links_capacity() {
        let mut queues = Queues::new();
        let (mut producers, mut observer, mut stream) = wire(&mut queues);
        let mut ledger = Ledger::new(Identity::default());

        for control_seq in 1..=4 {
            assert_eq!(
                producers
                    .control
                    .push(TimestampMicros(0), tick(control_seq)),
                Pushed::Sent
            );
        }
        observer.run_once(TimestampMicros(1_000), &mut ledger);
        // The control link holds four; the stream holds two.
        assert_eq!(ledger.control().value.control_seq, 4);
        assert_eq!(stream.oldest().map(|message| message.seq), Some(1));
        assert_eq!(stream.oldest().map(|message| message.seq), Some(2));
        assert_eq!(stream.oldest(), None);
        assert_eq!(ledger.observer().value.stream_refused, 2);
    }

    #[test]
    fn lost_control_ticks_and_stream_refusals_are_counted() {
        let mut queues = Queues::new();
        let (mut producers, mut observer, mut stream) = wire(&mut queues);
        let mut ledger = Ledger::new(Identity::default());

        for control_seq in 1..=6 {
            producers
                .control
                .push(TimestampMicros(0), tick(control_seq));
        }
        observer.run_once(TimestampMicros(1_000), &mut ledger);
        while stream.oldest().is_some() {}
        producers.control.push(TimestampMicros(0), tick(7));
        observer.run_once(TimestampMicros(2_000), &mut ledger);

        // Ticks 5 and 6 were refused by the full control link.
        assert_eq!(producers.control.refused(), 2);
        assert_eq!(ledger.observer().value.control_ticks_lost, 2);
        assert_eq!(ledger.observer().value.stream_refused, 2);
        assert_eq!(ledger.control().value.control_seq, 7);
    }

    #[test]
    fn the_observer_stamps_its_own_topic_with_its_run_count() {
        let mut queues = Queues::new();
        let (_producers, mut observer, _stream) = wire(&mut queues);
        let identity = Identity {
            imu_ready: true,
            imu_kind: 2,
            flash_jedec: [0xef, 0x40, 0x18],
            flash_capacity_bytes: 16 << 20,
        };
        let mut ledger = Ledger::new(identity);

        observer.run_once(TimestampMicros(1_000), &mut ledger);
        observer.run_once(TimestampMicros(2_000), &mut ledger);

        assert_eq!(ledger.observer().seq, 2);
        assert_eq!(ledger.observer().at, TimestampMicros(2_000));
        assert_eq!(ledger.identity(), identity);
    }

    #[test]
    fn messages_lost_counts_gaps_across_the_wrap() {
        assert_eq!(messages_lost(0, 1), 0);
        assert_eq!(messages_lost(5, 8), 2);
        assert_eq!(messages_lost(u32::MAX, 1), 0);
        assert_eq!(messages_lost(u32::MAX - 1, 2), 2);
        assert_eq!(next_run(u32::MAX), 1);
    }
}
