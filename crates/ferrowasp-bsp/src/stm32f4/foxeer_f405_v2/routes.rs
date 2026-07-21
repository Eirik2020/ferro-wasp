pub use super::serial::ACTIVE_SERIAL_ROUTES;

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
        stream: 0,
        channel: 3,
        direction: DmaDirection::PeripheralToMemory,
        owner: "SPI1 IMU RX",
    },
    DmaRoute {
        controller: 2,
        stream: 3,
        channel: 3,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "SPI1 IMU TX",
    },
    DmaRoute {
        controller: 2,
        stream: 4,
        channel: 0,
        direction: DmaDirection::PeripheralToMemory,
        owner: "ADC1 battery/current observation",
    },
];

pub const DEFERRED_MOTOR_DMA_ROUTES: &[DmaRoute] = &[
    DmaRoute {
        controller: 2,
        stream: 1,
        channel: 6,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 1 TIM1_CH1 DShot deferred",
    },
    DmaRoute {
        controller: 2,
        stream: 7,
        channel: 7,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 2 TIM8_CH4 DShot deferred",
    },
    DmaRoute {
        controller: 2,
        stream: 2,
        channel: 0,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 3 TIM8_CH3 DShot deferred",
    },
    DmaRoute {
        controller: 2,
        stream: 6,
        channel: 6,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 4 TIM1_CH3N DShot deferred",
    },
];

