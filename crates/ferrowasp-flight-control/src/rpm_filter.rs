//! Gyro notches that follow each motor's speed (design §21).
//!
//! Each motor's bidirectional-DShot eRPM becomes a smoothed mechanical
//! frequency ([`MotorFrequencies`]). The [`RpmFilter`] places one notch per
//! motor and harmonic at that frequency, on all three gyro axes, ahead of the
//! gyro low-pass. Behaviour follows Betaflight's `flight/rpm_filter.c`,
//! `drivers/dshot.c` and `common/filter.c` at commit `41a5b43`, except that
//! the loop rate is fixed, so there is no cycle-time compensation.
//!
//! The caller feeds each motor's last valid eRPM, held without an age limit
//! when replies stop (design §21): stale eRPM moves the notches, never motor
//! output.
//!
//! Requirements: CTRL-012, CTRL-031, CTRL-032, CTRL-033, CTRL-034
//! Safety-critical.

use core::f32::consts::PI;

use ferrowasp_control::filter::Notch;

/// Motors on a quad.
pub const MOTORS: usize = 4;

/// Harmonics a filter can track per motor.
pub const MAX_HARMONICS: usize = 3;

/// The RPM filter's settings, in Betaflight's units so a tune carries over.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RpmFilterConfig {
    /// Motor magnet poles; eRPM over half of them is mechanical RPM.
    pub motor_poles: u8,
    /// Harmonics notched per motor, 0 to [`MAX_HARMONICS`]; 0 turns the
    /// notches off.
    pub harmonics: u8,
    /// Each harmonic's notch depth, 0 to 100 percent.
    pub weights_percent: [u8; MAX_HARMONICS],
    /// Notches are off at and below this frequency.
    pub min_hz: u16,
    /// The range above `min_hz` over which a notch deepens to full weight.
    pub fade_range_hz: u16,
    /// Notch Q, in hundredths.
    pub q_hundredths: u16,
    /// Cutoff of the low-pass on each motor's frequency.
    pub frequency_lpf_hz: u16,
}

/// Betaflight's defaults (design §21). The pole count is the motors', set
/// before bench testing.
pub const DEFAULT_RPM_FILTER_CONFIG: RpmFilterConfig = RpmFilterConfig {
    motor_poles: 14,
    harmonics: 3,
    weights_percent: [100; MAX_HARMONICS],
    min_hz: 100,
    fade_range_hz: 50,
    q_hundredths: 500,
    frequency_lpf_hz: 150,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RpmFilterConfigError {
    /// Poles must be an even number, at least two.
    MotorPoles,
    /// More harmonics than [`MAX_HARMONICS`].
    Harmonics,
    /// A weight above 100 percent.
    Weight,
    /// A zero minimum frequency, Q or frequency low-pass.
    ZeroSetting,
    /// A loop rate of zero.
    LoopRate,
}

impl RpmFilterConfig {
    pub const fn validate(&self) -> Result<(), RpmFilterConfigError> {
        if self.motor_poles < 2 || !self.motor_poles.is_multiple_of(2) {
            return Err(RpmFilterConfigError::MotorPoles);
        }
        if self.harmonics as usize > MAX_HARMONICS {
            return Err(RpmFilterConfigError::Harmonics);
        }
        let mut index = 0;
        while index < MAX_HARMONICS {
            if self.weights_percent[index] > 100 {
                return Err(RpmFilterConfigError::Weight);
            }
            index += 1;
        }
        if self.min_hz == 0 || self.q_hundredths == 0 || self.frequency_lpf_hz == 0 {
            return Err(RpmFilterConfigError::ZeroSetting);
        }
        Ok(())
    }
}

/// Betaflight's PT1 gain: the fraction of the step taken each sample.
fn pt1_gain(cutoff_hz: f32, dt_s: f32) -> f32 {
    let omega = 2.0 * PI * cutoff_hz * dt_s;
    omega / (omega + 1.0)
}

/// Each motor's mechanical frequency from its eRPM, smoothed at the loop
/// rate. The RPM filter places its notches from it, and dynamic idle
/// (design §21) regulates its minimum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotorFrequencies {
    /// Mechanical hertz per unit of eRPM/100.
    hz_per_erpm_div100: f32,
    gain: f32,
    hz: [f32; MOTORS],
}

impl MotorFrequencies {
    pub fn new(config: &RpmFilterConfig, loop_rate_hz: u32) -> Result<Self, RpmFilterConfigError> {
        config.validate()?;
        if loop_rate_hz == 0 {
            return Err(RpmFilterConfigError::LoopRate);
        }
        let pole_pairs = f32::from(config.motor_poles / 2);
        Ok(Self {
            hz_per_erpm_div100: 100.0 / 60.0 / pole_pairs,
            gain: pt1_gain(
                f32::from(config.frequency_lpf_hz),
                1.0 / loop_rate_hz as f32,
            ),
            hz: [0.0; MOTORS],
        })
    }

