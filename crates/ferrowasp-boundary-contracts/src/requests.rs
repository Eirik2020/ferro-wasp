//! What tasks outside the safety-critical core may ask of it (design §11):
//! one typed request per kind, sent over the requester's own link to the
//! coordinator, and the result the coordinator publishes back on the
//! ledger. A requester knows a request by the sequence its link stamped on
//! it.
//!
//! Requirements: BORDER-013, BORDER-014
//! Safety-critical.

use ferrowasp_serial::{LogicalSerialPort, SerialFunction};

use crate::config_keys::ConfigKey;

/// Who asked: known from the link a request arrived on, never from its
/// content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Requester {
    Usb,
    ConfiguratorLink,
    Osd,
    /// The storage owner, which asks for a reboot once it has settled the
    /// store.
    Storage,
}

impl Requester {
    pub const COUNT: usize = 4;
    pub const ALL: [Self; Self::COUNT] =
        [Self::Usb, Self::ConfiguratorLink, Self::Osd, Self::Storage];

    pub const fn index(self) -> usize {
        match self {
            Self::Usb => 0,
            Self::ConfiguratorLink => 1,
            Self::Osd => 2,
            Self::Storage => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    /// A props-off motor test, decided by the safety master.
    MotorTest(MotorTest),
    /// A computer is attached on the requester's link; arming is refused
    /// while the lease this renews stands.
    HostPresent,
    /// Restart, into the firmware or the chip's ROM bootloader.
    Reboot(RebootTarget),
    /// Set one configuration value: at once for a live key, after a reboot
    /// for a boot-only one.
    ConfigSet { key: ConfigKey, value: f32 },
    /// Bind a serial port to a function, after a reboot.
    ConfigBind {
        port: LogicalSerialPort,
        function: SerialFunction,
    },
    /// Restore the board's default configuration, keeping the craft's
    /// accelerometer level.
    ConfigDefaults,
    /// Calibrate the accelerometer's level on a still, level craft, and
    /// adopt it.
    CalibrateAccel,
    /// The pilot saw that the boot fell back from the last saved
    /// configuration; arming may follow once the craft is otherwise ready.
    AcknowledgeConfigFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebootTarget {
    Firmware,
    RomBootloader,
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
    /// Accepted into the next boot's configuration; it applies after a
    /// reboot.
    Pending,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// This requester may not ask for this.
    NotPermitted,
    /// The request's content is out of range: a motor outside 1-4.
    Invalid,
    /// Another change is under way; ask again.
    Busy,
    /// The change was applied, but its participants did not confirm it in
    /// time; arming stays refused until reboot.
    NotConfirmed,
    /// The safety master refused it in its present state.
    Armed,
    ArmingInProgress,
    ArmSwitchHigh,
    OutputDisabled,
    MotorMapUnavailable,
    /// A calibration found the craft moving.
    CalibrationMoving,
    /// A calibration found the craft leaning more than 10 degrees.
    CalibrationNotLevel,
    /// A calibration found gravity too far from 1 g to trust.
    CalibrationBadGravity,
    /// A calibration got too few IMU samples in time.
    CalibrationNoSamples,
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
