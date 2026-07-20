use super::aliases::{Motor1Pin, Motor2Pin, Motor3Pin, Motor4Pin};
use super::profiles::{ESC_PWM_FREQUENCY_HZ, pwm_command_to_pulse_width_us};
use ferrowasp_stm32f4::hal_prelude::hal::{
    gpio::{Alternate, PA8, PB15, PC8, PC9},
    pac::{TIM1, TIM8, tim1::RegisterBlock},
    rcc::Rcc,
    timer::Timer,
};

const PWM_COUNTER_HZ: u32 = 1_000_000;
const PWM_PERIOD_TICKS: u32 = PWM_COUNTER_HZ / ESC_PWM_FREQUENCY_HZ;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscPwmError {
    InvalidTimerClock,
    InvalidMotor,
    InvalidCommand,
}

pub struct EscPwmResources {
    pub tim1: TIM1,
    pub tim8: TIM8,
    pub motor1_pin: Motor1Pin,
    pub motor2_pin: Motor2Pin,
    pub motor3_pin: Motor3Pin,
    pub motor4_pin: Motor4Pin,
}

pub struct EscPwmBank {
    tim1: TIM1,
    tim8: TIM8,
    _motor1_pin: PA8<Alternate<1>>,
    _motor2_pin: PC9<Alternate<3>>,
    _motor3_pin: PC8<Alternate<3>>,
    _motor4_pin: PB15<Alternate<1>>,
    last_pulse_width_us: [Option<u16>; 4],
    outputs_enabled: bool,
}

impl EscPwmBank {
    pub fn set_throttle(&mut self, motor_index: usize, command: u16) -> Result<(), EscPwmError> {
        let Some(pulse_width) = pwm_command_to_pulse_width_us(command) else {
            self.force_fully_off();
            return Err(EscPwmError::InvalidCommand);
        };

        match motor_index {
            0 => set_compare(self.tim1.ccr1(), pulse_width),
            1 => set_compare(self.tim8.ccr4(), pulse_width),
            2 => set_compare(self.tim8.ccr3(), pulse_width),
            3 => set_compare(self.tim1.ccr3(), pulse_width),
            _ => {
                self.force_fully_off();
                return Err(EscPwmError::InvalidMotor);
            }
        }

        self.last_pulse_width_us[motor_index] = Some(pulse_width);
        if !self.outputs_enabled && self.last_pulse_width_us.iter().all(Option::is_some) {
            self.enable_outputs();
        }
        Ok(())
    }

    pub fn set_throttles(&mut self, commands: [u16; 4]) -> Result<(), EscPwmError> {
        let mut pulse_widths = [0; 4];
        for (index, command) in commands.into_iter().enumerate() {
            let Some(pulse_width) = pwm_command_to_pulse_width_us(command) else {
                self.force_fully_off();
                return Err(EscPwmError::InvalidCommand);
            };
            pulse_widths[index] = pulse_width;
        }

        set_compare(self.tim1.ccr1(), pulse_widths[0]);
        set_compare(self.tim8.ccr4(), pulse_widths[1]);
        set_compare(self.tim8.ccr3(), pulse_widths[2]);
        set_compare(self.tim1.ccr3(), pulse_widths[3]);
        self.last_pulse_width_us = pulse_widths.map(Some);

        if !self.outputs_enabled {
            self.enable_outputs();
        }
        Ok(())
    }

    pub fn last_pulse_width_us(&self, motor_number: usize) -> Option<u32> {
        motor_number
            .checked_sub(1)
            .and_then(|index| self.last_pulse_width_us.get(index))
            .copied()
            .flatten()
            .map(u32::from)
    }

    pub fn force_fully_off(&mut self) {
        self.tim1.bdtr().modify(|_, w| w.moe().clear_bit());
        self.tim8.bdtr().modify(|_, w| w.moe().clear_bit());
        disable_channel_outputs(&self.tim1, &self.tim8);
        set_compare(self.tim1.ccr1(), 0);
        set_compare(self.tim8.ccr4(), 0);
        set_compare(self.tim8.ccr3(), 0);
        set_compare(self.tim1.ccr3(), 0);
        self.tim1.egr().write(|w| w.ug().update());
        self.tim8.egr().write(|w| w.ug().update());
        self.last_pulse_width_us = [None; 4];
        self.outputs_enabled = false;
    }

    fn enable_outputs(&mut self) {
        // Latch all prepared compare values before opening either advanced
        // timer's hardware output gate.
        self.tim1.egr().write(|w| w.ug().update());
        self.tim8.egr().write(|w| w.ug().update());
        enable_channel_outputs(&self.tim1, &self.tim8);
        self.tim1.bdtr().modify(|_, w| w.moe().set_bit());
        self.tim8.bdtr().modify(|_, w| w.moe().set_bit());
        self.outputs_enabled = true;
    }
}

