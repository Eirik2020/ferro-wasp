#![no_std]
#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlOutput {
    pub output: f32,
    pub p: f32,
    pub i: f32,
    pub d: f32,
    pub ff: f32,
    pub error: f32,
    pub setpoint_rate: f32,
    pub measurement_rate: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pid {
    pub setpoint: f32,
    output_limit: f32,
    p_gain: f32,
    i_gain: f32,
    d_gain: f32,
    ff_gain: f32,
    p_limit: f32,
    i_limit: f32,
    d_limit: f32,
    ff_limit: f32,
    integral: f32,
    anti_windup: bool,
    i_relax_setpoint_rate: f32,
    d_filter_alpha: f32,
    filtered_d: f32,
    previous_setpoint: Option<f32>,
    previous_measurement: Option<f32>,
}

impl Pid {
    pub const fn new(setpoint: f32, output_limit: f32) -> Self {
        Self {
            setpoint,
            output_limit,
            p_gain: 0.0,
            i_gain: 0.0,
            d_gain: 0.0,
            ff_gain: 0.0,
            p_limit: output_limit,
            i_limit: output_limit,
            d_limit: output_limit,
            ff_limit: output_limit,
            integral: 0.0,
            anti_windup: true,
            i_relax_setpoint_rate: f32::INFINITY,
            d_filter_alpha: 1.0,
            filtered_d: 0.0,
            previous_setpoint: None,
            previous_measurement: None,
        }
    }

    pub fn p(&mut self, gain: f32, limit: f32) -> &mut Self {
        self.p_gain = finite_or_zero(gain);
        self.p_limit = positive_limit_or_zero(limit);
        self
    }

    pub fn i(&mut self, gain: f32, limit: f32) -> &mut Self {
        self.i_gain = finite_or_zero(gain);
        self.i_limit = positive_limit_or_zero(limit);
        self.integral = clamp_symmetric(self.integral, self.i_limit);
        self
    }

    pub fn d(&mut self, gain: f32, limit: f32) -> &mut Self {
        self.d_gain = finite_or_zero(gain);
        self.d_limit = positive_limit_or_zero(limit);
        self
    }

    pub fn ff(&mut self, gain: f32, limit: f32) -> &mut Self {
        self.ff_gain = finite_or_zero(gain);
        self.ff_limit = positive_limit_or_zero(limit);
        self
    }

    pub fn anti_windup(&mut self, enabled: bool) -> &mut Self {
        self.anti_windup = enabled;
        self
    }

    pub fn d_filter_alpha(&mut self, alpha: f32) -> &mut Self {
        self.d_filter_alpha = finite_or_zero(alpha).clamp(0.0, 1.0);
        self
    }

    pub fn i_relax_setpoint_rate(&mut self, threshold_per_second: f32) -> &mut Self {
        let threshold = finite_or_zero(threshold_per_second);
        self.i_relax_setpoint_rate = if threshold > 0.0 {
            threshold
        } else {
            f32::INFINITY
        };
        self
    }

    pub fn set_setpoint(&mut self, setpoint: f32) {
        self.setpoint = finite_or_zero(setpoint);
    }

