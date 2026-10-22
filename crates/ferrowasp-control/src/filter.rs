//! Requirements: CTRL-006
//! Safety-critical.

use core::f32::consts::PI;

/// The one-pole smoothing factor that places the corner at `corner_hz` when
/// sampled at `sample_rate_hz`.
///
/// Saturates at 1.0 - no filtering - for a corner at or above Nyquist, rather
/// than failing, because the rate loop must always come away with a usable
/// coefficient.
#[allow(
    clippy::neg_cmp_op_on_partial_ord,
    reason = "`!(x > 0.0)` is true for NaN, so a NaN corner or rate takes the \
              no-filtering branch; `x <= 0.0` would let it into the coefficient"
)]
pub fn gyro_lpf_alpha(corner_hz: f32, sample_rate_hz: f32) -> f32 {
    if !(corner_hz > 0.0) || !(sample_rate_hz > 0.0) {
        return 1.0;
    }
    clamp_unit_interval(1.0 - libm::expf(-2.0 * core::f32::consts::PI * corner_hz / sample_rate_hz))
}

/// The corner a stored smoothing factor produces at `sample_rate_hz`.
///
/// Used to migrate configurations written before the corner was stored
/// directly.
#[allow(
    clippy::neg_cmp_op_on_partial_ord,
    reason = "`!(x > 0.0)` is true for NaN, so a NaN rate reports no corner"
)]
pub fn gyro_lpf_corner_hz(alpha: f32, sample_rate_hz: f32) -> f32 {
    let alpha = clamp_unit_interval(alpha);
    if alpha <= 0.0 || alpha >= 1.0 || !(sample_rate_hz > 0.0) {
        return 0.0;
    }
    -libm::logf(1.0 - alpha) * sample_rate_hz / (2.0 * core::f32::consts::PI)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LowPassFilter {
    alpha: f32,
    value: f32,
    initialized: bool,
}

impl LowPassFilter {
    pub const fn new(alpha: f32) -> Self {
        Self {
            alpha,
            value: 0.0,
            initialized: false,
        }
    }

    pub fn update(&mut self, input: f32) -> f32 {
        if !self.initialized {
            self.value = input;
            self.initialized = true;
            return self.value;
        }

        self.value += self.alpha * (input - self.value);
        self.value
    }

    pub fn set_alpha(&mut self, alpha: f32) {
        self.alpha = clamp_unit_interval(alpha);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuRateLowPassFilter {
    roll: LowPassFilter,
    pitch: LowPassFilter,
    yaw: LowPassFilter,
}

impl ImuRateLowPassFilter {
    pub const fn new(alpha: f32) -> Self {
        Self {
            roll: LowPassFilter::new(alpha),
            pitch: LowPassFilter::new(alpha),
            yaw: LowPassFilter::new(alpha),
        }
    }

    pub fn update(&mut self, roll: f32, pitch: f32, yaw: f32) -> (f32, f32, f32) {
        (
            self.roll.update(roll),
            self.pitch.update(pitch),
            self.yaw.update(yaw),
        )
    }

    pub fn set_alpha(&mut self, alpha: f32) {
        self.roll.set_alpha(alpha);
        self.pitch.set_alpha(alpha);
        self.yaw.set_alpha(alpha);
    }
}

/// One state-variable notch on three axes, as Betaflight's `rpmNotch_t`:
/// the output is the input minus `weight / Q` times the band-pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Notch {
    f: f32,
    a1: f32,
    a2: f32,
    weighted_q_inv: f32,
    ic1: [f32; 3],
    ic2: [f32; 3],
}

impl Notch {
    pub fn new(hz: f32, dt_s: f32, q: f32) -> Self {
        let mut notch = Self {
            f: 0.0,
            a1: 0.0,
            a2: 0.0,
            weighted_q_inv: 0.0,
            ic1: [0.0; 3],
            ic2: [0.0; 3],
        };
        notch.set(hz, dt_s, q, 0.0);
        notch
    }

    pub fn set(&mut self, hz: f32, dt_s: f32, q: f32, weight: f32) {
        let f = libm::tanf(PI * hz * dt_s);
        let q_inv = 1.0 / q;
        let a1 = 1.0 / (1.0 + f * (f + q_inv));
        self.f = f;
        self.a1 = a1;
        self.a2 = f * a1;
        self.weighted_q_inv = q_inv * weight;
    }

    /// The prewarped frequency coefficient, `tan(pi * hz * dt)`, which moves
    /// with the notch's centre.
    pub const fn coefficient(&self) -> f32 {
        self.f
    }

    pub fn apply(&mut self, axes: &mut [f32; 3]) {
        for (axis, value) in axes.iter_mut().enumerate() {
            let v3 = *value - self.ic2[axis];
            let v1 = self.a1 * self.ic1[axis] + self.a2 * v3;
            let v2 = self.ic2[axis] + self.f * v1;
            self.ic1[axis] = 2.0 * v1 - self.ic1[axis];
            self.ic2[axis] = 2.0 * v2 - self.ic2[axis];
            *value -= self.weighted_q_inv * v1;
        }
    }
}

fn clamp_unit_interval(value: f32) -> f32 {
    if !value.is_finite() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "actual {actual} != expected {expected}"
        );
    }

    #[test]
    fn low_pass_filter_initializes_to_first_sample_then_smooths() {
        let mut filter = LowPassFilter::new(0.5);

        assert_close(filter.update(10.0), 10.0);
        assert_close(filter.update(20.0), 15.0);
        assert_close(filter.update(15.0), 15.0);
    }

    #[test]
    fn imu_rate_filter_tracks_axes_independently() {
        let mut filter = ImuRateLowPassFilter::new(0.25);

        assert_eq!(filter.update(4.0, 8.0, 12.0), (4.0, 8.0, 12.0));
        let (roll, pitch, yaw) = filter.update(8.0, 0.0, 20.0);

        assert_close(roll, 5.0);
        assert_close(pitch, 6.0);
        assert_close(yaw, 14.0);
    }

    #[test]
    fn a_non_finite_or_non_positive_corner_or_rate_gives_no_filtering() {
        for (corner, rate) in [
            (f32::NAN, 2_000.0),
            (50.0, f32::NAN),
            (0.0, 2_000.0),
            (-5.0, 2_000.0),
            (50.0, 0.0),
            (f32::INFINITY, 2_000.0),
        ] {
            assert_eq!(
                gyro_lpf_alpha(corner, rate),
                1.0,
                "corner {corner}, rate {rate}"
            );
        }
        assert_eq!(gyro_lpf_corner_hz(0.5, f32::NAN), 0.0);
    }
}
