//! What tasks outside the safety-critical core may ask of it (design §11):
//! one typed request per kind, sent over the requester's own link to the
//! coordinator, and the result the coordinator publishes back on the
//! ledger. A requester knows a request by the sequence its link stamped on
//! it.
//!
//! Requirements: BORDER-013, BORDER-014
//! Safety-critical.

/// Who asked: known from the link a request arrived on, never from its
/// content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Requester {
    Usb,
    ConfiguratorLink,
    Osd,
}

impl Requester {
    pub const COUNT: usize = 3;
    pub const ALL: [Self; Self::COUNT] = [Self::Usb, Self::ConfiguratorLink, Self::Osd];

    pub const fn index(self) -> usize {
        match self {
            Self::Usb => 0,
            Self::ConfiguratorLink => 1,
            Self::Osd => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request {
    /// A props-off motor test, decided by the safety master.
    MotorTest(MotorTest),
    /// A computer is attached on the requester's link; arming is refused
    /// while the lease this renews stands.
    HostPresent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotorTest {
    /// Idle one logical motor, 1-4, for one lease.
    Spin { logical_motor: u8 },
    /// Stop every motor now.
    Stop,
    /// Set and save one logical motor's spin direction.
    Direction { logical_motor: u8, reversed: bool },
}

/// The outcome of one request, for the requester that sent it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestResult {
    /// The sequence the requester's link stamped on the request; 0 before
    /// any result.
    pub request_seq: u32,
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Outcome {
    #[default]
    Accepted,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// This requester may not ask for this.
    NotPermitted,
    /// The request's content is out of range: a motor outside 1-4.
    Invalid,
    /// The safety master refused it in its present state.
    Armed,
    ArmingInProgress,
    ArmSwitchHigh,
    OutputDisabled,
    MotorMapUnavailable,
}

/// A result on its way to the observer, with the requester it is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequesterResult {
    pub requester: Requester,
    pub result: RequestResult,
}

impl Default for RequesterResult {
    fn default() -> Self {
        Self {
            requester: Requester::Usb,
            result: RequestResult::default(),
        }
    }
}
