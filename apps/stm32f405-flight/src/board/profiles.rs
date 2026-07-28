use ferrowasp_core::frames::{DroneBodyFrame, FrameRotation, ImuControlAxisProfile};

pub const DSHOT_IDLE_TUNING_MAX_COMMAND: u16 = 250;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DshotMotorRouteProfile {
    /// One-based physical FCU output number, not the logical airframe motor.
    pub motor: u8,
    pub pin: &'static str,
    pub alternate: u8,
    pub timer_channel: &'static str,
    pub dma_route: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DshotFourMotorProfile {
    pub routes: [DshotMotorRouteProfile; 4],
    pub bitrate_hz: u32,
    pub active_motor_outputs: u8,
    pub timer_sync: &'static str,
}

pub const DSHOT_FOUR_MOTOR_PROFILE: DshotFourMotorProfile = DshotFourMotorProfile {
    routes: [
        DshotMotorRouteProfile {
            motor: 1,
            pin: "PA8",
            alternate: 1,
            timer_channel: "TIM1_CH1",
            dma_route: "DMA2 Stream1 Channel6",
        },
        DshotMotorRouteProfile {
            motor: 2,
            pin: "PC9",
            alternate: 3,
            timer_channel: "TIM8_CH4",
            dma_route: "DMA2 Stream7 Channel7",
        },
        DshotMotorRouteProfile {
            motor: 3,
            pin: "PC8",
            alternate: 3,
            timer_channel: "TIM8_CH3",
            dma_route: "DMA2 Stream4 Channel7",
        },
        DshotMotorRouteProfile {
            motor: 4,
            pin: "PB15",
            alternate: 1,
            timer_channel: "TIM1_CH3N",
            dma_route: "DMA2 Stream6 Channel6 (CCR3)",
        },
    ],
    bitrate_hz: 600_000,
    active_motor_outputs: 4,
    timer_sync: "TIM1 CEN TRGO -> TIM8 ITR0 trigger mode",
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdcObservationProfile {
    pub vbat_divider_ratio: f32,
    pub current_betaflight_scale: u32,
    pub battery_cell_count: u8,
}

pub const ADC_OBSERVATION_PROFILE: AdcObservationProfile = AdcObservationProfile {
    vbat_divider_ratio: 8.5,
    current_betaflight_scale: 70,
    battery_cell_count: 6,
};

pub const IMU_CONTROL_AXIS_PROFILE: ImuControlAxisProfile = ImuControlAxisProfile {
    gyro_raw_to_dps: 164,
    drone_body_frame: DroneBodyFrame::ForwardRightDown,
    imu_to_board_rotation: FrameRotation::IDENTITY,
    board_to_drone_rotation: FrameRotation::new([1, 0, 2], [1, -1, -1]),
    bias_calibration_samples: 800,
    bias_calibration_max_raw: 1000,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adc_profile_preserves_current_fcu_scaling() {
        assert_eq!(ADC_OBSERVATION_PROFILE.vbat_divider_ratio, 8.5);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_betaflight_scale, 70);
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_cell_count, 6);
    }

    #[test]
    fn imu_control_axis_profile_preserves_current_mapping() {
        assert_eq!(IMU_CONTROL_AXIS_PROFILE.gyro_raw_to_dps, 164);
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.drone_body_frame,
            DroneBodyFrame::ForwardRightDown
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.imu_to_board_rotation,
            FrameRotation::IDENTITY
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.board_to_drone_rotation,
            FrameRotation::new([1, 0, 2], [1, -1, -1])
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE
                .imu_to_drone_rotation()
                .map_raw([10, 20, -30]),
            [20, -10, 30]
        );
        assert_eq!(IMU_CONTROL_AXIS_PROFILE.bias_calibration_samples, 800);
        assert_eq!(IMU_CONTROL_AXIS_PROFILE.bias_calibration_max_raw, 1000);
    }

    #[test]
    fn dshot_profile_preserves_all_fcu3_motor_pins_and_routes() {
        assert_eq!(
            DSHOT_FOUR_MOTOR_PROFILE.routes.map(|route| route.pin),
            ["PA8", "PC9", "PC8", "PB15"]
        );
        assert_eq!(
            DSHOT_FOUR_MOTOR_PROFILE
                .routes
                .map(|route| (route.timer_channel, route.dma_route)),
            [
                ("TIM1_CH1", "DMA2 Stream1 Channel6"),
                ("TIM8_CH4", "DMA2 Stream7 Channel7"),
                ("TIM8_CH3", "DMA2 Stream4 Channel7"),
                ("TIM1_CH3N", "DMA2 Stream6 Channel6 (CCR3)"),
            ]
        );
        assert_eq!(DSHOT_FOUR_MOTOR_PROFILE.bitrate_hz, 600_000);
        assert_eq!(DSHOT_FOUR_MOTOR_PROFILE.active_motor_outputs, 4);
        assert!(DSHOT_FOUR_MOTOR_PROFILE.timer_sync.contains("ITR0"));
    }
}
