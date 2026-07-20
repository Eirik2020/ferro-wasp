use stm32f4xx_hal::{
    gpio::{Input, PA8, PB15, PC8, PC9},
    pac::{TIM1, TIM3, TIM12},
    prelude::*,
    rcc::Clocks,
    timer::{PwmChannel, Timer},
};

pub const ESC_PWM_FREQ_HZ: u32 = 400;
pub const ESC_PWM_CONFIG: rc_pwm::PwmConfig = rc_pwm::PwmConfig::default_const()
    .with_frequency_const(ESC_PWM_FREQ_HZ as u16)
    .with_pulse_range_const(1000, 2000)
    .with_command_range_const(0, 2000);

pub type Motor1Pwm = rc_pwm::PwmController<PwmChannel<TIM1, 0>>;
pub type Motor2Pwm = rc_pwm::PwmController<PwmChannel<TIM3, 3>>;
pub type Motor3Pwm = rc_pwm::PwmController<PwmChannel<TIM3, 2>>;
pub type Motor4Pwm = rc_pwm::PwmController<PwmChannel<TIM12, 1>>;

pub struct EscPwmResources {
    pub tim1: Timer<TIM1>,
    pub tim3: Timer<TIM3>,
    pub tim12: Timer<TIM12>,
    pub motor1_pin: PA8<Input>,
    pub motor2_pin: PC9<Input>,
    pub motor3_pin: PC8<Input>,
    pub motor4_pin: PB15<Input>,
}

pub struct EscPwmParts {
    pub m1: Motor1Pwm,
    pub m2: Motor2Pwm,
    pub m3: Motor3Pwm,
    pub m4: Motor4Pwm,
}

pub fn init_esc_pwm(resources: EscPwmResources, clocks: &Clocks) -> EscPwmParts {
    let EscPwmResources {
        mut tim1,
        mut tim3,
        mut tim12,
        motor1_pin,
        motor2_pin,
        motor3_pin,
        motor4_pin,
    } = resources;

    tim1.configure(clocks);
    tim3.configure(clocks);
    tim12.configure(clocks);

    let (_, (tim1_ch1, ..)) = tim1.pwm_hz(ESC_PWM_FREQ_HZ.Hz());
    let (_, (_, _, tim3_ch3, tim3_ch4)) = tim3.pwm_hz(ESC_PWM_FREQ_HZ.Hz());
    let (_, (_, tim12_ch2)) = tim12.pwm_hz(ESC_PWM_FREQ_HZ.Hz());

    let mut tim1_ch1 = tim1_ch1.with(motor1_pin);
    let mut tim3_ch4 = tim3_ch4.with(motor2_pin);
    let mut tim3_ch3 = tim3_ch3.with(motor3_pin);
    let mut tim12_ch2 = tim12_ch2.with(motor4_pin);

    tim1_ch1.enable();
    tim3_ch4.enable();
    tim3_ch3.enable();
    tim12_ch2.enable();

    EscPwmParts {
        m1: rc_pwm::PwmController::new(tim1_ch1, ESC_PWM_CONFIG).unwrap(),
        m2: rc_pwm::PwmController::new(tim3_ch4, ESC_PWM_CONFIG).unwrap(),
        m3: rc_pwm::PwmController::new(tim3_ch3, ESC_PWM_CONFIG).unwrap(),
        m4: rc_pwm::PwmController::new(tim12_ch2, ESC_PWM_CONFIG).unwrap(),
    }
}
