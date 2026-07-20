pub const PPM_FREQ: u32 = 600;

pub fn throttle_to_u16(value: f32) -> u16 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= u16::MAX as f32 {
        u16::MAX
    } else {
        value as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_to_u16_saturates_and_rejects_non_finite_values() {
        assert_eq!(throttle_to_u16(f32::NAN), 0);
        assert_eq!(throttle_to_u16(-1.0), 0);
        assert_eq!(throttle_to_u16(0.0), 0);
        assert_eq!(throttle_to_u16(1234.9), 1234);
        assert_eq!(throttle_to_u16(u16::MAX as f32 + 1.0), u16::MAX);
    }
}
