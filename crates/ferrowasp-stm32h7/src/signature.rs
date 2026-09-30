//! The device's 96-bit unique identifier.

/// The unique ID's twelve bytes, in memory order.
pub fn device_uid() -> [u8; 12] {
    *stm32h7xx_hal::signature::Uid::read()
}