pub const OPTIONAL_ESC_TELEMETRY_DMA_ROUTE: DmaRoute = DmaRoute {
    controller: 2,
    stream: 5,
    channel: 4,
    direction: DmaDirection::PeripheralToMemory,
    owner: "Optional USART1 BLHeli ESC telemetry RX",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiRoute {
    pub peripheral: &'static str,
    pub sck_pin: &'static str,
    pub miso_pin: &'static str,
    pub mosi_pin: &'static str,
    pub cs_pin: &'static str,
    pub mode: u8,
    pub rx_dma: &'static str,
    pub tx_dma: &'static str,
    pub device: &'static str,
}

pub const SPI1_IMU: SpiRoute = SpiRoute {
    peripheral: "SPI1",
    sck_pin: "PA5 AF5",
    miso_pin: "PA6 AF5",
    mosi_pin: "PA7 AF5",
    cs_pin: "PA4 GPIO output",
    mode: 3,
    rx_dma: "DMA2 Stream 0 Channel 3",
    tx_dma: "DMA2 Stream 3 Channel 3",
    device: "WHO_AM_I probe; MPU6500 0x70 or ICM42688-P 0x47 data path",
};

pub const OPTIONAL_SPI2_FLASH: SpiRoute = SpiRoute {
    peripheral: "SPI2",
    sck_pin: "PB13 AF5",
    miso_pin: "PC2 AF5",
    mosi_pin: "PC3 AF5",
    cs_pin: "PB12 GPIO output",
    mode: 0,
    rx_dma: "none; bounded priority-1 CPU transaction",
    tx_dma: "none; DMA1 Stream4 remains owned by UART4 TX",
    device: "optional JEDEC SPI NOR flight log and configuration storage",
};

pub const ACTIVE_SPI_ROUTES: &[SpiRoute] = &[SPI1_IMU];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmOutputKind {
    Main,
    Complementary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticPwmRoute {
    pub motor: u8,
    pub pin: &'static str,
    pub timer_channel: &'static str,
    pub output_kind: PwmOutputKind,
    pub logical_lane: &'static str,
}

pub const ACTIVE_STATIC_PWM_ROUTES: &[StaticPwmRoute] = &[
    StaticPwmRoute {
        motor: 1,
        pin: "PA8",
        timer_channel: "TIM1_CH1",
        output_kind: PwmOutputKind::Main,
        logical_lane: "logical motor 1 / rear-right",
    },
    StaticPwmRoute {
        motor: 2,
        pin: "PC9",
        timer_channel: "TIM8_CH4",
        output_kind: PwmOutputKind::Main,
        logical_lane: "logical motor 2 / front-right",
    },
    StaticPwmRoute {
        motor: 3,
        pin: "PC8",
        timer_channel: "TIM8_CH3",
        output_kind: PwmOutputKind::Main,
        logical_lane: "logical motor 3 / rear-left",
    },
    StaticPwmRoute {
        motor: 4,
        pin: "PB15",
        timer_channel: "TIM1_CH3N",
        output_kind: PwmOutputKind::Complementary,
        logical_lane: "logical motor 4 / front-left",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ResourceKind, find_duplicate_claim};
    use crate::stm32f4::foxeer_f405_v2::manifest::CLAIMS;

    #[test]
    fn active_routes_have_no_exclusive_claim_conflicts() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
        assert_eq!(ACTIVE_DMA_ROUTES.len(), 6);
        assert_eq!(ACTIVE_STATIC_PWM_ROUTES.len(), 4);
    }

    #[test]
    fn active_dma_allocation_leaves_all_four_deferred_motor_streams_free() {
        for active in ACTIVE_DMA_ROUTES {
            for motor in DEFERRED_MOTOR_DMA_ROUTES {
                assert_ne!(
                    (active.controller, active.stream),
                    (motor.controller, motor.stream)
                );
            }
        }
    }

    #[test]
    fn optional_esc_telemetry_dma_does_not_conflict_with_active_or_dshot_routes() {
        for route in ACTIVE_DMA_ROUTES.iter().chain(DEFERRED_MOTOR_DMA_ROUTES) {
            assert_ne!(
                (route.controller, route.stream),
                (
                    OPTIONAL_ESC_TELEMETRY_DMA_ROUTE.controller,
                    OPTIONAL_ESC_TELEMETRY_DMA_ROUTE.stream
                )
            );
        }
        assert_eq!(
            (
                OPTIONAL_ESC_TELEMETRY_DMA_ROUTE.controller,
                OPTIONAL_ESC_TELEMETRY_DMA_ROUTE.stream,
                OPTIONAL_ESC_TELEMETRY_DMA_ROUTE.channel,
            ),
            (2, 5, 4)
        );
    }

    #[test]
    fn optional_flash_route_preserves_validated_serial_dma() {
        assert_eq!(OPTIONAL_SPI2_FLASH.peripheral, "SPI2");
        assert_eq!(OPTIONAL_SPI2_FLASH.cs_pin, "PB12 GPIO output");
        assert!(OPTIONAL_SPI2_FLASH.rx_dma.starts_with("none"));
        assert!(OPTIONAL_SPI2_FLASH.tx_dma.contains("UART4 TX"));
    }

    #[test]
    fn m4_route_cannot_be_mistaken_for_an_ordinary_channel() {
        let m4 = ACTIVE_STATIC_PWM_ROUTES[3];
        assert_eq!(m4.motor, 4);
        assert_eq!(m4.pin, "PB15");
        assert_eq!(m4.timer_channel, "TIM1_CH3N");
        assert_eq!(m4.output_kind, PwmOutputKind::Complementary);
    }

    #[test]
    fn physical_routes_describe_the_provisional_foxeer_motor_order() {
        assert_eq!(
            [
                ACTIVE_STATIC_PWM_ROUTES[0].logical_lane,
                ACTIVE_STATIC_PWM_ROUTES[1].logical_lane,
                ACTIVE_STATIC_PWM_ROUTES[2].logical_lane,
                ACTIVE_STATIC_PWM_ROUTES[3].logical_lane,
            ],
            [
                "logical motor 1 / rear-right",
                "logical motor 2 / front-right",
                "logical motor 3 / rear-left",
                "logical motor 4 / front-left",
            ]
        );
    }

    #[test]
    fn every_active_dma_route_has_a_manifest_claim() {
        for route in ACTIVE_DMA_ROUTES {
            let expected = dma_claim_name(route);
            assert!(CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::DmaStream && Some(claim.resource) == expected
            }));
        }
    }

    fn dma_claim_name(route: &DmaRoute) -> Option<&'static str> {
        match (route.controller, route.stream, route.channel) {
            (1, 5, 4) => Some("DMA1_STREAM5_CH4"),
            (1, 2, 4) => Some("DMA1_STREAM2_CH4"),
            (1, 4, 4) => Some("DMA1_STREAM4_CH4"),
            (2, 0, 3) => Some("DMA2_STREAM0_CH3"),
            (2, 3, 3) => Some("DMA2_STREAM3_CH3"),
            (2, 4, 0) => Some("DMA2_STREAM4_CH0"),
            _ => None,
        }
    }
}
