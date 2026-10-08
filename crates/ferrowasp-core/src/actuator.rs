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

/// A pilot's motor order: which board output each logical mixer motor drives,
/// on top of the board's own wiring map.
///
/// Stored as four digits, one per logical motor in Betaflight Quad X order
/// (rear-right, front-right, rear-left, front-left), each naming a position
/// in the board map. `1234` keeps the board's wiring unchanged. One value
/// rather than four, so a swap is a single change and never a transient
/// duplicate that would drive two motors from one command.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MotorOutputMap {
    /// One-based board-map positions for logical motors 1-4.
    order: [u8; 4],
}

impl MotorOutputMap {
    /// The board's wiring, unchanged.
    pub const IDENTITY: Self = Self {
        order: [1, 2, 3, 4],
    };

    /// `None` unless the four digits are a permutation of 1-4.
    pub const fn from_config(value: u16) -> Option<Self> {
        if value < 1000 || value > 9999 {
            return None;
        }
        let digits = [
            (value / 1000) % 10,
            (value / 100) % 10,
            (value / 10) % 10,
            value % 10,
        ];
        let mut order = [0_u8; 4];
        let mut seen = [false; 4];
        let mut index = 0;
        while index < 4 {
            let digit = digits[index];
            if digit < 1 || digit > 4 || seen[(digit - 1) as usize] {
                return None;
            }
            seen[(digit - 1) as usize] = true;
            order[index] = digit as u8;
            index += 1;
        }
        Some(Self { order })
    }

    /// The four-digit configuration value.
    pub const fn to_config(self) -> u16 {
        self.order[0] as u16 * 1000
            + self.order[1] as u16 * 100
            + self.order[2] as u16 * 10
            + self.order[3] as u16
    }

    /// The logical-to-physical map the mixer uses: this order applied to the
    /// board's wiring map. With [`Self::IDENTITY`] it is the board map.
    pub const fn compose(self, board: [usize; 4]) -> [usize; 4] {
        [
            board[self.order[0] as usize - 1],
            board[self.order[1] as usize - 1],
            board[self.order[2] as usize - 1],
            board[self.order[3] as usize - 1],
        ]
    }
}

/// The one-based physical output a one-based logical motor drives, or `None`
/// for a motor number outside 1-4 or a map entry outside 1-4.
pub const fn physical_output_for_logical(map: [usize; 4], logical_motor: u8) -> Option<usize> {
    if logical_motor < 1 || logical_motor > 4 {
        return None;
    }
    let output = map[logical_motor as usize - 1];
    if output < 1 || output > 4 {
        None
    } else {
        Some(output)
    }
}

/// Packs a logical-to-physical map into four digits for a lock-free snapshot;
/// `0` means no map has been published.
pub const fn pack_motor_map(map: [usize; 4]) -> u16 {
    (map[0] * 1000 + map[1] * 100 + map[2] * 10 + map[3]) as u16
}

/// The inverse of [`pack_motor_map`]; `None` for `0` or anything that is not
/// a permutation of 1-4.
pub const fn unpack_motor_map(packed: u16) -> Option<[usize; 4]> {
    match MotorOutputMap::from_config(packed) {
        Some(map) => Some(map.compose([1, 2, 3, 4])),
        None => None,
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

    #[test]
    fn motor_map_accepts_only_permutations() {
        assert_eq!(
            MotorOutputMap::from_config(1234),
            Some(MotorOutputMap::IDENTITY)
        );
        assert_eq!(
            MotorOutputMap::from_config(4321).map(MotorOutputMap::to_config),
            Some(4321)
        );
        for invalid in [0, 999, 1123, 1235, 5123, 10000, u16::MAX] {
            assert_eq!(MotorOutputMap::from_config(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn identity_order_keeps_the_board_map() {
        let board = [3, 4, 2, 1];
        assert_eq!(MotorOutputMap::IDENTITY.compose(board), board);
    }

    /// Swapping logical motors 1 and 2 swaps which board outputs they drive,
    /// whatever the board wiring is.
    #[test]
    fn a_swap_moves_board_outputs_between_logical_motors() {
        let board = [3, 4, 2, 1];
        let swapped = MotorOutputMap::from_config(2134).unwrap();
        assert_eq!(swapped.compose(board), [4, 3, 2, 1]);
    }

    #[test]
    fn logical_motor_lookup_rejects_out_of_range() {
        let map = [3, 4, 2, 1];
        assert_eq!(physical_output_for_logical(map, 1), Some(3));
        assert_eq!(physical_output_for_logical(map, 4), Some(1));
        assert_eq!(physical_output_for_logical(map, 0), None);
        assert_eq!(physical_output_for_logical(map, 5), None);
        assert_eq!(physical_output_for_logical([0, 2, 3, 4], 1), None);
    }

    #[test]
    fn packed_maps_round_trip_and_zero_is_unpublished() {
        assert_eq!(
            unpack_motor_map(pack_motor_map([3, 4, 2, 1])),
            Some([3, 4, 2, 1])
        );
        assert_eq!(unpack_motor_map(0), None);
    }
}
