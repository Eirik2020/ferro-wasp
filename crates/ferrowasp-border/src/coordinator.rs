//! The coordinator (design §11, §19): the one way in from tasks outside the
//! safety-critical core. Each requester has its own link, so a requester
//! that floods fills only its own queue; each run takes at most a few
//! requests from each, in turn. The coordinator checks a request's content
//! and who may send it, and passes what it accepts to the owner of the
//! state, the safety master, which decides. Motor tests come only from USB,
//! so there is no bench session to arbitrate yet, and a reboot only from the
//! storage owner, once it has settled the store. It never applies anything
//! itself, and never judges flight state.
//!
//! Requirements: BORDER-012, BORDER-013, BORDER-014, BORDER-015, BORDER-016,
//! BORDER-018
//! Safety-critical.

use ferrowasp_boundary_contracts::requests::{
    MotorTest, Outcome, Refusal, Request, RequestResult, Requester, RequesterResult,
};
use ferrowasp_io_contracts::link::{LinkConsumer, LinkProducer, Stamped};
use ferrowasp_io_contracts::time::TimestampMicros;
use ferrowasp_safety::host::HostLink;
use ferrowasp_safety::{BenchMotorRequest, SafetyEvent, SafetyReply};

/// Requests taken from one requester in one run.
pub const REQUESTS_PER_RUN: usize = 2;
/// Requests the safety master may hold unanswered at once.
const TICKETS: usize = 4;

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
}

pub struct Coordinator<'a> {
    links: CoordinatorLinks<'a>,
    results: LinkProducer<'a, RequesterResult>,
    /// An accepted request the safety master could not take yet, by
    /// requester: retried before that requester's queue is read again.
    pending: [Option<(Stamped<Request>, SafetyEvent)>; Requester::COUNT],
    tickets: [Option<Ticket>; TICKETS],
    next_ticket: u32,
    /// Which requester the next run serves first, so each leads in turn.
    first: usize,
    now_ms: u64,
}

impl<'a> Coordinator<'a> {
    /// `results` is the link to the observer, which publishes each result on
    /// its requester's topic.
    pub fn new(links: CoordinatorLinks<'a>, results: LinkProducer<'a, RequesterResult>) -> Self {
        Self {
            links,
            results,
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

        for offset in 0..Requester::COUNT {
            let requester = Requester::ALL[(self.first + offset) % Requester::COUNT];
            self.serve(requester, &mut send);
        }
        self.first = (self.first + 1) % Requester::COUNT;
    }

    fn serve(&mut self, requester: Requester, send: &mut impl FnMut(SafetyEvent) -> bool) {
        let mut taken = 0;
        if let Some((request, event)) = self.pending[requester.index()] {
            if !self.forward(requester, request, event, send) {
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
                Ok(event) => {
                    if !self.forward(requester, request, event, send) {
                        self.pending[requester.index()] = Some((request, event));
                        return;
                    }
                }
                Err(refusal) => self.publish(requester, request.seq, Outcome::Refused(refusal)),
            }
        }
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
            SafetyEvent::BenchMotor { request, .. } => {
                Some(SafetyEvent::BenchMotor { request, ticket })
            }
            SafetyEvent::Reboot { target, .. } => Some(SafetyEvent::Reboot { target, ticket }),
            _ => None,
        };
        let Some(with_ticket) = ticketed(self.next_ticket) else {
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
        });
        self.next_ticket = self.next_ticket.wrapping_add(1).max(1);
        true
    }

    fn settle(&mut self, reply: SafetyReply) {
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
        self.publish(ticket.requester, ticket.request_seq, reply.outcome);
    }

    fn publish(&mut self, requester: Requester, request_seq: u32, outcome: Outcome) {
        self.results.push(
            TimestampMicros(self.now_ms.saturating_mul(1_000)),
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

/// The content and permission checks: what the coordinator decides before
/// the owner sees a request.
fn check(requester: Requester, request: Request) -> Result<SafetyEvent, Refusal> {
    match request {
        Request::HostPresent => match requester {
            Requester::Usb => Ok(SafetyEvent::HostPresent(HostLink::Usb)),
            Requester::ConfiguratorLink => Ok(SafetyEvent::HostPresent(HostLink::ConfiguratorLink)),
            Requester::Osd | Requester::Storage => Err(Refusal::NotPermitted),
        },
        // Only the storage owner asks for a reboot, once the store is
        // settled; the command lines ask it, not the coordinator.
        Request::Reboot(target) => match requester {
            Requester::Storage => Ok(SafetyEvent::Reboot { target, ticket: 0 }),
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
            Ok(SafetyEvent::BenchMotor { request, ticket: 0 })
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
    }

    fn wire(queues: &mut Queues) -> (Coordinator<'_>, Ends<'_>) {
        let (usb, usb_rx) = link(&mut queues.usb);
        let (configurator_link, configurator_rx) = link(&mut queues.configurator_link);
        let (osd, osd_rx) = link(&mut queues.osd);
        let (storage, storage_rx) = link(&mut queues.storage);
        let (replies, replies_rx) = link(&mut queues.replies);
        let (results_tx, results) = link(&mut queues.results);
        (
            Coordinator::new(
                CoordinatorLinks {
                    usb: usb_rx,
                    configurator_link: configurator_rx,
                    osd: osd_rx,
                    storage: storage_rx,
                    safety_replies: replies_rx,
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
}
