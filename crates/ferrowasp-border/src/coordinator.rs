//! The coordinator (design §11, §14, §19): the one way in from tasks
//! outside the safety-critical core. Each requester has its own link, so a
//! requester that floods fills only its own queue; each run takes at most a
//! few requests from each, in turn. The coordinator checks a request's
//! content and who may send it, and passes what it accepts to the owner of
//! the state. The safety master decides on motor tests, which come only from
//! USB, and on a reboot, which comes only from the storage owner once it has
//! settled the store. It never judges flight state.
//!
//! The configuration's owner runs here. A change to a live key is a
//! transaction, one at a time: the safety master holds arming off, the
//! change is committed and sent to the control loop and RC input, and the
//! inhibit is released once the control loop confirms it applied it. A
//! change it does not confirm in time is refused as unconfirmed, and the
//! inhibit's lease lapses, which refuses arming until reboot. A change to a
//! boot-only key waits in the next boot's image and needs no inhibit.
//!
//! An accelerometer calibration is a transaction too, with a longer lease:
//! under the inhibit it takes each fresh sample the control loop sends, and
//! the level it finds is applied like any live change. One that fails
//! changed nothing, so its inhibit is released.
//!
//! The configuration topic is sent to the observer before the result of the
//! change that moved it, and the observer takes it first, so a requester
//! that sees its result on the ledger sees the configuration after it.
//!
//! Requirements: BORDER-012, BORDER-013, BORDER-014, BORDER-015, BORDER-016,
//! BORDER-018, CONFIG-015, CONFIG-016, CONFIG-019, CONFIG-020, CONFIG-021,
//! CONFIG-022, CONFIG-023, CONFIG-028, CONFIG-029, CONFIG-030,
//! CONFIG-031
//! Safety-critical.

use ferrowasp_boundary_contracts::config_keys::ConfigKey;
use ferrowasp_boundary_contracts::requests::{
    MotorTest, Outcome, Refusal, Request, RequestResult, Requester, RequesterResult,
};
use ferrowasp_config::boot::{BootConfig, Fallback};
use ferrowasp_config::owner::{BoardRules, Change, ConfigOwner};
use ferrowasp_config::stored::StoredConfig;
use ferrowasp_control::calibration::accel_level::{
    AccelLevelCalibration, CalibrationError, LevelSample,
};
use ferrowasp_flight_control::drone_toolbox::FlightConfig;
use ferrowasp_io_contracts::link::{LinkConsumer, LinkProducer, Pushed, Stamped};
use ferrowasp_io_contracts::time::TimestampMicros;
use ferrowasp_safety::config_gate::{CALIBRATION_INHIBIT_LEASE_MS, InhibitLease};
use ferrowasp_safety::host::HostLink;
use ferrowasp_safety::{BenchMotorRequest, SafetyEvent, SafetyReply};
use ferrowasp_serial::{LogicalSerialPort, SerialFunction};

/// Requests taken from one requester in one run.
pub const REQUESTS_PER_RUN: usize = 2;
/// Requests the safety master may hold unanswered at once.
const TICKETS: usize = 4;
/// How long a config transaction may take from its start to its release:
/// thirty coordinator runs, inside the arm inhibit's lease, so a release
/// sent in time always arrives before the lease lapses.
pub const CONFIG_TRANSACTION_MS: u64 = 300;
const _: () =
    assert!(CONFIG_TRANSACTION_MS < ferrowasp_safety::config_gate::CONFIG_INHIBIT_LEASE_MS);
/// How long a calibration may wait for its samples: 500 take half a second
/// at 1 kHz. It and the change that follows stay inside its lease.
pub const CALIBRATION_SAMPLING_MS: u64 = 3_000;
const _: () =
    assert!(CALIBRATION_SAMPLING_MS + CONFIG_TRANSACTION_MS < CALIBRATION_INHIBIT_LEASE_MS);

/// The configuration on the ledger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfigTopic {
    /// What the craft flies.
    pub active: StoredConfig,
    /// What the next boot loads, and `config save` writes.
    pub next_boot: StoredConfig,
    /// Advances with every committed change.
    pub sequence: u32,
    /// Why the boot fell back, until the pilot acknowledges it.
    pub fallback: Option<Fallback>,
}

impl ConfigTopic {
    /// The topic as the board booted.
    pub const fn boot(boot: &BootConfig) -> Self {
        Self {
            active: boot.config,
            next_boot: boot.config,
            sequence: 1,
            fallback: boot.fallback,
        }
    }
}

impl Default for ConfigTopic {
    fn default() -> Self {
        Self {
            active: StoredConfig::first_hop_default(),
            next_boot: StoredConfig::first_hop_default(),
            sequence: 0,
            fallback: None,
        }
    }
}

/// The configuration's links: to the tasks that fly it, back from the
/// control loop, and to the observer.
pub struct ConfigLinks<'a> {
    /// A `state` link: the control loop takes the newest while disarmed.
    pub control: LinkProducer<'a, FlightConfig>,
    /// A `state` link: RC input takes the newest before each frame.
    pub rc_input: LinkProducer<'a, FlightConfig>,
    /// The sequence of the last configuration the control loop applied.
    pub applied: LinkConsumer<'a, u32>,
    /// An `event` link: each fresh IMU sample, which a calibration takes.
    pub level_samples: LinkConsumer<'a, LevelSample>,
    pub topic: LinkProducer<'a, ConfigTopic>,
}

/// What the owner starts from: the boot's choice and the board's defaults.
#[derive(Clone, Copy, Debug)]
pub struct ConfigBoot {
    pub boot: BootConfig,
    pub default: StoredConfig,
    pub board: BoardRules,
}

/// A config request that passed its checks.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ConfigChange {
    Set {
        key: ConfigKey,
        value: f32,
    },
    Bind {
        port: LogicalSerialPort,
        function: SerialFunction,
    },
    Defaults,
    Calibrate,
}

/// What [`check`] found a request to be.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Checked {
    /// For the safety master to decide.
    Safety(SafetyEvent),
    /// A change for the owner.
    Config(ConfigChange),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Waiting for the safety master to grant the arm inhibit.
    Granting,
    /// A calibration, taking samples.
    Sampling(AccelLevelCalibration),
    /// Committed and sent; waiting for the control loop to confirm it.
    Confirming { applied_seq: u32 },
    /// Done, applied or changing nothing; the release, then `outcome`, wait
    /// for the safety master to take the release.
    Releasing { outcome: Outcome },
}

