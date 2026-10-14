//! The STM32F4's factory-programmed unique device ID.

/// The 96-bit unique ID as 12 bytes: wafer X and Y, wafer number, and lot.
pub fn device_uid() -> [u8; 12] {
    let uid = stm32f4xx_hal::signature::Uid::get();
    let mut bytes = [0u8; 12];
    bytes[..2].copy_from_slice(&uid.x().to_le_bytes());
    bytes[2..4].copy_from_slice(&uid.y().to_le_bytes());
    bytes[4] = uid.waf_num();
    let lot = uid.lot_num().as_bytes();
    let len = lot.len().min(7);
    bytes[5..5 + len].copy_from_slice(&lot[..len]);
    bytes
}
