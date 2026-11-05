//! The IMU class: a part whose sample the bus layer reads as one burst,
//! decoded into the `ImuSample` the control loop takes.
//!
//! The class follows what the IMU drivers deliver: the SPI transport reads a
//! burst from a fixed register, and the parser decodes it, stamps it and
//! pushes it onto the IMU's `state` link. So the class is a decoder, not a
//! polled [`Sensor`](crate::class::Sensor); `ImuSample` is reused unchanged,
//! since it already is the class's sample.
//!
//! Requirements: SENSOR-005
//! Safety-critical.

pub use ferrowasp_io_contracts::imu::ImuSample;

use crate::class::SensorError;

/// The longest burst [`check_refuses_malformed_bursts`] can exercise.
pub const MAX_CHECKED_BURST_LEN: usize = 64;

/// An IMU part, as its burst decoder.
pub trait Imu {
    /// The identity register's expected value (WHO_AM_I).
    const IDENTITY: u8;
    /// The register a sample burst starts at.
    const BURST_REGISTER: u8;
    /// Bytes in one burst, the leading address byte included.
    const BURST_LEN: usize;

    /// Decode one burst into a sample in the sensor's axes, in g, degrees
    /// per second and degrees Celsius. A burst shorter than
    /// [`Imu::BURST_LEN`] is `SensorError::Bus`; one whose payload is all
    /// zero or all one bits, as a stuck or absent bus reads, is
    /// `SensorError::OutOfRange`.
    fn decode_burst(&self, frame: &[u8]) -> Result<ImuSample, SensorError>;
}

/// The first way an implementation broke the class's rules for malformed
/// bursts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MalformedBurstFailure {
    /// `BURST_LEN` is 0 or over [`MAX_CHECKED_BURST_LEN`], so it cannot be checked.
    UncheckableLength,
    /// A burst of this many bytes was not refused as `SensorError::Bus`.
    ShortBurstAccepted { len: usize },
    /// A whole burst whose payload is all this byte was not refused as
    /// `SensorError::OutOfRange`.
    StuckBusAccepted { byte: u8 },
}

/// The class's conformance check for malformed bursts: every length short
/// of a burst, and a whole burst of each stuck-bus pattern, must be refused
/// with its error kind. Each implementation's tests call it with itself.
pub fn check_refuses_malformed_bursts<I: Imu>(imu: &I) -> Result<(), MalformedBurstFailure> {
    if I::BURST_LEN == 0 || I::BURST_LEN > MAX_CHECKED_BURST_LEN {
        return Err(MalformedBurstFailure::UncheckableLength);
    }
    let mut frame = [0u8; MAX_CHECKED_BURST_LEN];
    if let Some(address) = frame.first_mut() {
        *address = I::BURST_REGISTER;
    }
    for len in 0..I::BURST_LEN {
        let short = frame.get(..len).unwrap_or(&[]);
        if imu.decode_burst(short) != Err(SensorError::Bus) {
            return Err(MalformedBurstFailure::ShortBurstAccepted { len });
        }
    }
    for byte in [0x00, 0xff] {
        for (index, slot) in frame.iter_mut().enumerate() {
            *slot = if index == 0 { I::BURST_REGISTER } else { byte };
        }
        let whole = frame.get(..I::BURST_LEN).unwrap_or(&[]);
        if imu.decode_burst(whole) != Err(SensorError::OutOfRange) {
            return Err(MalformedBurstFailure::StuckBusAccepted { byte });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seven-byte part: address, then three big-endian gyro axes.
    struct GyroOnly {
        refuse_short: bool,
        refuse_stuck: bool,
    }

    impl Imu for GyroOnly {
        const IDENTITY: u8 = 0x5a;
        const BURST_REGISTER: u8 = 0x43;
        const BURST_LEN: usize = 7;

        fn decode_burst(&self, frame: &[u8]) -> Result<ImuSample, SensorError> {
            let Some(payload) = frame.get(1..Self::BURST_LEN) else {
                return if self.refuse_short {
                    Err(SensorError::Bus)
                } else {
                    Ok(ImuSample::default())
                };
            };
            let stuck =
                payload.iter().all(|byte| *byte == 0) || payload.iter().all(|byte| *byte == 0xff);
            if stuck && self.refuse_stuck {
                return Err(SensorError::OutOfRange);
            }
            let mut sample = ImuSample::default();
            let (pairs, _) = payload.as_chunks::<2>();
            for (axis, pair) in sample.gyro_raw.iter_mut().zip(pairs) {
                *axis = i16::from_be_bytes(*pair);
            }
            Ok(sample)
        }
    }

    struct TooLong;

    impl Imu for TooLong {
        const IDENTITY: u8 = 0;
        const BURST_REGISTER: u8 = 0;
        const BURST_LEN: usize = MAX_CHECKED_BURST_LEN + 1;

        fn decode_burst(&self, _frame: &[u8]) -> Result<ImuSample, SensorError> {
            Err(SensorError::Bus)
        }
    }

    #[test]
    fn an_imu_refuses_short_and_stuck_bus_bursts() {
        let good = GyroOnly {
            refuse_short: true,
            refuse_stuck: true,
        };
        assert_eq!(check_refuses_malformed_bursts(&good), Ok(()));
        let decoded = good.decode_burst(&[0x43, 0x00, 0x01, 0xff, 0xfe, 0x7f, 0xff]);
        assert_eq!(decoded.map(|sample| sample.gyro_raw), Ok([1, -2, i16::MAX]));

        let accepts_short = GyroOnly {
            refuse_short: false,
            refuse_stuck: true,
        };
        assert_eq!(
            check_refuses_malformed_bursts(&accepts_short),
            Err(MalformedBurstFailure::ShortBurstAccepted { len: 0 })
        );

        let accepts_stuck = GyroOnly {
            refuse_short: true,
            refuse_stuck: false,
        };
        assert_eq!(
            check_refuses_malformed_bursts(&accepts_stuck),
            Err(MalformedBurstFailure::StuckBusAccepted { byte: 0x00 })
        );

        assert_eq!(
            check_refuses_malformed_bursts(&TooLong),
            Err(MalformedBurstFailure::UncheckableLength)
        );
    }
}