#[derive(Clone, Copy, Debug)]
struct Transaction {
    requester: Requester,
    request_seq: u32,
    ticket: u32,
    /// The change to commit once granted; a calibration finds its own.
    change: Option<Change>,
    until_ms: u64,
    phase: Phase,
}

#[derive(Clone, Copy, PartialEq)]
enum TicketKind {
    /// The reply is the request's result.
    Decision,
    /// The reply is the result, and an acceptance clears the fallback.
    FallbackAcknowledgement,
}

/// The consuming end of every requester's link, and of the safety master's
/// replies.
pub struct CoordinatorLinks<'a> {
    pub usb: LinkConsumer<'a, Request>,
    pub configurator_link: LinkConsumer<'a, Request>,
    pub osd: LinkConsumer<'a, Request>,
    pub storage: LinkConsumer<'a, Request>,
    pub safety_replies: LinkConsumer<'a, SafetyReply>,
}

#[derive(Clone, Copy)]
struct Ticket {
    ticket: u32,
    requester: Requester,
    request_seq: u32,
    kind: TicketKind,
}

pub struct Coordinator<'a> {
    links: CoordinatorLinks<'a>,
    results: LinkProducer<'a, RequesterResult>,
    config: ConfigLinks<'a>,
    owner: ConfigOwner,
    default_config: StoredConfig,
    fallback: Option<Fallback>,
    transaction: Option<Transaction>,
    /// The last configuration the control loop confirmed applying.
    applied_seq: u32,
    /// The topic or RC input's copy changed and has not been taken yet:
    /// both are `state` links, so a push a full link refused is sent again
    /// next run rather than lost.
    topic_due: bool,
    rc_input_due: bool,
    /// An accepted request its owner could not take yet, by requester:
    /// retried before that requester's queue is read again.
    pending: [Option<(Stamped<Request>, Checked)>; Requester::COUNT],
    tickets: [Option<Ticket>; TICKETS],
    next_ticket: u32,
    /// Which requester the next run serves first, so each leads in turn.
    first: usize,
    now_ms: u64,
}

impl<'a> Coordinator<'a> {
    /// `results` is the link to the observer, which publishes each result on
    /// its requester's topic.
    pub fn new(
        links: CoordinatorLinks<'a>,
        config: ConfigLinks<'a>,
        boot: ConfigBoot,
        results: LinkProducer<'a, RequesterResult>,
    ) -> Self {
        Self {
            links,
            results,
            config,
            owner: ConfigOwner::new(boot.boot.config, boot.board),
            default_config: boot.default,
            fallback: boot.boot.fallback,
            transaction: None,
            applied_seq: 0,
            topic_due: true,
            rc_input_due: false,
            pending: [None; Requester::COUNT],
            tickets: [None; TICKETS],
            next_ticket: 1,
            first: 0,
            now_ms: 0,
        }
    }

    /// One run at `now_ms` since boot. `send` hands an event to the safety
    /// master and says whether it was taken; one it does not take is retried
    /// next run.
    pub fn run_once(&mut self, now_ms: u64, mut send: impl FnMut(SafetyEvent) -> bool) {
        self.now_ms = now_ms;
        for _ in 0..self.links.safety_replies.capacity() {
            let Some(reply) = self.links.safety_replies.oldest() else {
                break;
            };
            self.settle(reply.value);
        }
        if let Some(applied) = self.config.applied.newest() {
            self.applied_seq = applied.value;
        }
        self.progress(&mut send);

        for offset in 0..Requester::COUNT {
            let requester = Requester::ALL[(self.first + offset) % Requester::COUNT];
            self.serve(requester, &mut send);
        }
        self.first = (self.first + 1) % Requester::COUNT;
        self.send_state();
    }

    fn serve(&mut self, requester: Requester, send: &mut impl FnMut(SafetyEvent) -> bool) {
        let mut taken = 0;
        if let Some((request, checked)) = self.pending[requester.index()] {
            if !self.act(requester, request, checked, send) {
                return;
            }
            self.pending[requester.index()] = None;
            taken += 1;
        }
        while taken < REQUESTS_PER_RUN {
            let link = match requester {
                Requester::Usb => &mut self.links.usb,
                Requester::ConfiguratorLink => &mut self.links.configurator_link,
                Requester::Osd => &mut self.links.osd,
                Requester::Storage => &mut self.links.storage,
            };
            let Some(request) = link.oldest() else {
                break;
            };
            taken += 1;
            match check(requester, request.value) {
                Ok(checked) => {
                    if !self.act(requester, request, checked, send) {
                        self.pending[requester.index()] = Some((request, checked));
                        return;
                    }
                }
                Err(refusal) => self.publish(requester, request.seq, Outcome::Refused(refusal)),
            }
        }
    }

    /// Pass a checked request to its owner; false when it must wait.
    fn act(
        &mut self,
        requester: Requester,
        request: Stamped<Request>,
        checked: Checked,
        send: &mut impl FnMut(SafetyEvent) -> bool,
    ) -> bool {
        match checked {
            Checked::Safety(event) => self.forward(requester, request, event, send),
            Checked::Config(change) => self.begin(requester, request.seq, change, send),
        }
    }

    /// Start a config change; false when one is already running or the
    /// safety master cannot take the inhibit request yet.
    fn begin(
        &mut self,
        requester: Requester,
        request_seq: u32,
        change: ConfigChange,
        send: &mut impl FnMut(SafetyEvent) -> bool,
    ) -> bool {
        if self.transaction.is_some() {
            return false;
        }
        let proposed = match change {
            ConfigChange::Set { key, value } => self.owner.propose_set(key, value),
            ConfigChange::Bind { port, function } => self.owner.propose_binding(port, function),
            ConfigChange::Defaults => self.owner.propose_replace(self.default_config),
            ConfigChange::Calibrate => {
                return self.start(
                    requester,
                    request_seq,
                    None,
                    InhibitLease::Calibration,
                    send,
                );
            }
        };
        let Ok(proposed) = proposed else {
            self.publish(requester, request_seq, Outcome::Refused(Refusal::Invalid));
            return true;
        };
        if proposed.active.is_none() {
            // Only the next boot's image changes: nothing flying moves.
            self.owner.commit(proposed);
            self.topic_due = true;
            self.send_state();
            self.publish(requester, request_seq, Outcome::Pending);
            return true;
        }
        self.start(
            requester,
            request_seq,
            Some(proposed),
            InhibitLease::Change,
            send,
        )
    }

