//! Dynamic idle (design §21): a PID loop on the slowest motor's speed that
//! raises the motor output floor for all four motors together.
//!
//! It follows Betaflight's `flight/mixer.c` and `mixer_init.c` at commit
//! `41a5b43`, with one departure: when any motor's eRPM is stale, the increase
//! holds and the I-term stops integrating, and if the data stays stale the
//! increase decays to the static idle. Held eRPM therefore never winds the
//! floor up.
//!
//! The output is the floor increase as a fraction of the motor output range.
//! The mixer adds it to its floor; `actuator_output` still validates every
//! command (W1).
//!
//! Requirements: CTRL-011
//! Safety-critical.

/// Dynamic idle's settings, in Betaflight's units so a tune carries over.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicIdleConfig {
    /// Target speed of the slowest motor, in hundreds of RPM; 0 turns
    /// dynamic idle off.
    pub min_rpm_hundreds: u8,
    pub p_gain: u8,
    pub i_gain: u8,
    pub d_gain: u8,
    /// The most the floor rises once throttle has been raised, in tenths of
    /// a percent of the output range.
    pub max_increase_permille: u8,
    /// How long stale eRPM holds the increase before it decays.
    pub stale_hold_ms: u16,
    /// How long the decay takes across the whole `max_increase` range.
    pub stale_decay_ms: u16,
}

