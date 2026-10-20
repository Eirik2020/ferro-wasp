#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardIdentity {
    pub target_id: &'static str,
    pub name: &'static str,
    pub mcu: &'static str,
    pub package: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinAssignment {
    pub signal: &'static str,
    pub pin: &'static str,
    pub alternate: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerMode {
    ControlScheduler,
    Dshot,
    IoWatchdog,
    MicrosecondTimebase,
    StaticPwm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerGroupDescription {
    pub timer: &'static str,
    pub mode: TimerMode,
    pub channels: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceClaim {
    pub kind: ResourceKind,
    pub resource: &'static str,
    pub owner: &'static str,
}

impl ResourceClaim {
    pub const fn new(kind: ResourceKind, resource: &'static str, owner: &'static str) -> Self {
        Self {
            kind,
            resource,
            owner,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Peripheral,
    Pin,
    DmaStream,
    Irq,
    TimerChannel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestError {
    DuplicateClaim {
        kind: ResourceKind,
        resource: &'static str,
        first_owner: &'static str,
        second_owner: &'static str,
    },
}

pub const fn find_duplicate_claim(claims: &[ResourceClaim]) -> Option<ManifestError> {
    let mut outer = 0;

    while outer < claims.len() {
        let mut inner = outer + 1;

        while inner < claims.len() {
            let first = claims[outer];
            let second = claims[inner];

            if same_kind(first.kind, second.kind) && str_eq(first.resource, second.resource) {
                return Some(ManifestError::DuplicateClaim {
                    kind: first.kind,
                    resource: first.resource,
                    first_owner: first.owner,
                    second_owner: second.owner,
                });
            }

            inner += 1;
        }

        outer += 1;
    }

    None
}

pub const fn count_claims_by_kind(claims: &[ResourceClaim], kind: ResourceKind) -> usize {
    let mut index = 0;
    let mut count = 0;

    while index < claims.len() {
        if same_kind(claims[index].kind, kind) {
            count += 1;
        }

        index += 1;
    }

    count
}

const fn same_kind(left: ResourceKind, right: ResourceKind) -> bool {
    matches!(
        (left, right),
        (ResourceKind::Peripheral, ResourceKind::Peripheral)
            | (ResourceKind::Pin, ResourceKind::Pin)
            | (ResourceKind::DmaStream, ResourceKind::DmaStream)
            | (ResourceKind::Irq, ResourceKind::Irq)
            | (ResourceKind::TimerChannel, ResourceKind::TimerChannel)
    )
}

const fn str_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();

    if left.len() != right.len() {
        return false;
    }

    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }

        index += 1;
    }

    true
}