    /// Ask for the arm inhibit that starts a transaction; false when the
    /// safety master cannot take the request yet.
    fn start(
        &mut self,
        requester: Requester,
        request_seq: u32,
        change: Option<Change>,
        lease: InhibitLease,
        send: &mut impl FnMut(SafetyEvent) -> bool,
    ) -> bool {
        let ticket = self.next_ticket;
        if !send(SafetyEvent::InhibitArming { ticket, lease }) {
            return false;
        }
        self.next_ticket = next_ticket(ticket);
        self.transaction = Some(Transaction {
            requester,
            request_seq,
            ticket,
            change,
            until_ms: self.now_ms.saturating_add(CONFIG_TRANSACTION_MS),
            phase: Phase::Granting,
        });
        true
    }

    /// Move the running transaction on: refuse it once its time is up,
    /// release the inhibit once the control loop confirms.
    fn progress(&mut self, send: &mut impl FnMut(SafetyEvent) -> bool) {
        let Some(mut transaction) = self.transaction else {
            return;
        };
        if let Phase::Sampling(calibration) = transaction.phase {
            self.sample(transaction, calibration);
            let Some(sampled) = self.transaction else {
                return;
            };
            transaction = sampled;
        }
        if let Phase::Confirming { applied_seq } = transaction.phase
            && self.applied_seq == applied_seq
        {
            transaction.phase = Phase::Releasing {
                outcome: Outcome::Accepted,
            };
        }
        if self.now_ms >= transaction.until_ms {
            // Unreleased, the inhibit's lease lapses and arming stays
            // refused until reboot: the change may be half applied.
            // A grant whose reply never came holds the inhibit all the same.
            self.transaction = None;
            self.publish(
                transaction.requester,
                transaction.request_seq,
                Outcome::Refused(Refusal::NotConfirmed),
            );
            return;
        }
        if let Phase::Releasing { outcome } = transaction.phase {
            if !send(SafetyEvent::ReleaseInhibit {
                ticket: transaction.ticket,
            }) {
                self.transaction = Some(transaction);
                return;
            }
            self.transaction = None;
            self.publish(transaction.requester, transaction.request_seq, outcome);
            return;
        }
        self.transaction = Some(transaction);
    }

    /// The safety master granted or refused a transaction's inhibit.
    fn granted(&mut self, mut transaction: Transaction, outcome: Outcome) {
        if outcome != Outcome::Accepted {
            self.transaction = None;
            self.publish(transaction.requester, transaction.request_seq, outcome);
            return;
        }
        let Some(change) = transaction.change else {
            // A calibration: samples from before the grant may come from a
            // craft that was still being set down.
            for _ in 0..self.config.level_samples.capacity() {
                if self.config.level_samples.oldest().is_none() {
                    break;
                }
            }
            transaction.phase = Phase::Sampling(AccelLevelCalibration::new());
            transaction.until_ms = self.now_ms.saturating_add(CALIBRATION_SAMPLING_MS);
            self.transaction = Some(transaction);
            return;
        };
        self.apply(transaction, change);
    }

    /// Feed a calibration this run's samples; once it has a level, apply it
    /// like any live change, and once it fails, release what changed nothing.
    fn sample(&mut self, mut transaction: Transaction, mut calibration: AccelLevelCalibration) {
        let mut finished = None;
        for _ in 0..self.config.level_samples.capacity() {
            let Some(sample) = self.config.level_samples.oldest() else {
                break;
            };
            finished = calibration.push(sample.value.gravity_g, sample.value.gyro_dps);
            if finished.is_some() {
                break;
            }
        }
        let refusal = match finished {
            None if self.now_ms < transaction.until_ms => {
                transaction.phase = Phase::Sampling(calibration);
                self.transaction = Some(transaction);
                return;
            }
            None => Refusal::CalibrationNoSamples,
            Some(Ok(level)) => {
                transaction.until_ms = self.now_ms.saturating_add(CONFIG_TRANSACTION_MS);
                let change = self.owner.propose_level(level);
                self.apply(transaction, change);
                return;
            }
            Some(Err(CalibrationError::Moving)) => Refusal::CalibrationMoving,
            Some(Err(CalibrationError::NotLevel)) => Refusal::CalibrationNotLevel,
            Some(Err(CalibrationError::BadGravity)) => Refusal::CalibrationBadGravity,
        };
        transaction.until_ms = self.now_ms.saturating_add(CONFIG_TRANSACTION_MS);
        transaction.phase = Phase::Releasing {
            outcome: Outcome::Refused(refusal),
        };
        self.transaction = Some(transaction);
    }

    /// Commit a granted change and send it to the tasks that fly it.
    fn apply(&mut self, mut transaction: Transaction, change: Change) {
        self.owner.commit(change);
        self.topic_due = true;
        self.rc_input_due = true;
        let at = self.at();
        // A push the control loop's link refuses is never confirmed, so the
        // transaction ends unconfirmed.
        self.config.control.push(at, self.owner.active().flight());
        transaction.phase = Phase::Confirming {
            applied_seq: self.config.control.sequence(),
        };
        self.transaction = Some(transaction);
        self.send_state();
    }

    /// Send the topic and RC input's copy where they changed.
    fn send_state(&mut self) {
        let at = self.at();
        if self.rc_input_due {
            self.rc_input_due =
                self.config.rc_input.push(at, self.owner.active().flight()) == Pushed::Refused;
        }
        if self.topic_due {
            let topic = ConfigTopic {
                active: self.owner.active(),
                next_boot: self.owner.next_boot(),
                sequence: self.owner.sequence(),
                fallback: self.fallback,
            };
            self.topic_due = self.config.topic.push(at, topic) == Pushed::Refused;
        }
    }

