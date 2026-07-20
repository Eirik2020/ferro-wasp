#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DmaDirection {
    PeripheralToMemory,
    MemoryToPeripheral,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaRoute {
    pub controller: u8,
    pub stream: u8,
    pub channel: u8,
    pub direction: DmaDirection,
    pub owner: &'static str,
}

pub const ACTIVE_DMA_ROUTES: &[DmaRoute] = &[
    DmaRoute {
        controller: 2,
        stream: 5,
        channel: 4,
        direction: DmaDirection::PeripheralToMemory,
        owner: "USART1 BLHeli ESC telemetry RX",
    },
    DmaRoute {
        controller: 1,
        stream: 5,
        channel: 4,
        direction: DmaDirection::PeripheralToMemory,
        owner: "USART2 SBUS RX",
    },
    DmaRoute {
        controller: 1,
        stream: 2,
        channel: 4,
        direction: DmaDirection::PeripheralToMemory,
        owner: "UART4 MSP RX",
    },
    DmaRoute {
        controller: 1,
        stream: 4,
        channel: 4,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "UART4 MSP TX",
    },
    DmaRoute {
        controller: 2,
        stream: 2,
        channel: 3,
        direction: DmaDirection::PeripheralToMemory,
        owner: "SPI1 MPU6500 RX",
    },
    DmaRoute {
        controller: 2,
        stream: 3,
        channel: 3,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "SPI1 MPU6500 TX",
    },
    DmaRoute {
        controller: 2,
        stream: 0,
        channel: 0,
        direction: DmaDirection::PeripheralToMemory,
        owner: "ADC1 battery observation",
    },
];

pub const MOTOR_DSHOT_DMA_ROUTES: [DmaRoute; 4] = [
    DmaRoute {
        controller: 2,
        stream: 1,
        channel: 6,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 1 DShot TIM1_CH1 compare",
    },
    DmaRoute {
        controller: 2,
        stream: 7,
        channel: 7,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 2 DShot TIM8_CH4 compare",
    },
    DmaRoute {
        controller: 2,
        stream: 4,
        channel: 7,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 3 DShot TIM8_CH3 compare",
    },
    DmaRoute {
        controller: 2,
        stream: 6,
        channel: 6,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 4 DShot TIM1_CH3N compare",
    },
];

