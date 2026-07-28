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

pub fn remap_motor_outputs(logical: [f32; 4], logical_to_physical: [usize; 4]) -> [f32; 4] {
    let mut physical = [0.0; 4];

    for (logical_index, physical_output) in logical_to_physical.into_iter().enumerate() {
        if (1..=4).contains(&physical_output) {
            physical[physical_output - 1] = logical[logical_index];
        }
    }

    physical
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

    #[test]
    fn motor_outputs_follow_a_one_based_permutation() {
        assert_eq!(
            remap_motor_outputs([10.0, 20.0, 30.0, 40.0], [3, 4, 2, 1]),
            [40.0, 30.0, 10.0, 20.0]
        );
        assert_eq!(
            remap_motor_outputs([10.0, 20.0, 30.0, 40.0], [1, 2, 3, 4]),
            [10.0, 20.0, 30.0, 40.0]
        );
    }
}
