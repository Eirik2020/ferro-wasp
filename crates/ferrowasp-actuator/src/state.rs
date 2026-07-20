#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActuatorState {
    Inhibited,
    Idle,
    Active,
    FaultLatched,
}