/// Betaflight's defaults: off, P/I/D 50, 15%. The stale hold and decay are
/// provisional, until logged reply-loss data sets them (design §21).
pub const DEFAULT_DYNAMIC_IDLE_CONFIG: DynamicIdleConfig = DynamicIdleConfig {
    min_rpm_hundreds: 0,
    p_gain: 50,
    i_gain: 50,
    d_gain: 50,
    max_increase_permille: 150,
    stale_hold_ms: 100,
    stale_decay_ms: 500,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DynamicIdleConfigError {
    /// A loop rate of zero.
    LoopRate,
    /// A static idle outside 0 to 1 of the output range.
    StaticIdle,
}

/// The controller's last update, for the blackbox (design §21).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DynamicIdleTerms {
    pub p: f32,
    pub i: f32,
    pub d: f32,
    /// The floor increase, a fraction of the output range.
    pub increase: f32,
    /// True while stale eRPM holds or decays the increase.
    pub stale_hold: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicIdle {
    target_hz: f32,
    p_gain: f32,
    i_gain: f32,
    d_gain: f32,
    max_increase: f32,
    static_idle: f32,
    /// Smoothing of the speed the D-term differentiates: about 20 ms.
    d_smoothing: f32,
    hold_loops: u32,
    decay_per_loop: f32,
    i: f32,
    previous_hz: Option<f32>,
    stale_loops: u32,
    terms: DynamicIdleTerms,
}

impl DynamicIdle {
    /// `static_idle` is the fixed idle as a fraction of the output range:
    /// the cap until throttle is first raised, and where stale eRPM decays to.
    pub fn new(
        config: &DynamicIdleConfig,
        loop_rate_hz: u32,
        static_idle: f32,
    ) -> Result<Self, DynamicIdleConfigError> {
        if loop_rate_hz == 0 {
            return Err(DynamicIdleConfigError::LoopRate);
        }
        if !(0.0..=1.0).contains(&static_idle) {
            return Err(DynamicIdleConfigError::StaticIdle);
        }
        let rate = loop_rate_hz as f32;
        let dt_s = 1.0 / rate;
        let max_increase = f32::from(config.max_increase_permille) * 0.001;
        let decay_loops = f32::from(config.stale_decay_ms) * 0.001 * rate;
        Ok(Self {
            target_hz: f32::from(config.min_rpm_hundreds) * 100.0 / 60.0,
            p_gain: f32::from(config.p_gain) * 0.000_15,
            i_gain: f32::from(config.i_gain) * 0.01 * dt_s,
            d_gain: f32::from(config.d_gain) * 0.000_000_3 * rate,
            max_increase,
            static_idle,
            d_smoothing: 800.0 * dt_s / 20.0,
            hold_loops: (u32::from(config.stale_hold_ms) * loop_rate_hz) / 1_000,
            decay_per_loop: if decay_loops > 0.0 {
                max_increase / decay_loops
            } else {
                f32::INFINITY
            },
            i: 0.0,
            previous_hz: None,
            stale_loops: 0,
            terms: DynamicIdleTerms::default(),
        })
    }

    pub fn enabled(&self) -> bool {
        self.target_hz > 0.0
    }

    /// One control-loop update. `min_motor_hz` is the slowest motor's
    /// smoothed speed (`MotorFrequencies::min_hz`); `throttle_raised` is
    /// whether throttle has left low since arming; `erpm_stale` is whether
    /// any motor has gone without a valid reply for longer than the command
    /// lease. Returns the floor increase.
    pub fn update(&mut self, min_motor_hz: f32, throttle_raised: bool, erpm_stale: bool) -> f32 {
        if !self.enabled() {
            return 0.0;
        }
        let cap = if throttle_raised {
            self.max_increase
        } else {
            self.static_idle
        };

        if erpm_stale {
            self.stale_loops = self.stale_loops.saturating_add(1);
            if self.stale_loops > self.hold_loops {
                let increase = self.terms.increase;
                let toward = self.static_idle - increase;
                let step = toward.clamp(-self.decay_per_loop, self.decay_per_loop);
                self.terms.increase = increase + step;
                self.i = self.terms.increase;
            }
            self.terms.stale_hold = true;
            // The smoothed speed restarts from fresh data.
            self.previous_hz = None;
            return self.terms.increase;
        }
        self.stale_loops = 0;

        let error = self.target_hz - min_motor_hz;
        let previous = self.previous_hz.unwrap_or(min_motor_hz);
        let smoothed = previous + self.d_smoothing * (min_motor_hz - previous);
        let d = (previous - smoothed) * self.d_gain;
        self.previous_hz = Some(smoothed);
        let p = error * self.p_gain;
        // The I-term rises fast and falls slowly.
        self.i = (self.i + error.max(-0.1) * self.i_gain).clamp(0.0, cap);
        let increase = (p + self.i + d).clamp(0.0, cap);
        self.terms = DynamicIdleTerms {
            p,
            i: self.i,
            d,
            increase,
            stale_hold: false,
        };
        increase
    }

    /// Starts over, as on disarm.
    pub fn reset(&mut self) {
        self.i = 0.0;
        self.previous_hz = None;
        self.stale_loops = 0;
        self.terms = DynamicIdleTerms::default();
    }

    pub const fn terms(&self) -> DynamicIdleTerms {
        self.terms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOP_HZ: u32 = 2_000;
    /// Betaflight's default static idle, 5.5%.
    const STATIC_IDLE: f32 = 0.055;

    fn on(min_rpm_hundreds: u8) -> DynamicIdle {
        DynamicIdle::new(
            &DynamicIdleConfig {
                min_rpm_hundreds,
                ..DEFAULT_DYNAMIC_IDLE_CONFIG
            },
            LOOP_HZ,
            STATIC_IDLE,
        )
        .unwrap()
    }

    /// A motor that settles at 50 Hz plus 2,000 Hz per unit of floor, with
    /// a lag: the loop has to raise the floor to reach its target.
    fn run(idle: &mut DynamicIdle, loops: usize, throttle_raised: bool, hz: &mut f32) -> f32 {
        let mut increase = 0.0;
        for _ in 0..loops {
            increase = idle.update(*hz, throttle_raised, false);
            *hz += 0.02 * (50.0 + 2_000.0 * increase - *hz);
        }
        increase
    }

    #[test]
    fn off_by_default() {
        let mut idle =
            DynamicIdle::new(&DEFAULT_DYNAMIC_IDLE_CONFIG, LOOP_HZ, STATIC_IDLE).unwrap();
        assert!(!idle.enabled());
        assert_eq!(idle.update(0.0, true, false), 0.0);
    }

    #[test]
    fn gains_scale_as_betaflights() {
        let idle = on(100);
        assert!((idle.target_hz - 166.667).abs() < 0.001);
        assert!((idle.p_gain - 0.0075).abs() < 1e-7);
        assert!((idle.i_gain - 0.5 / LOOP_HZ as f32).abs() < 1e-9);
        assert!((idle.d_gain - 0.03).abs() < 1e-7);
        assert!((idle.max_increase - 0.15).abs() < 1e-7);
        assert!((idle.d_smoothing - 0.02).abs() < 1e-7);
    }

    #[test]
    fn invalid_settings_are_refused() {
        assert_eq!(
            DynamicIdle::new(&DEFAULT_DYNAMIC_IDLE_CONFIG, 0, STATIC_IDLE),
            Err(DynamicIdleConfigError::LoopRate)
        );
        assert_eq!(
            DynamicIdle::new(&DEFAULT_DYNAMIC_IDLE_CONFIG, LOOP_HZ, 1.5),
            Err(DynamicIdleConfigError::StaticIdle)
        );
    }

    #[test]
    fn the_loop_brings_the_slowest_motor_to_its_target() {
        let mut idle = on(100);
        let mut hz = 50.0;
        let increase = run(&mut idle, 10_000, true, &mut hz);
        assert!((hz - 166.667).abs() < 2.0, "{hz} Hz");
        assert!(increase > STATIC_IDLE && increase < 0.15, "{increase}");
    }

    #[test]
    fn before_throttle_is_raised_the_floor_stops_at_static_idle() {
        let mut idle = on(200);
        let mut hz = 50.0;
        let increase = run(&mut idle, 10_000, false, &mut hz);
        assert_eq!(increase, STATIC_IDLE);
    }

    #[test]
    fn after_throttle_is_raised_the_floor_stops_at_the_maximum() {
        // 425 Hz: more than the test motor's 350 Hz at the full increase.
        let mut idle = on(255);
        let mut hz = 50.0;
        let increase = run(&mut idle, 10_000, true, &mut hz);
        assert_eq!(increase, 0.15);
    }

    #[test]
    fn motors_above_target_need_no_increase() {
        let mut idle = on(100);
        for _ in 0..1_000 {
            assert_eq!(idle.update(400.0, true, false), 0.0);
        }
    }

    #[test]
    fn the_i_term_rises_fast_and_falls_slowly() {
        let mut idle = on(100);
        // Few enough loops that neither direction reaches a bound.
        for _ in 0..5 {
            idle.update(100.0, true, false);
        }
        let risen = idle.terms().i;
        for _ in 0..5 {
            idle.update(400.0, true, false);
        }
        let fallen = risen - idle.terms().i;
        assert!(risen > 0.0 && risen < 0.15);
        // 66.7 Hz under target versus a fall capped at 0.1 Hz of error.
        assert!(fallen > 0.0 && fallen < risen / 100.0, "{risen} {fallen}");
    }

    #[test]
    fn a_falling_motor_speed_pushes_through_the_d_term() {
        let mut idle = on(100);
        for _ in 0..100 {
            idle.update(300.0, true, false);
        }
        idle.update(250.0, true, false);
        assert!(idle.terms().d > 0.0);
    }

    #[test]
    fn stale_erpm_holds_the_increase_without_integrating() {
        let mut idle = on(100);
        for _ in 0..500 {
            idle.update(100.0, true, false);
        }
        let held = idle.terms();
        // 100 ms of stale data at 2 kHz.
        for _ in 0..200 {
            assert_eq!(idle.update(0.0, true, true), held.increase);
        }
        assert!(idle.terms().stale_hold);
        assert_eq!(idle.i, held.i);
    }

    #[test]
    fn stale_erpm_then_decays_the_increase_to_static_idle() {
        let mut idle = on(255);
        let mut hz = 50.0;
        run(&mut idle, 10_000, true, &mut hz);
        assert_eq!(idle.terms().increase, 0.15);
        for _ in 0..200 {
            idle.update(hz, true, true);
        }
        idle.update(hz, true, true);
        assert!(idle.terms().increase < 0.15);
        // The whole range in 500 ms, so static idle well within that.
        for _ in 0..1_000 {
            idle.update(hz, true, true);
        }
        assert_eq!(idle.terms().increase, STATIC_IDLE);
    }

    #[test]
    fn stale_erpm_never_winds_the_floor_up_from_below_static_idle_past_it() {
        let mut idle = on(100);
        for _ in 0..1_000 {
            idle.update(400.0, true, false);
        }
        assert_eq!(idle.terms().increase, 0.0);
        for _ in 0..5_000 {
            assert!(idle.update(0.0, true, true) <= STATIC_IDLE);
        }
        assert_eq!(idle.terms().increase, STATIC_IDLE);
    }

    #[test]
    fn fresh_erpm_resumes_from_where_the_decay_left_off() {
        let mut idle = on(200);
        let mut hz = 50.0;
        run(&mut idle, 10_000, true, &mut hz);
        for _ in 0..2_000 {
            idle.update(hz, true, true);
        }
        let decayed = idle.terms().increase;
        let resumed = idle.update(hz, true, false);
        assert!(!idle.terms().stale_hold);
        assert!(resumed >= decayed, "{resumed} after {decayed}");
    }

    #[test]
    fn reset_starts_over() {
        let mut idle = on(100);
        for _ in 0..500 {
            idle.update(100.0, true, false);
        }
        idle.reset();
        assert_eq!(idle.terms(), DynamicIdleTerms::default());
        assert_eq!(idle.i, 0.0);
    }
}