    /// Takes each motor's last valid eRPM/100 and returns the smoothed
    /// frequencies. Called once per control loop.
    pub fn update(&mut self, erpm_div100: [u32; MOTORS]) -> [f32; MOTORS] {
        for (hz, erpm) in self.hz.iter_mut().zip(erpm_div100) {
            *hz += self.gain * (erpm as f32 * self.hz_per_erpm_div100 - *hz);
        }
        self.hz
    }

    pub const fn hz(&self) -> [f32; MOTORS] {
        self.hz
    }

    /// The slowest motor's frequency, which dynamic idle regulates.
    pub fn min_hz(&self) -> f32 {
        self.hz.iter().copied().fold(f32::INFINITY, f32::min)
    }
}

/// Every refreshed notch is updated within this long.
const NOTCH_REFRESH_S: f32 = 0.001;

/// The notches for every motor and harmonic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RpmFilter {
    harmonics: usize,
    weights: [f32; MAX_HARMONICS],
    min_hz: f32,
    max_hz: f32,
    fade_range_hz: f32,
    q: f32,
    dt_s: f32,
    notches: [[Notch; MAX_HARMONICS]; MOTORS],
    updates_per_loop: usize,
    next_motor: usize,
    next_harmonic: usize,
}

impl RpmFilter {
    pub fn new(config: &RpmFilterConfig, loop_rate_hz: u32) -> Result<Self, RpmFilterConfigError> {
        config.validate()?;
        if loop_rate_hz == 0 {
            return Err(RpmFilterConfigError::LoopRate);
        }
        let dt_s = 1.0 / loop_rate_hz as f32;
        let min_hz = f32::from(config.min_hz);
        let q = f32::from(config.q_hundredths) / 100.0;
        let harmonics = usize::from(config.harmonics);
        let notches = core::array::from_fn(|_| {
            core::array::from_fn(|harmonic| Notch::new(min_hz * (harmonic + 1) as f32, dt_s, q))
        });
        let loops_per_refresh = NOTCH_REFRESH_S / dt_s;
        let notches_per_axis = (MOTORS * harmonics) as f32;
        Ok(Self {
            harmonics,
            weights: config
                .weights_percent
                .map(|weight| f32::from(weight) / 100.0),
            min_hz,
            // Short of Nyquist, as in Betaflight, to keep notches stable.
            max_hz: 0.48 * loop_rate_hz as f32,
            fade_range_hz: f32::from(config.fade_range_hz),
            q,
            dt_s,
            notches,
            updates_per_loop: libm::ceilf(notches_per_axis / loops_per_refresh) as usize,
            next_motor: 0,
            next_harmonic: 0,
        })
    }

    /// Filters one gyro sample, roll, pitch and yaw, after moving the next
    /// share of the notches to `motor_hz`. Called once per control loop.
    pub fn apply(&mut self, motor_hz: &[f32; MOTORS], gyro: [f32; 3]) -> [f32; 3] {
        if self.harmonics == 0 {
            return gyro;
        }
        for _ in 0..self.updates_per_loop {
            self.refresh_next(motor_hz);
        }
        let mut axes = gyro;
        for harmonic in 0..self.harmonics {
            if self.weights[harmonic] <= 0.0 {
                continue;
            }
            for motor in &mut self.notches {
                motor[harmonic].apply(&mut axes);
            }
        }
        axes
    }

