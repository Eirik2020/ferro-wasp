#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthState {
    Initializing,
    Healthy,
    Degraded,
    Faulted,
}

impl HealthState {
    pub const fn worst(self, other: Self) -> Self {
        use HealthState::*;

        match (self, other) {
            (Faulted, _) | (_, Faulted) => Faulted,
            (Degraded, _) | (_, Degraded) => Degraded,
            (Initializing, _) | (_, Initializing) => Initializing,
            (Healthy, Healthy) => Healthy,
        }
    }
}