    pub fn reset_integral(&mut self) {
        self.integral = 0.0;
    }

    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.filtered_d = 0.0;
        self.previous_setpoint = None;
        self.previous_measurement = None;
    }

    pub fn next_control_output(&mut self, measurement: f32) -> ControlOutput {
        self.next_control_output_dt(measurement, 1.0)
    }

    pub fn next_control_output_dt(&mut self, measurement: f32, dt_seconds: f32) -> ControlOutput {
        let dt_seconds = positive_dt_or_one(dt_seconds);
        let setpoint = finite_or_zero(self.setpoint);
        let measurement = finite_or_zero(measurement);
        let previous_setpoint = self.previous_setpoint.unwrap_or(setpoint);
        let previous_measurement = self.previous_measurement.unwrap_or(measurement);
        let setpoint_rate = (setpoint - previous_setpoint) / dt_seconds;
        let measurement_rate = (measurement - previous_measurement) / dt_seconds;
        let error = finite_or_zero(self.setpoint) - finite_or_zero(measurement);

        let p = clamp_symmetric(error * self.p_gain, self.p_limit);

        let d = match self.previous_measurement {
            Some(_) => {
                let raw_d = clamp_symmetric(-measurement_rate * self.d_gain, self.d_limit);
                self.filtered_d += self.d_filter_alpha * (raw_d - self.filtered_d);
                self.filtered_d
            }
            None => {
                self.filtered_d = 0.0;
                0.0
            }
        };
        self.previous_setpoint = Some(setpoint);
        self.previous_measurement = Some(measurement);

        let ff = match self.previous_setpoint {
            Some(_) => clamp_symmetric(setpoint_rate * self.ff_gain, self.ff_limit),
            None => 0.0,
        };

        let next_integral = clamp_symmetric(
            self.integral + error * self.i_gain * dt_seconds,
            self.i_limit,
        );
        if self.should_accept_integral(p, next_integral, d, ff, error, setpoint_rate) {
            self.integral = next_integral;
        }
        let i = self.integral;

        ControlOutput {
            output: clamp_symmetric(p + i + d + ff, positive_limit_or_zero(self.output_limit)),
            p,
            i,
            d,
            ff,
            error,
            setpoint_rate,
            measurement_rate,
        }
    }

    fn should_accept_integral(
        &self,
        p: f32,
        next_integral: f32,
        d: f32,
        ff: f32,
        error: f32,
        setpoint_rate: f32,
    ) -> bool {
        if !self.anti_windup {
            return true;
        }

        if setpoint_rate.abs() > self.i_relax_setpoint_rate
            && next_integral.abs() >= self.integral.abs()
        {
            return false;
        }

        let output_limit = positive_limit_or_zero(self.output_limit);
        if output_limit == 0.0 {
            return false;
        }

        let next_unclamped_output = p + next_integral + d + ff;
        if next_unclamped_output.abs() <= output_limit {
            return true;
        }

        let current_unclamped_output = p + self.integral + d + ff;
        if next_integral.abs() < self.integral.abs() {
            return true;
        }

        next_unclamped_output > output_limit && error < 0.0
            || next_unclamped_output < -output_limit && error > 0.0
            || current_unclamped_output.abs() > next_unclamped_output.abs()
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn positive_limit_or_zero(limit: f32) -> f32 {
    let limit = finite_or_zero(limit);
    if limit > 0.0 { limit } else { 0.0 }
}

fn positive_dt_or_one(dt_seconds: f32) -> f32 {
    let dt_seconds = finite_or_zero(dt_seconds);
    if dt_seconds > 0.0 { dt_seconds } else { 1.0 }
}

fn clamp_symmetric(value: f32, limit: f32) -> f32 {
    let value = finite_or_zero(value);
    let limit = positive_limit_or_zero(limit);

    if limit == 0.0 {
        0.0
    } else if value > limit {
        limit
    } else if value < -limit {
        -limit
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::Pid;

    #[test]
    fn proportional_output_tracks_error() {
        let mut pid = Pid::new(100.0, 500.0);
        pid.p(1.5, 500.0);

        let output = pid.next_control_output(60.0);

        assert_eq!(output.error, 40.0);
        assert_eq!(output.p, 60.0);
        assert_eq!(output.output, 60.0);
    }

    #[test]
    fn total_output_is_clamped_symmetrically() {
        let mut pid = Pid::new(100.0, 25.0);
        pid.p(2.0, 500.0);

        assert_eq!(pid.next_control_output(0.0).output, 25.0);
        assert_eq!(pid.next_control_output(200.0).output, -25.0);
    }

    #[test]
    fn integral_accumulates_and_respects_limit() {
        let mut pid = Pid::new(10.0, 100.0);
        pid.i(2.0, 15.0);

        assert_eq!(pid.next_control_output(0.0).i, 15.0);
        assert_eq!(pid.next_control_output(0.0).i, 15.0);

        pid.reset_integral();
        assert_eq!(pid.next_control_output(10.0).i, 0.0);
    }

    #[test]
    fn integral_scales_with_dt_seconds() {
        let mut pid = Pid::new(10.0, 100.0);
        pid.i(2.0, 100.0);

        assert_eq!(pid.next_control_output_dt(0.0, 0.25).i, 5.0);
        assert_eq!(pid.next_control_output_dt(0.0, 0.25).i, 10.0);
    }

    #[test]
    fn anti_windup_freezes_integral_when_output_is_saturated() {
        let mut pid = Pid::new(100.0, 50.0);
        pid.p(1.0, 500.0).i(1.0, 500.0);

        let first = pid.next_control_output(0.0);
        let second = pid.next_control_output(0.0);

        assert_eq!(first.output, 50.0);
        assert_eq!(second.output, 50.0);
        assert_eq!(first.i, 0.0);
        assert_eq!(second.i, 0.0);
    }

    #[test]
    fn anti_windup_allows_integral_to_unwind() {
        let mut pid = Pid::new(0.0, 50.0);
        pid.i(1.0, 500.0).anti_windup(false);
        pid.next_control_output(0.0);
        pid.setpoint = 100.0;
        pid.next_control_output(0.0);
        pid.next_control_output(0.0);
        pid.anti_windup(true);
        pid.setpoint = -100.0;

        let output = pid.next_control_output(0.0);

        assert!(output.i < 200.0);
    }

    #[test]
    fn derivative_is_zero_until_previous_error_exists() {
        let mut pid = Pid::new(10.0, 100.0);
        pid.d(3.0, 100.0);

        assert_eq!(pid.next_control_output(0.0).d, 0.0);
        assert_eq!(pid.next_control_output(5.0).d, -15.0);
    }

    #[test]
    fn derivative_uses_measurement_not_setpoint() {
        let mut pid = Pid::new(10.0, 100.0);
        pid.d(3.0, 100.0);

        pid.next_control_output_dt(0.0, 0.5);
        pid.set_setpoint(20.0);

        assert_eq!(pid.next_control_output_dt(0.0, 0.5).d, 0.0);
        assert_eq!(pid.next_control_output_dt(5.0, 0.5).d, -30.0);
    }

    #[test]
    fn feedforward_tracks_setpoint_rate() {
        let mut pid = Pid::new(0.0, 100.0);
        pid.ff(0.5, 100.0);

        pid.next_control_output_dt(0.0, 0.25);
        pid.set_setpoint(20.0);
        let output = pid.next_control_output_dt(0.0, 0.25);

        assert_eq!(output.setpoint_rate, 80.0);
        assert_eq!(output.ff, 40.0);
        assert_eq!(output.output, 40.0);
    }

    #[test]
    fn i_term_relax_freezes_growth_during_fast_setpoint_changes() {
        let mut pid = Pid::new(0.0, 500.0);
        pid.i(1.0, 500.0).i_relax_setpoint_rate(50.0);

        pid.next_control_output_dt(0.0, 0.1);
        pid.set_setpoint(100.0);
        let fast_step = pid.next_control_output_dt(0.0, 0.1);
        let fast_hold = pid.next_control_output_dt(0.0, 0.1);

        assert_eq!(fast_step.i, 0.0);
        assert_eq!(fast_hold.i, 10.0);
    }

    #[test]
    fn derivative_filter_smooths_d_term() {
        let mut pid = Pid::new(10.0, 100.0);
        pid.d(10.0, 100.0).d_filter_alpha(0.25);

        assert_eq!(pid.next_control_output(0.0).d, 0.0);
        assert_eq!(pid.next_control_output(5.0).d, -12.5);
        assert_eq!(pid.next_control_output(5.0).d, -9.375);
    }

    #[test]
    fn reset_clears_derivative_history() {
        let mut pid = Pid::new(10.0, 100.0);
        pid.d(1.0, 100.0);

        pid.next_control_output(0.0);
        pid.reset();

        assert_eq!(pid.next_control_output(5.0).d, 0.0);
    }

    #[test]
    fn non_finite_values_do_not_escape_to_output() {
        let mut pid = Pid::new(f32::NAN, f32::INFINITY);
        pid.p(f32::INFINITY, f32::INFINITY)
            .i(f32::NAN, f32::NAN)
            .d(f32::INFINITY, f32::INFINITY);

        let output = pid.next_control_output(f32::NAN);

        assert!(output.output.is_finite());
        assert_eq!(output.output, 0.0);
    }
}
