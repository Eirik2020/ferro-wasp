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

/// Converts physical body angular rates into the established FCU3 rate/mixer
/// convention.
///
/// The prototype controller predates `DroneBodyFrame`: its positive pitch
/// command/output produces nose-down torque, while the physical body-frame
/// convention names nose-up pitch positive. Roll and yaw already agree. Keep
/// this compatibility transform explicit instead of hiding it in a board's
/// measured sensor-to-body rotation. This is a signed-axis reflection, not a
/// physical rotation; the existing `FrameRotation` type is the bounded
/// signed-permutation representation used for both.
pub const BODY_RATE_TO_RATE_CONTROLLER_MAP: FrameRotation =
    FrameRotation::new([0, 1, 2], [1, -1, 1]);

/// Converts the established FCU3 rate/mixer convention back into physical
/// body angular rates. The compatibility transform is a single-axis sign
/// reversal and is therefore its own inverse.
pub const RATE_CONTROLLER_TO_BODY_MAP: FrameRotation = BODY_RATE_TO_RATE_CONTROLLER_MAP;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRotation {
    pub source_axes: [usize; 3],
    pub signs: [i32; 3],
}

impl FrameRotation {
    /// Leaves all axes unchanged.
    pub const IDENTITY: Self = Self::new([0, 1, 2], [1, 1, 1]);

    /// Applies a positive 90-degree right-hand rotation around +Z.
    ///
    /// Output X comes from negative input Y, output Y comes from input X, and
    /// output Z remains unchanged.
    pub const ROTATE_Z_90: Self = Self::new([1, 0, 2], [-1, 1, 1]);

    /// Applies a 180-degree rotation around +Z.
    pub const ROTATE_Z_180: Self = Self::new([0, 1, 2], [-1, -1, 1]);

    /// Applies a positive 270-degree right-hand rotation around +Z.
    ///
    /// This is equivalent to a negative 90-degree rotation around +Z.
    pub const ROTATE_Z_270: Self = Self::new([1, 0, 2], [1, -1, 1]);

    /// Turns the sensor upside down around +X while preserving its forward axis.
    ///
    /// Output X remains unchanged while output Y and Z are inverted. The name
    /// includes the preserved axis because "upside down" alone does not specify
    /// the sensor's yaw orientation.
    pub const UPSIDE_DOWN_X_FORWARD: Self = Self::new([0, 1, 2], [1, -1, -1]);

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

    /// Applies this rotation while retaining an `i16` representation.
    ///
    /// Negating `i16::MIN` cannot be represented, so that one boundary value
    /// saturates to `i16::MAX` instead of overflowing.
    pub fn map_i16_saturating(self, input: [i16; 3]) -> [i16; 3] {
        self.map_i16_to_i32(input)
            .map(|value| value.clamp(i16::MIN as i32, i16::MAX as i32) as i16)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuControlAxisProfile {
    pub gyro_raw_to_dps: u32,
    pub drone_body_frame: DroneBodyFrame,
    pub imu_to_board_rotation: FrameRotation,
    pub board_to_drone_rotation: FrameRotation,
    pub bias_calibration_samples: u32,
    pub bias_calibration_max_raw: i32,
}

impl ImuControlAxisProfile {
    pub const fn imu_to_drone_rotation(self) -> FrameRotation {
        self.imu_to_board_rotation
            .then(self.board_to_drone_rotation)
    }

    /// Maps sensor angular rates into the established FCU3 rate/mixer sign
    /// convention. This remains distinct from the physical sensor-to-body
    /// rotation used by acceleration and orientation reporting.
    pub const fn imu_to_rate_controller_map(self) -> FrameRotation {
        self.imu_to_drone_rotation()
            .then(BODY_RATE_TO_RATE_CONTROLLER_MAP)
    }

    /// Converts sensor specific force into the drone-frame gravity direction
    /// consumed by the current complementary attitude estimator.
    pub fn sensor_accel_to_drone_gravity(self, sensor_accel: [f32; 3]) -> [f32; 3] {
        let specific_force = self.imu_to_drone_rotation().map_f32(sensor_accel);
        [-specific_force[0], -specific_force[1], -specific_force[2]]
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
    fn named_orthogonal_rotations_have_explicit_axis_mappings() {
        let input = [10, 20, 30];

        assert_eq!(FrameRotation::ROTATE_Z_90.map_i32(input), [-20, 10, 30]);
        assert_eq!(FrameRotation::ROTATE_Z_180.map_i32(input), [-10, -20, 30]);
        assert_eq!(FrameRotation::ROTATE_Z_270.map_i32(input), [20, -10, 30]);
        assert_eq!(
            FrameRotation::UPSIDE_DOWN_X_FORWARD.map_i32(input),
            [10, -20, -30]
        );
    }

    #[test]
    fn four_quarter_turns_around_z_return_to_identity() {
        let half_turn = FrameRotation::ROTATE_Z_90.then(FrameRotation::ROTATE_Z_90);
        let full_turn = half_turn.then(half_turn);

        assert_eq!(half_turn, FrameRotation::ROTATE_Z_180);
        assert_eq!(full_turn, FrameRotation::IDENTITY);
    }

    #[test]
    fn signed_axis_permutation_maps_values() {
        let rotation = FrameRotation::new([1, 0, 2], [1, -1, -1]);

        assert_eq!(rotation.map_i16_to_i32([10, 20, -30]), [20, -10, 30]);
        assert_eq!(rotation.map_i32([10, 20, -30]), [20, -10, 30]);
        assert_eq!(rotation.map_f32([10.0, 20.0, -30.0]), [20.0, -10.0, 30.0]);
        assert_eq!(rotation.map_i16_saturating([10, 20, -30]), [20, -10, 30]);
    }

    #[test]
    fn i16_rotation_saturates_the_unrepresentable_negated_minimum() {
        let rotation = FrameRotation::new([0, 1, 2], [-1, 1, 1]);

        assert_eq!(
            rotation.map_i16_saturating([i16::MIN, 0, 0]),
            [i16::MAX, 0, 0]
        );
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

    #[test]
    fn controller_compatibility_only_inverts_physical_pitch_rate() {
        assert_eq!(
            BODY_RATE_TO_RATE_CONTROLLER_MAP.map_i32([10, 20, 30]),
            [10, -20, 30]
        );
        assert_eq!(
            BODY_RATE_TO_RATE_CONTROLLER_MAP.then(RATE_CONTROLLER_TO_BODY_MAP),
            FrameRotation::IDENTITY
        );
    }

    #[test]
    fn imu_profile_composes_board_and_controller_frames() {
        let profile = ImuControlAxisProfile {
            gyro_raw_to_dps: 164,
            drone_body_frame: DroneBodyFrame::ForwardRightDown,
            imu_to_board_rotation: FrameRotation::new([1, 0, 2], [-1, -1, -1]),
            board_to_drone_rotation: FrameRotation::IDENTITY,
            bias_calibration_samples: 800,
            bias_calibration_max_raw: 1_000,
        };

        assert_eq!(
            profile.imu_to_drone_rotation().map_raw([10, 20, -30]),
            [-20, -10, 30]
        );
        assert_eq!(
            profile.imu_to_rate_controller_map().map_raw([10, 20, -30]),
            [-20, 10, 30]
        );
        assert_eq!(
            profile.sensor_accel_to_drone_gravity([0.0, 0.0, 1.0]),
            [0.0, 0.0, 1.0]
        );
    }
}
