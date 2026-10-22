//! The IMU kinds a board may carry on SPI1, as plain data: a board's data
//! crate names them, and the shared SPI1 tasks identify and read them.
//!
//! Requirements: HW-SPI-001
//! Safety-critical.

/// The IMU detected on SPI1, identified by its WHO_AM_I byte. Which of these
/// a board may carry is the board's; how each is identified and read is not.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Spi1ImuKind {
    Mpu6500 = 1,
    Icm42688P = 2,
    Mpu6000 = 3,
}

impl Spi1ImuKind {
    pub const MPU6500_WHO_AM_I: u8 = 0x70;
    pub const ICM42688P_WHO_AM_I: u8 = 0x47;
    pub const MPU6000_WHO_AM_I: u8 = 0x68;

    pub const fn from_who_am_i(who_am_i: u8) -> Option<Self> {
        match who_am_i {
            Self::MPU6500_WHO_AM_I => Some(Self::Mpu6500),
            Self::ICM42688P_WHO_AM_I => Some(Self::Icm42688P),
            Self::MPU6000_WHO_AM_I => Some(Self::Mpu6000),
            _ => None,
        }
    }

    pub const fn from_discriminant(value: u8) -> Option<Self> {
        match value {
            value if value == Self::Mpu6500 as u8 => Some(Self::Mpu6500),
            value if value == Self::Icm42688P as u8 => Some(Self::Icm42688P),
            value if value == Self::Mpu6000 as u8 => Some(Self::Mpu6000),
            _ => None,
        }
    }

    pub const fn who_am_i(self) -> u8 {
        match self {
            Self::Mpu6500 => Self::MPU6500_WHO_AM_I,
            Self::Icm42688P => Self::ICM42688P_WHO_AM_I,
            Self::Mpu6000 => Self::MPU6000_WHO_AM_I,
        }
    }

    pub const fn dma_burst_register(self) -> u8 {
        match self {
            Self::Mpu6500 | Self::Mpu6000 => 0x3b,
            Self::Icm42688P => 0x1d,
        }
    }
}