    const fn at(&self) -> TimestampMicros {
        TimestampMicros(self.now_ms.saturating_mul(1_000))
    }

    /// Hand `event` to the safety master; false when it was not taken, or
    /// no ticket is free for a decision the requester waits for.
    fn forward(
        &mut self,
        requester: Requester,
        request: Stamped<Request>,
        event: SafetyEvent,
        send: &mut impl FnMut(SafetyEvent) -> bool,
    ) -> bool {
        let ticketed = |ticket| match event {
            SafetyEvent::BenchMotor { request, .. } => Some((
                SafetyEvent::BenchMotor { request, ticket },
                TicketKind::Decision,
            )),
            SafetyEvent::Reboot { target, .. } => {
                Some((SafetyEvent::Reboot { target, ticket }, TicketKind::Decision))
            }
            SafetyEvent::AcknowledgeConfigFallback { .. } => Some((
                SafetyEvent::AcknowledgeConfigFallback { ticket },
                TicketKind::FallbackAcknowledgement,
            )),
            _ => None,
        };
        let Some((with_ticket, kind)) = ticketed(self.next_ticket) else {
            // A host renewal publishes no result: it arrives several times a
            // second and only extends a lease.
            return send(event);
        };
        let Some(slot) = self.tickets.iter().position(Option::is_none) else {
            return false;
        };
        if !send(with_ticket) {
            return false;
        }
        self.tickets[slot] = Some(Ticket {
            ticket: self.next_ticket,
            requester,
            request_seq: request.seq,
            kind,
        });
        self.next_ticket = next_ticket(self.next_ticket);
        true
    }

    fn settle(&mut self, reply: SafetyReply) {
        if let Some(transaction) = self.transaction
            && transaction.ticket == reply.ticket
            && transaction.phase == Phase::Granting
        {
            self.granted(transaction, reply.outcome);
            return;
        }
        let Some(slot) = self
            .tickets
            .iter()
            .position(|ticket| ticket.is_some_and(|ticket| ticket.ticket == reply.ticket))
        else {
            return;
        };
        let Some(ticket) = self.tickets[slot].take() else {
            return;
        };
        if ticket.kind == TicketKind::FallbackAcknowledgement && reply.outcome == Outcome::Accepted
        {
            self.fallback = None;
            self.topic_due = true;
            self.send_state();
        }
        self.publish(ticket.requester, ticket.request_seq, reply.outcome);
    }

    fn publish(&mut self, requester: Requester, request_seq: u32, outcome: Outcome) {
        self.results.push(
            self.at(),
            RequesterResult {
                requester,
                result: RequestResult {
                    request_seq,
                    outcome,
                },
            },
        );
    }
}

const fn next_ticket(ticket: u32) -> u32 {
    match ticket.wrapping_add(1) {
        0 => 1,
        next => next,
    }
}

/// The content and permission checks: what the coordinator decides before
/// the owner sees a request.
fn check(requester: Requester, request: Request) -> Result<Checked, Refusal> {
    // The storage owner speaks for both command lines on configuration; the
    // OSD's menu sets values.
    let config = |change| match requester {
        Requester::Storage => Ok(Checked::Config(change)),
        _ => Err(Refusal::NotPermitted),
    };
    match request {
        Request::HostPresent => match requester {
            Requester::Usb => Ok(Checked::Safety(SafetyEvent::HostPresent(HostLink::Usb))),
            Requester::ConfiguratorLink => Ok(Checked::Safety(SafetyEvent::HostPresent(
                HostLink::ConfiguratorLink,
            ))),
            Requester::Osd | Requester::Storage => Err(Refusal::NotPermitted),
        },
        // Only the storage owner asks for a reboot, once the store is
        // settled; the command lines ask it, not the coordinator.
        Request::Reboot(target) => match requester {
            Requester::Storage => Ok(Checked::Safety(SafetyEvent::Reboot { target, ticket: 0 })),
            _ => Err(Refusal::NotPermitted),
        },
        Request::ConfigSet { key, value } => match requester {
            Requester::Storage | Requester::Osd => {
                Ok(Checked::Config(ConfigChange::Set { key, value }))
            }
            Requester::Usb | Requester::ConfiguratorLink => Err(Refusal::NotPermitted),
        },
        Request::ConfigBind { port, function } => config(ConfigChange::Bind { port, function }),
        Request::ConfigDefaults => config(ConfigChange::Defaults),
        Request::CalibrateAccel => config(ConfigChange::Calibrate),
        Request::AcknowledgeConfigFallback => match requester {
            Requester::Storage => Ok(Checked::Safety(SafetyEvent::AcknowledgeConfigFallback {
                ticket: 0,
            })),
            _ => Err(Refusal::NotPermitted),
        },
        Request::MotorTest(test) => {
            // Only the USB command line carries motor tests.
            if requester != Requester::Usb {
                return Err(Refusal::NotPermitted);
            }
            let request = match test {
                MotorTest::Stop => BenchMotorRequest::Stop,
                MotorTest::Spin { logical_motor } => {
                    valid_motor(logical_motor)?;
                    BenchMotorRequest::Spin { logical_motor }
                }
                MotorTest::Direction {
                    logical_motor,
                    reversed,
                } => {
                    valid_motor(logical_motor)?;
                    BenchMotorRequest::Direction {
                        logical_motor,
                        reversed,
                    }
                }
            };
            // The ticket is filled in when the request is forwarded.
            Ok(Checked::Safety(SafetyEvent::BenchMotor {
                request,
                ticket: 0,
            }))
        }
    }
}

