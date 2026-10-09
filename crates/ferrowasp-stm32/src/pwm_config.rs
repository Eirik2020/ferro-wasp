pub const ESC_PWM_FREQUENCY_HZ: u32 = 400;
pub const ESC_PWM_MIN_US: u16 = 1_000;
pub const ESC_PWM_MAX_US: u16 = 2_000;
pub const ESC_COMMAND_MAX: u16 = 2_000;

pub const ESC_PWM_CONFIG: rc_pwm::PwmConfig = rc_pwm::PwmConfig::default_const()
    .with_frequency_const(ESC_PWM_FREQUENCY_HZ as u16)
    .with_pulse_range_const(ESC_PWM_MIN_US as u32, ESC_PWM_MAX_US as u32)
    .with_command_range_const(0, ESC_COMMAND_MAX);

pub const fn pwm_command_to_pulse_width_us(command: u16) -> Option<u16> {
    if command > ESC_COMMAND_MAX {
        return None;
    }

    let pulse_span = (ESC_PWM_MAX_US - ESC_PWM_MIN_US) as u32;
    let scaled = (command as u32 * pulse_span) / ESC_COMMAND_MAX as u32;
    Some(ESC_PWM_MIN_US + scaled as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mapping_matches_the_f405_pwm_contract() {
        assert_eq!(pwm_command_to_pulse_width_us(0), Some(1_000));
        assert_eq!(pwm_command_to_pulse_width_us(1_000), Some(1_500));
        assert_eq!(pwm_command_to_pulse_width_us(2_000), Some(2_000));
        assert_eq!(pwm_command_to_pulse_width_us(2_001), None);
    }

    #[test]
    fn rc_pwm_config_matches_the_mapping() {
        assert_eq!(
            ESC_PWM_CONFIG.pwm_frequency().hz(),
            ESC_PWM_FREQUENCY_HZ as u16
        );
    }
}