pub fn init_esc_pwm(resources: EscPwmResources, rcc: &mut Rcc) -> Result<EscPwmBank, EscPwmError> {
    let timer_clock_hz = rcc.clocks.timclk2().raw();
    if timer_clock_hz % PWM_COUNTER_HZ != 0 {
        return Err(EscPwmError::InvalidTimerClock);
    }

    let divider = timer_clock_hz / PWM_COUNTER_HZ;
    if divider == 0 || divider > u16::MAX as u32 + 1 {
        return Err(EscPwmError::InvalidTimerClock);
    }
    let prescaler = (divider - 1) as u16;

    // Hold every external motor pad low before any alternate function is selected.
    let mut motor1_pin = resources.motor1_pin.into_push_pull_output();
    let mut motor2_pin = resources.motor2_pin.into_push_pull_output();
    let mut motor3_pin = resources.motor3_pin.into_push_pull_output();
    let mut motor4_pin = resources.motor4_pin.into_push_pull_output();
    motor1_pin.set_low();
    motor2_pin.set_low();
    motor3_pin.set_low();
    motor4_pin.set_low();

    let tim1 = Timer::new(resources.tim1, rcc).release();
    let tim8 = Timer::new(resources.tim8, rcc).release();
    configure_timer_base(&tim1, prescaler);
    configure_timer_base(&tim8, prescaler);
    configure_tim1_channels(&tim1);
    configure_tim8_channels(&tim8);

    // M4 uses TIM1_CH3N while TIM1_CH3 is disabled. RM0090 Table 96 defines
    // that case as OC3N = OC3REF xor CC3NP, so active-high polarity keeps
    // CC3NP clear for the same pulse shape as an ordinary ESC PWM output.
    tim1.ccer().modify(|_, w| {
        w.cc1p()
            .clear_bit()
            .cc1e()
            .clear_bit()
            .cc3e()
            .clear_bit()
            .cc3np()
            .active_high()
            .cc3ne()
            .clear_bit()
    });
    tim8.ccer().modify(|_, w| {
        w.cc3p()
            .clear_bit()
            .cc3e()
            .clear_bit()
            .cc4p()
            .clear_bit()
            .cc4e()
            .clear_bit()
    });

    // MOE=0 now drives each used output to a defined low idle state rather
    // than leaving the alternate-function pads high impedance.
    tim1.cr2()
        .modify(|_, w| w.ois1().clear_bit().ois3n().clear_bit());
    tim8.cr2()
        .modify(|_, w| w.ois3().clear_bit().ois4().clear_bit());
    configure_safe_output_gate(&tim1);
    configure_safe_output_gate(&tim8);

    // Keep each pad under the GPIO low latch until the complete advanced-timer
    // off-state and polarity configuration is in place.
    let motor1_pin = motor1_pin.into_alternate::<1>();
    let motor2_pin = motor2_pin.into_alternate::<3>();
    let motor3_pin = motor3_pin.into_alternate::<3>();
    let motor4_pin = motor4_pin.into_alternate::<1>();

    tim1.cr1().modify(|_, w| w.cen().set_bit());
    tim8.cr1().modify(|_, w| w.cen().set_bit());

    Ok(EscPwmBank {
        tim1,
        tim8,
        _motor1_pin: motor1_pin,
        _motor2_pin: motor2_pin,
        _motor3_pin: motor3_pin,
        _motor4_pin: motor4_pin,
        last_pulse_width_us: [None; 4],
        outputs_enabled: false,
    })
}

fn configure_timer_base(timer: &RegisterBlock, prescaler: u16) {
    timer.cr1().reset();
    timer.ccer().reset();
    timer.bdtr().reset();
    timer.psc().write(|w| w.psc().set(prescaler));
    timer
        .arr()
        .write(|w| w.arr().set((PWM_PERIOD_TICKS - 1) as u16));
    timer.ccr1().write(|w| w.ccr().set(0));
    timer.ccr2().write(|w| w.ccr().set(0));
    timer.ccr3().write(|w| w.ccr().set(0));
    timer.ccr4().write(|w| w.ccr().set(0));
    timer.cr1().modify(|_, w| w.arpe().set_bit());
}

fn configure_tim1_channels(timer: &RegisterBlock) {
    timer
        .ccmr1_output()
        .write(|w| w.cc1s().output().oc1pe().enabled().oc1m().pwm_mode1());
    timer
        .ccmr2_output()
        .write(|w| w.cc3s().output().oc3pe().enabled().oc3m().pwm_mode1());
    timer.egr().write(|w| w.ug().update());
}

fn configure_tim8_channels(timer: &RegisterBlock) {
    timer.ccmr2_output().write(|w| {
        w.cc3s()
            .output()
            .oc3pe()
            .enabled()
            .oc3m()
            .pwm_mode1()
            .cc4s()
            .output()
            .oc4pe()
            .enabled()
            .oc4m()
            .pwm_mode1()
    });
    timer.egr().write(|w| w.ug().update());
}

fn configure_safe_output_gate(timer: &RegisterBlock) {
    timer.bdtr().modify(|_, w| {
        w.ossi()
            .set_bit()
            .ossr()
            .set_bit()
            .bke()
            .clear_bit()
            .aoe()
            .clear_bit()
            .moe()
            .clear_bit()
    });
}

fn enable_channel_outputs(tim1: &RegisterBlock, tim8: &RegisterBlock) {
    tim1.ccer()
        .modify(|_, w| w.cc1e().set_bit().cc3e().clear_bit().cc3ne().set_bit());
    tim8.ccer()
        .modify(|_, w| w.cc3e().set_bit().cc4e().set_bit());
}

fn disable_channel_outputs(tim1: &RegisterBlock, tim8: &RegisterBlock) {
    tim1.ccer()
        .modify(|_, w| w.cc1e().clear_bit().cc3e().clear_bit().cc3ne().clear_bit());
    tim8.ccer()
        .modify(|_, w| w.cc3e().clear_bit().cc4e().clear_bit());
}

fn set_compare(register: &ferrowasp_stm32f4::hal_prelude::hal::pac::tim1::CCR, value: u16) {
    register.write(|w| w.ccr().set(value));
}
