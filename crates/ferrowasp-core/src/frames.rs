#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DroneBodyFrame {
    /// Body frame used by flight-control logic:
    /// +X forward, +Y right, +Z down.
    ///
    /// Angular rates use the right-hand rule about those axes:
    /// positive roll lowers the right side, positive pitch raises the nose,
    /// and positive yaw turns the nose right when viewed from above.
    ForwardRightDown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRotation {
    pub source_axes: [usize; 3],
    pub signs: [i32; 3],
}

impl FrameRotation {
    pub const IDENTITY: Self = Self::new([0, 1, 2], [1, 1, 1]);

    /// Creates a signed axis permutation.
    ///
    /// `source_axes[n]` selects the input axis used for output axis `n`, and
    /// `signs[n]` applies the direction. For example, `[1, 0, 2]` with
    /// `[1, -1, 1]` maps output X from input Y, output Y from negative input X,
    /// and output Z from input Z.
    pub const fn new(source_axes: [usize; 3], signs: [i32; 3]) -> Self {
        Self { source_axes, signs }
    }

    /// Returns `other` after `self`.
    ///
    /// If `self` maps frame A to frame B and `other` maps frame B to frame C,
    /// the result maps frame A directly to frame C.
    pub const fn then(self, other: Self) -> Self {
        let source_0 = self.source_axes[other.source_axes[0]];
        let source_1 = self.source_axes[other.source_axes[1]];
        let source_2 = self.source_axes[other.source_axes[2]];

        let sign_0 = self.signs[other.source_axes[0]] * other.signs[0];
        let sign_1 = self.signs[other.source_axes[1]] * other.signs[1];
        let sign_2 = self.signs[other.source_axes[2]] * other.signs[2];

        Self::new([source_0, source_1, source_2], [sign_0, sign_1, sign_2])
    }

    pub fn map_i16_to_i32(self, input: [i16; 3]) -> [i32; 3] {
        [
            input[self.source_axes[0]] as i32 * self.signs[0],
            input[self.source_axes[1]] as i32 * self.signs[1],
            input[self.source_axes[2]] as i32 * self.signs[2],
        ]
    }

    pub fn map_raw(self, input: [i16; 3]) -> [i32; 3] {
        self.map_i16_to_i32(input)
    }

    pub fn map_i32(self, input: [i32; 3]) -> [i32; 3] {
        [
            input[self.source_axes[0]] * self.signs[0],
            input[self.source_axes[1]] * self.signs[1],
            input[self.source_axes[2]] * self.signs[2],
        ]
    }

    pub fn map_f32(self, input: [f32; 3]) -> [f32; 3] {
        [
            input[self.source_axes[0]] * self.signs[0] as f32,
            input[self.source_axes[1]] * self.signs[1] as f32,
            input[self.source_axes[2]] * self.signs[2] as f32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_preserves_axes() {
        assert_eq!(
            FrameRotation::IDENTITY.map_i16_to_i32([10, -20, 30]),
            [10, -20, 30]
        );
    }

    #[test]
    fn signed_axis_permutation_maps_values() {
        let rotation = FrameRotation::new([1, 0, 2], [1, -1, -1]);

        assert_eq!(rotation.map_i16_to_i32([10, 20, -30]), [20, -10, 30]);
        assert_eq!(rotation.map_i32([10, 20, -30]), [20, -10, 30]);
        assert_eq!(rotation.map_f32([10.0, 20.0, -30.0]), [20.0, -10.0, 30.0]);
    }

    #[test]
    fn composition_applies_second_rotation_after_first() {
        let imu_to_board = FrameRotation::new([2, 0, 1], [1, -1, 1]);
        let board_to_drone = FrameRotation::new([1, 0, 2], [1, 1, -1]);

        let imu_to_drone = imu_to_board.then(board_to_drone);

        assert_eq!(imu_to_drone, FrameRotation::new([0, 2, 1], [-1, 1, -1]));
        assert_eq!(
            imu_to_drone.map_i16_to_i32([10, 20, 30]),
            board_to_drone.map_i32(imu_to_board.map_i16_to_i32([10, 20, 30]))
        );
    }

    #[test]
    fn body_frame_names_the_control_convention() {
        assert_eq!(
            DroneBodyFrame::ForwardRightDown,
            DroneBodyFrame::ForwardRightDown
        );
    }
}
