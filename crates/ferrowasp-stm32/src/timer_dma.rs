pub const TIMER_DMA_STATUS_NOTE: &str = "FCU3 has an opt-in four-motor DShot600 bench backend; PWM timer-DMA and flight use remain deferred.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerDmaIntegrationState {
    StaticPwmOnly,
    FourMotorDshotBench,
}

#[cfg(not(feature = "dshot"))]
pub const TIMER_DMA_INTEGRATION_STATE: TimerDmaIntegrationState =
    TimerDmaIntegrationState::StaticPwmOnly;
#[cfg(feature = "dshot")]
pub const TIMER_DMA_INTEGRATION_STATE: TimerDmaIntegrationState =
    TimerDmaIntegrationState::FourMotorDshotBench;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "dshot"))]
    #[test]
    fn default_timer_dma_state_remains_static_pwm() {
        assert_eq!(
            TIMER_DMA_INTEGRATION_STATE,
            TimerDmaIntegrationState::StaticPwmOnly
        );
        assert!(TIMER_DMA_STATUS_NOTE.contains("four-motor DShot600"));
        assert!(TIMER_DMA_STATUS_NOTE.contains("PWM timer-DMA"));
    }

    #[cfg(feature = "dshot")]
    #[test]
    fn dshot_feature_reports_the_four_motor_bench_state() {
        assert_eq!(
            TIMER_DMA_INTEGRATION_STATE,
            TimerDmaIntegrationState::FourMotorDshotBench
        );
        assert!(TIMER_DMA_STATUS_NOTE.contains("four-motor DShot600"));
        assert!(TIMER_DMA_STATUS_NOTE.contains("flight use"));
    }
}