pub const MOTOR1_DSHOT_DMA_ROUTE: DmaRoute = MOTOR_DSHOT_DMA_ROUTES[0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiRoute {
    pub logical_bus: &'static str,
    pub peripheral: &'static str,
    pub sck_pin: &'static str,
    pub miso_pin: &'static str,
    pub mosi_pin: &'static str,
    pub cs_pin: &'static str,
    pub rx_dma: &'static str,
    pub tx_dma: &'static str,
    pub device: &'static str,
}

pub const SPI1_MPU6500: SpiRoute = SpiRoute {
    logical_bus: "SPI1",
    peripheral: "SPI1",
    sck_pin: "PA5 AF5",
    miso_pin: "PA6 AF5",
    mosi_pin: "PA7 AF5",
    cs_pin: "PA4 GPIO output",
    rx_dma: "DMA2 Stream 2 Channel 3",
    tx_dma: "DMA2 Stream 3 Channel 3",
    device: "MPU6500 primary IMU",
};

pub const ACTIVE_SPI_ROUTES: &[SpiRoute] = &[SPI1_MPU6500];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticPwmRoute {
    pub motor: u8,
    pub pin: &'static str,
    pub timer_channel: &'static str,
    pub logical_lane: &'static str,
}

pub const ACTIVE_STATIC_PWM_ROUTES: &[StaticPwmRoute] = &[
    StaticPwmRoute {
        motor: 1,
        pin: "PA8",
        timer_channel: "TIM1_CH1",
        logical_lane: "motor[0]",
    },
    StaticPwmRoute {
        motor: 2,
        pin: "PC9",
        timer_channel: "TIM3_CH4",
        logical_lane: "motor[1]",
    },
    StaticPwmRoute {
        motor: 3,
        pin: "PC8",
        timer_channel: "TIM3_CH3",
        logical_lane: "motor[2]",
    },
    StaticPwmRoute {
        motor: 4,
        pin: "PB15",
        timer_channel: "TIM12_CH2",
        logical_lane: "motor[3]",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ResourceClaim, ResourceKind};
    use crate::stm32f4::ferrowasp_fcu3::manifest::CLAIMS;

    #[test]
    fn route_counts_match_the_frozen_fcu3_manifest() {
        assert_eq!(ACTIVE_DMA_ROUTES.len(), 7);
        assert_eq!(ACTIVE_SPI_ROUTES.len(), 1);
        assert_eq!(ACTIVE_STATIC_PWM_ROUTES.len(), 4);
    }

    #[test]
    fn optional_four_motor_dshot_routes_use_free_dma2_streams() {
        assert_eq!(
            MOTOR_DSHOT_DMA_ROUTES.map(|route| (
                route.controller,
                route.stream,
                route.channel,
                route.direction,
            )),
            [
                (2, 1, 6, DmaDirection::MemoryToPeripheral),
                (2, 7, 7, DmaDirection::MemoryToPeripheral),
                (2, 4, 7, DmaDirection::MemoryToPeripheral),
                (2, 6, 6, DmaDirection::MemoryToPeripheral),
            ]
        );

        for (index, route) in MOTOR_DSHOT_DMA_ROUTES.iter().enumerate() {
            assert!(!ACTIVE_DMA_ROUTES.iter().any(|active| {
                active.controller == route.controller && active.stream == route.stream
            }));
            assert!(!MOTOR_DSHOT_DMA_ROUTES[index + 1..].iter().any(|other| {
                other.controller == route.controller && other.stream == route.stream
            }));
        }
    }

    #[test]
    fn dshot_profile_preserves_all_four_external_motor_pins() {
        assert_eq!(
            [
                ACTIVE_STATIC_PWM_ROUTES[0].pin,
                ACTIVE_STATIC_PWM_ROUTES[1].pin,
                ACTIVE_STATIC_PWM_ROUTES[2].pin,
                ACTIVE_STATIC_PWM_ROUTES[3].pin,
            ],
            ["PA8", "PC9", "PC8", "PB15"]
        );
        assert_eq!(MOTOR1_DSHOT_DMA_ROUTE, MOTOR_DSHOT_DMA_ROUTES[0]);
    }

    #[test]
    fn static_pwm_routes_preserve_fcu3_motor_order() {
        assert_eq!(
            [
                route_identity(ACTIVE_STATIC_PWM_ROUTES[0]),
                route_identity(ACTIVE_STATIC_PWM_ROUTES[1]),
                route_identity(ACTIVE_STATIC_PWM_ROUTES[2]),
                route_identity(ACTIVE_STATIC_PWM_ROUTES[3]),
            ],
            [
                (1, "PA8", "TIM1_CH1"),
                (2, "PC9", "TIM3_CH4"),
                (3, "PC8", "TIM3_CH3"),
                (4, "PB15", "TIM12_CH2"),
            ]
        );
    }

    #[test]
    fn every_active_dma_route_has_a_manifest_claim() {
        let expected_claims = [
            "DMA2_STREAM5_CH4",
            "DMA1_STREAM5_CH4",
            "DMA1_STREAM2_CH4",
            "DMA1_STREAM4_CH4",
            "DMA2_STREAM2_CH3",
            "DMA2_STREAM3_CH3",
            "DMA2_STREAM0",
        ];

        for (route, expected_claim) in ACTIVE_DMA_ROUTES.iter().zip(expected_claims) {
            assert_eq!(dma_claim_name(route), Some(expected_claim));
            assert!(
                CLAIMS
                    .iter()
                    .any(|claim| { is_dma_claim(claim) && claim.resource == expected_claim })
            );
        }
    }

    fn route_identity(route: StaticPwmRoute) -> (u8, &'static str, &'static str) {
        (route.motor, route.pin, route.timer_channel)
    }

    fn is_dma_claim(claim: &ResourceClaim) -> bool {
        claim.kind == ResourceKind::DmaStream
    }

    fn dma_claim_name(route: &DmaRoute) -> Option<&'static str> {
        match (route.controller, route.stream, route.channel) {
            (1, 5, 4) => Some("DMA1_STREAM5_CH4"),
            (1, 2, 4) => Some("DMA1_STREAM2_CH4"),
            (1, 4, 4) => Some("DMA1_STREAM4_CH4"),
            (2, 2, 3) => Some("DMA2_STREAM2_CH3"),
            (2, 3, 3) => Some("DMA2_STREAM3_CH3"),
            (2, 0, 0) => Some("DMA2_STREAM0"),
            (2, 5, 4) => Some("DMA2_STREAM5_CH4"),
            _ => None,
        }
    }
}
