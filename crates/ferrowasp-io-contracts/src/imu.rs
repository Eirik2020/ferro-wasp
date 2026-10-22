//! The IMU sample the parser hands the control loop (design §12), over a
//! `state` link: gyro and accelerometer from one burst in one message, so
//! the loop never mixes two samples.
//!
//! Requirements: IO-004
//! Safety-critical.

/// One decoded IMU burst, in the sensor's axes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImuSample {
    pub gyro_raw: [i16; 3],
    /// Acceleration in g, and rates in degrees per second.
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
    pub temperature_c: f32,
}
