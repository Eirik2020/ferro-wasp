mod fault;

pub use fault::WaveformFault;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveformMode {
    StaticPwm,
    FiniteDma,
    ContinuousStreaming,
}