const fn valid_motor(logical_motor: u8) -> Result<(), Refusal> {
    if logical_motor >= 1 && logical_motor <= 4 {
        Ok(())
    } else {
        Err(Refusal::Invalid)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use ferrowasp_io_contracts::link::link;
    use heapless::spsc::Queue;
    use std::vec::Vec;

    struct Queues {
        usb: Queue<Stamped<Request>, 5>,
        configurator_link: Queue<Stamped<Request>, 5>,
        osd: Queue<Stamped<Request>, 5>,
        storage: Queue<Stamped<Request>, 5>,
        replies: Queue<Stamped<SafetyReply>, 5>,
        results: Queue<Stamped<RequesterResult>, 9>,
        control: Queue<Stamped<FlightConfig>, 3>,
        rc_input: Queue<Stamped<FlightConfig>, 3>,
        applied: Queue<Stamped<u32>, 3>,
        topic: Queue<Stamped<ConfigTopic>, 3>,
        level_samples: Queue<Stamped<LevelSample>, 65>,
    }

    impl Queues {
        fn new() -> Self {
            Self {
                usb: Queue::new(),
                configurator_link: Queue::new(),
                osd: Queue::new(),
                storage: Queue::new(),
                replies: Queue::new(),
                results: Queue::new(),
                control: Queue::new(),
                rc_input: Queue::new(),
                applied: Queue::new(),
                topic: Queue::new(),
                level_samples: Queue::new(),
            }
        }
    }

    struct Ends<'a> {
        usb: LinkProducer<'a, Request>,
        configurator_link: LinkProducer<'a, Request>,
        osd: LinkProducer<'a, Request>,
        storage: LinkProducer<'a, Request>,
        replies: LinkProducer<'a, SafetyReply>,
        results: LinkConsumer<'a, RequesterResult>,
        control: LinkConsumer<'a, FlightConfig>,
        rc_input: LinkConsumer<'a, FlightConfig>,
        applied: LinkProducer<'a, u32>,
        topic: LinkConsumer<'a, ConfigTopic>,
        level_samples: LinkProducer<'a, LevelSample>,
    }

    /// UART2 and UART3, both taking either receiver protocol; RC on UART2.
    const BOARD: BoardRules = {
        use ferrowasp_serial::{SerialBindings, SerialCapabilities, SerialProfile, SerialRoute};
        const fn route(logical: LogicalSerialPort) -> SerialRoute {
            SerialRoute {
                logical,
                peripheral: "test",
                tx_pin: "test",
                rx_pin: "test",
                profile: SerialProfile::disabled(),
                rx_dma: "test",
                tx_dma: Some("test"),
                capabilities: SerialCapabilities {
                    sbus: true,
                    crsf: true,
                    mavlink: false,
                    msp: true,
                    esc_telemetry: true,
                    cli: true,
                    tx: true,
                },
            }
        }
        BoardRules {
            routes: &[
                route(LogicalSerialPort::Uart2),
                route(LogicalSerialPort::Uart3),
            ],
            default_bindings: SerialBindings::none()
                .with(LogicalSerialPort::Uart2, SerialFunction::RcInput),
        }
    };

    fn wire(queues: &mut Queues) -> (Coordinator<'_>, Ends<'_>) {
        wire_from(queues, None)
    }

    fn wire_from(queues: &mut Queues, fallback: Option<Fallback>) -> (Coordinator<'_>, Ends<'_>) {
        let (usb, usb_rx) = link(&mut queues.usb);
        let (configurator_link, configurator_rx) = link(&mut queues.configurator_link);
        let (osd, osd_rx) = link(&mut queues.osd);
        let (storage, storage_rx) = link(&mut queues.storage);
        let (replies, replies_rx) = link(&mut queues.replies);
        let (results_tx, results) = link(&mut queues.results);
        let (control_tx, control) = link(&mut queues.control);
        let (rc_input_tx, rc_input) = link(&mut queues.rc_input);
        let (applied, applied_rx) = link(&mut queues.applied);
        let (topic_tx, topic) = link(&mut queues.topic);
        let (level_samples, level_samples_rx) = link(&mut queues.level_samples);
        (
            Coordinator::new(
                CoordinatorLinks {
                    usb: usb_rx,
                    configurator_link: configurator_rx,
                    osd: osd_rx,
                    storage: storage_rx,
                    safety_replies: replies_rx,
                },
                ConfigLinks {
                    control: control_tx,
                    rc_input: rc_input_tx,
                    applied: applied_rx,
                    level_samples: level_samples_rx,
                    topic: topic_tx,
                },
                ConfigBoot {
                    boot: BootConfig {
                        config: StoredConfig::first_hop_default(),
                        sequence: 0,
                        slot: 1,
                        fallback,
                    },
                    default: StoredConfig::first_hop_default(),
                    board: BOARD,
                },
                results_tx,
            ),
            Ends {
                usb,
                configurator_link,
                osd,
                storage,
                replies,
                results,
                control,
                rc_input,
                applied,
                topic,
                level_samples,
            },
        )
    }

    fn results(ends: &mut Ends<'_>) -> Vec<RequesterResult> {
        core::iter::from_fn(|| ends.results.oldest().map(|result| result.value)).collect()
    }

    fn spin(logical_motor: u8) -> Request {
        Request::MotorTest(MotorTest::Spin { logical_motor })
    }

    #[test]
    fn a_motor_test_reaches_the_safety_master_and_its_decision_comes_back() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let mut sent = Vec::new();

        ends.usb.push(TimestampMicros(0), spin(2));
        coordinator.run_once(10, |event| {
            sent.push(event);
            true
        });
        let SafetyEvent::BenchMotor { request, ticket } = sent[0] else {
            panic!("expected a bench request, got {:?}", sent[0]);
        };
        assert_eq!(request, BenchMotorRequest::Spin { logical_motor: 2 });
        assert!(
            results(&mut ends).is_empty(),
            "no result before the decision"
        );

        ends.replies.push(
            TimestampMicros(0),
            SafetyReply {
                ticket,
                outcome: Outcome::Refused(Refusal::Armed),
            },
        );
        coordinator.run_once(11, |_| true);
        assert_eq!(
            results(&mut ends),
            [RequesterResult {
                requester: Requester::Usb,
                result: RequestResult {
                    request_seq: 1,
                    outcome: Outcome::Refused(Refusal::Armed),
                },
            }]
        );
    }

    #[test]
    fn content_and_permission_are_refused_before_the_safety_master_sees_them() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let mut sent = Vec::new();

        ends.usb.push(TimestampMicros(0), spin(5));
        ends.osd.push(TimestampMicros(0), spin(1));
        ends.configurator_link
            .push(TimestampMicros(0), Request::MotorTest(MotorTest::Stop));
        ends.osd.push(TimestampMicros(0), Request::HostPresent);
        coordinator.run_once(10, |event| {
            sent.push(event);
            true
        });

        assert!(sent.is_empty());
        let all = results(&mut ends);
        assert!(all.contains(&RequesterResult {
            requester: Requester::Usb,
            result: RequestResult {
                request_seq: 1,
                outcome: Outcome::Refused(Refusal::Invalid),
            },
        }));
        assert!(all.contains(&RequesterResult {
            requester: Requester::Osd,
            result: RequestResult {
                request_seq: 1,
                outcome: Outcome::Refused(Refusal::NotPermitted),
            },
        }));
        assert!(all.contains(&RequesterResult {
            requester: Requester::Osd,
            result: RequestResult {
                request_seq: 2,
                outcome: Outcome::Refused(Refusal::NotPermitted),
            },
        }));
        assert!(all.contains(&RequesterResult {
            requester: Requester::ConfiguratorLink,
            result: RequestResult {
                request_seq: 1,
                outcome: Outcome::Refused(Refusal::NotPermitted),
            },
        }));
    }

    #[test]
    fn a_host_renewal_names_its_link_and_publishes_nothing() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let mut sent = Vec::new();

        ends.usb.push(TimestampMicros(0), Request::HostPresent);
        ends.configurator_link
            .push(TimestampMicros(0), Request::HostPresent);
        coordinator.run_once(10, |event| {
            sent.push(event);
            true
        });

        assert!(sent.contains(&SafetyEvent::HostPresent(HostLink::Usb)));
        assert!(sent.contains(&SafetyEvent::HostPresent(HostLink::ConfiguratorLink)));
        assert!(results(&mut ends).is_empty());
    }

    #[test]
    fn a_request_the_safety_master_cannot_take_waits_and_holds_its_queue() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);

        ends.usb.push(TimestampMicros(0), spin(1));
        ends.usb.push(TimestampMicros(0), spin(2));
        let mut offered = Vec::new();
        coordinator.run_once(10, |event| {
            offered.push(event);
            false
        });
        assert_eq!(offered.len(), 1, "the second waits behind the first");

        let mut sent = Vec::new();
        coordinator.run_once(11, |event| {
            sent.push(event);
            true
        });
        let motors: Vec<_> = sent
            .iter()
            .map(|event| match event {
                SafetyEvent::BenchMotor {
                    request: BenchMotorRequest::Spin { logical_motor },
                    ..
                } => *logical_motor,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(motors, [1, 2], "in order, after the retry");
    }

    #[test]
    fn one_run_takes_a_bounded_number_from_each_requester_in_turn() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);

        for _ in 0..4 {
            ends.usb.push(TimestampMicros(0), Request::HostPresent);
        }
        ends.configurator_link
            .push(TimestampMicros(0), Request::HostPresent);
        let mut sent = Vec::new();
        coordinator.run_once(10, |event| {
            sent.push(event);
            true
        });

        let usb = sent
            .iter()
            .filter(|event| **event == SafetyEvent::HostPresent(HostLink::Usb))
            .count();
        assert_eq!(
            usb, REQUESTS_PER_RUN,
            "a flood of USB requests waits its turn"
        );
        assert!(sent.contains(&SafetyEvent::HostPresent(HostLink::ConfiguratorLink)));
    }

    #[test]
    fn a_reboot_comes_only_from_the_storage_owner_and_its_decision_comes_back() {
        use ferrowasp_boundary_contracts::requests::RebootTarget;
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let mut sent = Vec::new();

        let reboot = Request::Reboot(RebootTarget::RomBootloader);
        ends.usb.push(TimestampMicros(0), reboot);
        ends.configurator_link.push(TimestampMicros(0), reboot);
        ends.osd.push(TimestampMicros(0), reboot);
        ends.storage.push(TimestampMicros(0), reboot);
        coordinator.run_once(10, |event| {
            sent.push(event);
            true
        });

        let [SafetyEvent::Reboot { target, ticket }] = sent[..] else {
            panic!("expected one reboot, got {sent:?}");
        };
        assert_eq!(target, RebootTarget::RomBootloader);
        let refused = results(&mut ends);
        assert_eq!(refused.len(), 3);
        assert!(
            refused
                .iter()
                .all(|result| result.requester != Requester::Storage
                    && result.result.outcome == Outcome::Refused(Refusal::NotPermitted))
        );

        ends.replies.push(
            TimestampMicros(0),
            SafetyReply {
                ticket,
                outcome: Outcome::Refused(Refusal::ArmingInProgress),
            },
        );
        coordinator.run_once(11, |_| true);
        assert_eq!(
            results(&mut ends),
            [RequesterResult {
                requester: Requester::Storage,
                result: RequestResult {
                    request_seq: 1,
                    outcome: Outcome::Refused(Refusal::ArmingInProgress),
                },
            }]
        );
    }

    fn roll_p(value: f32) -> Request {
        Request::ConfigSet {
            key: ConfigKey::RollP,
            value,
        }
    }

    fn reply(ends: &mut Ends<'_>, ticket: u32, outcome: Outcome) {
        ends.replies
            .push(TimestampMicros(0), SafetyReply { ticket, outcome });
    }

    fn storage_outcomes(ends: &mut Ends<'_>) -> Vec<Outcome> {
        results(ends)
            .into_iter()
            .filter(|result| result.requester == Requester::Storage)
            .map(|result| result.result.outcome)
            .collect()
    }

    /// Run once, sending everything, and return what reached the safety
    /// master.
    fn run(coordinator: &mut Coordinator<'_>, now_ms: u64) -> Vec<SafetyEvent> {
        let mut sent = Vec::new();
        coordinator.run_once(now_ms, |event| {
            sent.push(event);
            true
        });
        sent
    }

    #[test]
    fn the_boot_configuration_is_published_on_the_first_run() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire_from(&mut queues, Some(Fallback::Damaged));
        run(&mut coordinator, 10);
        let topic = ends.topic.newest().unwrap().value;
        assert_eq!(topic.active, StoredConfig::first_hop_default());
        assert_eq!(topic.sequence, 1);
        assert_eq!(topic.fallback, Some(Fallback::Damaged));
    }

    #[test]
    fn a_live_change_holds_arming_off_until_the_control_loop_applies_it() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        ends.topic.newest();

        ends.storage.push(TimestampMicros(0), roll_p(0.07));
        let [SafetyEvent::InhibitArming { ticket, .. }] = run(&mut coordinator, 10)[..] else {
            panic!("expected an inhibit request");
        };
        assert!(
            ends.control.newest().is_none(),
            "nothing applies before the grant"
        );

        reply(&mut ends, ticket, Outcome::Accepted);
        assert!(run(&mut coordinator, 20).is_empty());
        let sent = ends.control.newest().unwrap();
        assert_eq!(sent.value.tuning.rate_gains.roll.p, 0.07);
        assert_eq!(ends.rc_input.newest().unwrap().value, sent.value);
        let topic = ends.topic.newest().unwrap().value;
        assert_eq!(topic.active.tuning.rate_gains.roll.p, 0.07);
        assert_eq!(topic.next_boot.tuning.rate_gains.roll.p, 0.07);
        assert!(
            storage_outcomes(&mut ends).is_empty(),
            "not until confirmed"
        );

        ends.applied.push(TimestampMicros(0), sent.seq);
        assert_eq!(
            run(&mut coordinator, 30),
            [SafetyEvent::ReleaseInhibit { ticket }]
        );
        assert_eq!(storage_outcomes(&mut ends), [Outcome::Accepted]);
    }

    #[test]
    fn a_change_the_control_loop_never_confirms_is_refused_and_not_released() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        ends.storage.push(TimestampMicros(0), roll_p(0.07));
        let [SafetyEvent::InhibitArming { ticket, .. }] = run(&mut coordinator, 10)[..] else {
            panic!("expected an inhibit request");
        };
        reply(&mut ends, ticket, Outcome::Accepted);
        run(&mut coordinator, 20);

        let sent = run(&mut coordinator, 10 + CONFIG_TRANSACTION_MS);
        assert!(
            !sent.contains(&SafetyEvent::ReleaseInhibit { ticket }),
            "the lease lapses, so arming stays refused until reboot"
        );
        assert_eq!(
            storage_outcomes(&mut ends),
            [Outcome::Refused(Refusal::NotConfirmed)]
        );
    }

    #[test]
    fn a_refused_inhibit_changes_nothing() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        run(&mut coordinator, 0);
        ends.topic.newest();
        ends.storage.push(TimestampMicros(0), roll_p(0.07));
        let [SafetyEvent::InhibitArming { ticket, .. }] = run(&mut coordinator, 10)[..] else {
            panic!("expected an inhibit request");
        };
        reply(&mut ends, ticket, Outcome::Refused(Refusal::Armed));
        run(&mut coordinator, 20);
        assert_eq!(
            storage_outcomes(&mut ends),
            [Outcome::Refused(Refusal::Armed)]
        );
        assert!(ends.control.newest().is_none());
        assert!(
            ends.topic.newest().is_none(),
            "the configuration did not move"
        );
    }

    #[test]
    fn a_value_out_of_range_is_refused_without_asking_the_safety_master() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        ends.storage.push(TimestampMicros(0), roll_p(f32::NAN));
        assert!(run(&mut coordinator, 10).is_empty());
        assert_eq!(
            storage_outcomes(&mut ends),
            [Outcome::Refused(Refusal::Invalid)]
        );
    }

    #[test]
    fn a_boot_only_change_waits_for_a_reboot_without_an_inhibit() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        run(&mut coordinator, 0);
        ends.topic.newest();
        ends.storage.push(
            TimestampMicros(0),
            Request::ConfigBind {
                port: LogicalSerialPort::Uart3,
                function: SerialFunction::RcInput,
            },
        );
        assert!(run(&mut coordinator, 10).is_empty());
        assert_eq!(storage_outcomes(&mut ends), [Outcome::Pending]);
        let topic = ends.topic.newest().unwrap().value;
        assert_eq!(topic.active.serial_bindings, None);
        assert_eq!(
            topic
                .next_boot
                .serial_bindings
                .unwrap()
                .function(LogicalSerialPort::Uart3),
            SerialFunction::RcInput
        );
        assert!(ends.control.newest().is_none(), "nothing flying moves");
    }

    #[test]
    fn a_binding_the_board_refuses_is_invalid() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        ends.storage.push(
            TimestampMicros(0),
            Request::ConfigBind {
                port: LogicalSerialPort::Uart2,
                function: SerialFunction::None,
            },
        );
        assert!(run(&mut coordinator, 10).is_empty());
        assert_eq!(
            storage_outcomes(&mut ends),
            [Outcome::Refused(Refusal::Invalid)],
            "RC stays bound"
        );
    }

    #[test]
    fn config_changes_run_one_at_a_time_and_the_next_waits_its_turn() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        ends.storage.push(TimestampMicros(0), roll_p(0.07));
        ends.osd.push(TimestampMicros(0), roll_p(0.08));
        let sent = run(&mut coordinator, 10);
        assert_eq!(sent.len(), 1, "the second waits: {sent:?}");
        let SafetyEvent::InhibitArming { ticket, .. } = sent[0] else {
            panic!("expected an inhibit request");
        };
        reply(&mut ends, ticket, Outcome::Accepted);
        run(&mut coordinator, 20);
        let applied = ends.control.newest().unwrap();
        ends.applied.push(TimestampMicros(0), applied.seq);
        let sent = run(&mut coordinator, 30);
        assert!(sent.contains(&SafetyEvent::ReleaseInhibit { ticket }));
        assert!(
            sent.iter()
                .any(|event| matches!(event, SafetyEvent::InhibitArming { ticket: next, .. } if *next != ticket)),
            "the waiting change starts once the first is released: {sent:?}"
        );
    }

    #[test]
    fn only_the_storage_owner_and_the_osd_change_the_configuration() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        ends.usb.push(TimestampMicros(0), roll_p(0.07));
        ends.configurator_link
            .push(TimestampMicros(0), roll_p(0.07));
        ends.osd.push(TimestampMicros(0), Request::ConfigDefaults);
        ends.osd
            .push(TimestampMicros(0), Request::AcknowledgeConfigFallback);
        assert!(run(&mut coordinator, 10).is_empty());
        let all = results(&mut ends);
        assert_eq!(all.len(), 4);
        assert!(
            all.iter()
                .all(|result| result.result.outcome == Outcome::Refused(Refusal::NotPermitted))
        );
    }

    #[test]
    fn an_acknowledged_fallback_leaves_the_topic() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire_from(&mut queues, Some(Fallback::Invalid));
        run(&mut coordinator, 0);
        ends.topic.newest();
        ends.storage
            .push(TimestampMicros(0), Request::AcknowledgeConfigFallback);
        let [SafetyEvent::AcknowledgeConfigFallback { ticket }] = run(&mut coordinator, 10)[..]
        else {
            panic!("expected the acknowledgement");
        };
        reply(&mut ends, ticket, Outcome::Accepted);
        run(&mut coordinator, 20);
        assert_eq!(ends.topic.newest().unwrap().value.fallback, None);
        assert_eq!(storage_outcomes(&mut ends), [Outcome::Accepted]);
    }

    #[test]
    fn rc_input_gets_the_newest_change_once_its_full_link_has_room() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let mut now = 0;
        for value in [0.05, 0.06, 0.07] {
            ends.storage.push(TimestampMicros(0), roll_p(value));
            now += 10;
            let sent = run(&mut coordinator, now);
            let Some(SafetyEvent::InhibitArming { ticket, .. }) = sent.first().copied() else {
                panic!("expected an inhibit request: {sent:?}");
            };
            reply(&mut ends, ticket, Outcome::Accepted);
            now += 10;
            run(&mut coordinator, now);
            let applied = ends.control.newest().unwrap();
            ends.applied.push(TimestampMicros(0), applied.seq);
            now += 10;
            run(&mut coordinator, now);
        }
        // RC input read nothing meanwhile: its link took two, refused the
        // third, and gets it once it reads.
        let mut newest = ends.rc_input.newest().unwrap().value;
        assert_eq!(newest.tuning.rate_gains.roll.p, 0.06);
        run(&mut coordinator, now + 10);
        newest = ends.rc_input.newest().unwrap().value;
        assert_eq!(newest.tuning.rate_gains.roll.p, 0.07);
    }

    /// Start a calibration and grant its inhibit; its ticket.
    fn granted_calibration(coordinator: &mut Coordinator<'_>, ends: &mut Ends<'_>) -> u32 {
        // Left from before the grant, from a craft still being set down.
        ends.level_samples.push(
            TimestampMicros(0),
            LevelSample {
                gravity_g: [0.5, 0.0, 0.8],
                gyro_dps: [90.0; 3],
            },
        );
        ends.storage
            .push(TimestampMicros(0), Request::CalibrateAccel);
        let [
            SafetyEvent::InhibitArming {
                ticket,
                lease: InhibitLease::Calibration,
            },
        ] = run(coordinator, 10)[..]
        else {
            panic!("expected a calibration's inhibit request");
        };
        reply(ends, ticket, Outcome::Accepted);
        run(coordinator, 20);
        ticket
    }

    fn still(gravity_g: [f32; 3]) -> LevelSample {
        LevelSample {
            gravity_g,
            gyro_dps: [0.5, -0.2, 0.1],
        }
    }

    #[test]
    fn a_calibration_samples_under_the_inhibit_and_applies_its_level() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let ticket = granted_calibration(&mut coordinator, &mut ends);
        let mut now = 20;
        let mut fed = 0;
        let mut applied = false;
        while !applied && fed < 600 {
            for _ in 0..40 {
                ends.level_samples
                    .push(TimestampMicros(0), still([0.02, 0.0, 0.999]));
                fed += 1;
            }
            now += 10;
            assert!(
                !run(&mut coordinator, now).contains(&SafetyEvent::ReleaseInhibit { ticket }),
                "held until the level is applied"
            );
            if let Some(sent) = ends.control.newest() {
                assert_eq!(sent.value.accel_level.offset_mg, [20, 0, -1]);
                ends.applied.push(TimestampMicros(0), sent.seq);
                applied = true;
            }
        }
        assert!(applied, "500 still samples give a level");
        assert!(storage_outcomes(&mut ends).is_empty());
        now += 10;
        assert!(run(&mut coordinator, now).contains(&SafetyEvent::ReleaseInhibit { ticket }));
        assert_eq!(storage_outcomes(&mut ends), [Outcome::Accepted]);
        let topic = ends.topic.newest().unwrap().value;
        assert_eq!(topic.active.accel_level.offset_mg, [20, 0, -1]);
        assert_eq!(topic.next_boot.accel_level.offset_mg, [20, 0, -1]);
    }

    #[test]
    fn a_failed_calibration_changes_nothing_and_releases_its_inhibit() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let ticket = granted_calibration(&mut coordinator, &mut ends);
        let mut now = 20;
        let mut sent = Vec::new();
        for round in 0..20 {
            for sample in 0..40 {
                let wobble = if (round * 40 + sample) % 2 == 0 {
                    0.1
                } else {
                    0.0
                };
                ends.level_samples
                    .push(TimestampMicros(0), still([wobble, 0.0, 1.0]));
            }
            now += 10;
            sent = run(&mut coordinator, now);
            if !sent.is_empty() {
                break;
            }
        }
        assert_eq!(sent, [SafetyEvent::ReleaseInhibit { ticket }]);
        assert_eq!(
            storage_outcomes(&mut ends),
            [Outcome::Refused(Refusal::CalibrationMoving)]
        );
        assert!(ends.control.newest().is_none(), "nothing applied");
    }

    #[test]
    fn a_calibration_without_samples_gives_up_and_releases_its_inhibit() {
        let mut queues = Queues::new();
        let (mut coordinator, mut ends) = wire(&mut queues);
        let ticket = granted_calibration(&mut coordinator, &mut ends);
        assert!(run(&mut coordinator, 20 + CALIBRATION_SAMPLING_MS - 10).is_empty());
        assert_eq!(
            run(&mut coordinator, 20 + CALIBRATION_SAMPLING_MS),
            [SafetyEvent::ReleaseInhibit { ticket }]
        );
        assert_eq!(
            storage_outcomes(&mut ends),
            [Outcome::Refused(Refusal::CalibrationNoSamples)]
        );
    }
}
