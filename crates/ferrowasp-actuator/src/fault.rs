#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActuatorFault {
    UnauthorizedCommand,
    TimerDmaUnavailable,
    OutputFault,
    WatchdogInhibit,
}