    /// Moves one notch to its motor's current frequency, round-robin.
    fn refresh_next(&mut self, motor_hz: &[f32; MOTORS]) {
        let (motor, harmonic) = (self.next_motor, self.next_harmonic);
        if self.weights[harmonic] > 0.0 {
            let hz = ((harmonic + 1) as f32 * motor_hz[motor]).clamp(self.min_hz, self.max_hz);
            let margin = hz - self.min_hz;
            let fade = if margin < self.fade_range_hz {
                margin / self.fade_range_hz
            } else {
                1.0
            };
            self.notches[motor][harmonic].set(hz, self.dt_s, self.q, fade * self.weights[harmonic]);
        }
        self.next_harmonic = (harmonic + 1) % self.harmonics;
        if self.next_harmonic == 0 {
            self.next_motor = (motor + 1) % MOTORS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOP_HZ: u32 = 2_000;

    fn config() -> RpmFilterConfig {
        DEFAULT_RPM_FILTER_CONFIG
    }

    /// The eRPM/100 that puts a 14-pole motor at `hz`.
    fn erpm_div100_for(hz: f32) -> u32 {
        libm::roundf(hz * 60.0 * 7.0 / 100.0) as u32
    }

    /// Peak output amplitude of a unit sine at `signal_hz` on roll, after
    /// `settle` samples, with every motor held at `motor_hz`.
    fn residual(filter: &mut RpmFilter, motor_hz: f32, signal_hz: f32) -> f32 {
        let motors = [motor_hz; MOTORS];
        let mut peak: f32 = 0.0;
        for sample in 0..4_000 {
            let t = sample as f32 / LOOP_HZ as f32;
            let input = libm::sinf(2.0 * PI * signal_hz * t);
            let output = filter.apply(&motors, [input, 0.0, 0.0])[0];
            if sample >= 2_000 {
                peak = peak.max(output.abs());
            }
        }
        peak
    }

    #[test]
    fn defaults_are_betaflights_and_valid() {
        assert_eq!(config().validate(), Ok(()));
        assert_eq!(config().harmonics, 3);
        assert_eq!(config().min_hz, 100);
        assert_eq!(config().fade_range_hz, 50);
        assert_eq!(config().q_hundredths, 500);
        assert_eq!(config().frequency_lpf_hz, 150);
    }

    #[test]
    fn invalid_settings_are_refused() {
        let with = |change: fn(&mut RpmFilterConfig)| {
            let mut config = config();
            change(&mut config);
            config.validate()
        };
        assert_eq!(
            with(|c| c.motor_poles = 13),
            Err(RpmFilterConfigError::MotorPoles)
        );
        assert_eq!(
            with(|c| c.motor_poles = 0),
            Err(RpmFilterConfigError::MotorPoles)
        );
        assert_eq!(
            with(|c| c.harmonics = 4),
            Err(RpmFilterConfigError::Harmonics)
        );
        assert_eq!(
            with(|c| c.weights_percent[2] = 101),
            Err(RpmFilterConfigError::Weight)
        );
        assert_eq!(
            with(|c| c.q_hundredths = 0),
            Err(RpmFilterConfigError::ZeroSetting)
        );
        assert_eq!(
            RpmFilter::new(&config(), 0),
            Err(RpmFilterConfigError::LoopRate)
        );
    }

    #[test]
    fn erpm_becomes_mechanical_hertz_through_the_pole_pairs() {
        let mut motors = MotorFrequencies::new(&config(), LOOP_HZ).unwrap();
        // 60,000 eRPM on 7 pole pairs is 8,571 RPM: 142.9 Hz.
        for _ in 0..1_000 {
            motors.update([600, 600, 600, 600]);
        }
        assert!((motors.hz()[0] - 142.857).abs() < 0.01);
    }

    #[test]
    fn motor_frequency_is_smoothed_by_a_150_hz_pt1() {
        let mut motors = MotorFrequencies::new(&config(), LOOP_HZ).unwrap();
        let step = motors.update([600, 0, 0, 0])[0];
        let omega = 2.0 * PI * 150.0 / LOOP_HZ as f32;
        assert!((step / 142.857 - omega / (omega + 1.0)).abs() < 1e-4);
        assert_eq!(motors.min_hz(), 0.0);
    }

    #[test]
    fn a_held_erpm_holds_the_frequency() {
        let mut motors = MotorFrequencies::new(&config(), LOOP_HZ).unwrap();
        for _ in 0..1_000 {
            motors.update([600, 300, 450, 900]);
        }
        let settled = motors.hz();
        for _ in 0..1_000 {
            motors.update([600, 300, 450, 900]);
        }
        assert_eq!(motors.hz(), settled);
        assert!((motors.min_hz() - 71.43).abs() < 0.01);
    }

    #[test]
    fn with_no_harmonics_the_gyro_passes_unchanged() {
        let mut filter = RpmFilter::new(
            &RpmFilterConfig {
                harmonics: 0,
                ..config()
            },
            LOOP_HZ,
        )
        .unwrap();
        assert_eq!(
            filter.apply(&[300.0; MOTORS], [1.0, -2.0, 3.0]),
            [1.0, -2.0, 3.0]
        );
    }

    #[test]
    fn each_harmonic_of_the_motor_is_notched() {
        for harmonic in 1..=3 {
            let mut filter = RpmFilter::new(&config(), LOOP_HZ).unwrap();
            let residual = residual(&mut filter, 250.0, 250.0 * harmonic as f32);
            assert!(residual < 0.05, "harmonic {harmonic}: {residual}");
        }
    }

    #[test]
    fn stick_frequencies_pass_through() {
        let mut filter = RpmFilter::new(&config(), LOOP_HZ).unwrap();
        let residual = residual(&mut filter, 250.0, 20.0);
        assert!((residual - 1.0).abs() < 0.02, "{residual}");
    }

    #[test]
    fn notches_below_the_minimum_fade_out() {
        // The fundamental alone: the 120 and 180 Hz harmonics' notches would
        // shave a few percent off 60 Hz themselves.
        let mut filter = RpmFilter::new(
            &RpmFilterConfig {
                harmonics: 1,
                ..config()
            },
            LOOP_HZ,
        )
        .unwrap();
        // 60 Hz motors: the fundamental sits under the 100 Hz minimum, so its
        // notch is at the minimum with no weight and passes 60 Hz untouched.
        let residual = residual(&mut filter, 60.0, 60.0);
        assert!((residual - 1.0).abs() < 0.02, "{residual}");
    }

    #[test]
    fn notches_are_half_deep_halfway_through_the_fade() {
        let mut filter = RpmFilter::new(
            &RpmFilterConfig {
                harmonics: 1,
                ..config()
            },
            LOOP_HZ,
        )
        .unwrap();
        // 125 Hz is halfway through the 100-150 Hz fade: a half-weight notch
        // over four motors at once still leaves a fraction of the input.
        let half = residual(&mut filter, 125.0, 125.0);
        let mut full = RpmFilter::new(
            &RpmFilterConfig {
                harmonics: 1,
                ..config()
            },
            LOOP_HZ,
        )
        .unwrap();
        let deep = residual(&mut full, 175.0, 175.0);
        assert!(half > deep && half < 0.5, "half {half}, full {deep}");
    }

    #[test]
    fn notches_stop_short_of_nyquist() {
        let mut filter = RpmFilter::new(&config(), LOOP_HZ).unwrap();
        let motors = [400.0; MOTORS];
        // The third harmonic, 1,200 Hz, is clamped to 960 Hz.
        for _ in 0..10 {
            filter.apply(&motors, [0.0; 3]);
        }
        let expected = libm::tanf(PI * 960.0 / LOOP_HZ as f32);
        assert!((filter.notches[0][2].coefficient() - expected).abs() < 1e-6);
    }

    #[test]
    fn every_notch_is_refreshed_within_a_millisecond() {
        let mut filter = RpmFilter::new(&config(), LOOP_HZ).unwrap();
        assert_eq!(filter.updates_per_loop, 6);
        let before = filter.notches;
        // Two loops at 2 kHz.
        filter.apply(&[300.0; MOTORS], [0.0; 3]);
        filter.apply(&[300.0; MOTORS], [0.0; 3]);
        for (motor, (after, before)) in filter.notches.iter().zip(&before).enumerate() {
            for (harmonic, (after, before)) in after.iter().zip(before).enumerate() {
                assert_ne!(
                    after.coefficient(),
                    before.coefficient(),
                    "motor {motor} harmonic {harmonic}"
                );
            }
        }
    }

    #[test]
    fn a_zero_weight_harmonic_is_neither_refreshed_nor_applied() {
        let mut config = config();
        config.weights_percent = [100, 0, 100];
        let mut filter = RpmFilter::new(&config, LOOP_HZ).unwrap();
        let before = filter.notches;
        for _ in 0..4 {
            filter.apply(&[300.0; MOTORS], [0.0; 3]);
        }
        for (after, before) in filter.notches.iter().zip(&before) {
            assert_eq!(after[1].coefficient(), before[1].coefficient());
        }
        // Its harmonic, 600 Hz, passes.
        let mut filter = RpmFilter::new(&config, LOOP_HZ).unwrap();
        let residual = residual(&mut filter, 300.0, 600.0);
        assert!(residual > 0.9, "{residual}");
    }

    #[test]
    fn the_settled_motor_frequency_notches_its_own_noise() {
        let mut motors = MotorFrequencies::new(&config(), LOOP_HZ).unwrap();
        let mut filter = RpmFilter::new(&config(), LOOP_HZ).unwrap();
        let erpm = erpm_div100_for(220.0);
        let mut peak: f32 = 0.0;
        for sample in 0..6_000 {
            let hz = motors.update([erpm; MOTORS]);
            let t = sample as f32 / LOOP_HZ as f32;
            let output = filter.apply(&hz, [libm::sinf(2.0 * PI * 220.0 * t), 0.0, 0.0])[0];
            if sample >= 4_000 {
                peak = peak.max(output.abs());
            }
        }
        assert!(peak < 0.05, "{peak}");
    }
}
