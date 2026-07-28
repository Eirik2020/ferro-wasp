use ferrowasp_stm32f4::board_routes::{DmaDirection, DmaRoute, SpiRoute};

pub const ACTIVE_IO_DMA_ROUTES: [DmaRoute; 7] = [
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

pub const ACTIVE_DMA_ROUTES: [DmaRoute; 11] = [
    ACTIVE_IO_DMA_ROUTES[0],
    ACTIVE_IO_DMA_ROUTES[1],
    ACTIVE_IO_DMA_ROUTES[2],
    ACTIVE_IO_DMA_ROUTES[3],
    ACTIVE_IO_DMA_ROUTES[4],
    ACTIVE_IO_DMA_ROUTES[5],
    ACTIVE_IO_DMA_ROUTES[6],
    MOTOR_DSHOT_DMA_ROUTES[0],
    MOTOR_DSHOT_DMA_ROUTES[1],
    MOTOR_DSHOT_DMA_ROUTES[2],
    MOTOR_DSHOT_DMA_ROUTES[3],
];

pub const MOTOR1_DSHOT_DMA_ROUTE: DmaRoute = MOTOR_DSHOT_DMA_ROUTES[0];

pub const SPI1_MPU6500: SpiRoute = SpiRoute {
    peripheral: "SPI1",
    sck_pin: "PA5 AF5",
    miso_pin: "PA6 AF5",
    mosi_pin: "PA7 AF5",
    cs_pin: "PA4 GPIO output",
    mode: 3,
    rx_dma: "DMA2 Stream 2 Channel 3",
    tx_dma: "DMA2 Stream 3 Channel 3",
    device: "MPU6500 primary IMU",
};

pub const ACTIVE_SPI_ROUTES: &[SpiRoute] = &[SPI1_MPU6500];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::manifest::CLAIMS;
    use ferrowasp_stm32f4::board_manifest::ResourceKind;

    #[test]
    fn route_counts_match_the_active_fcu3_manifest() {
        assert_eq!(ACTIVE_DMA_ROUTES.len(), 11);
        assert_eq!(ACTIVE_SPI_ROUTES.len(), 1);
    }

    #[test]
    fn dshot_routes_are_active_and_exclusive() {
        assert_eq!(&ACTIVE_DMA_ROUTES[7..], &MOTOR_DSHOT_DMA_ROUTES);
        for (index, route) in ACTIVE_DMA_ROUTES.iter().enumerate() {
            assert!(!ACTIVE_DMA_ROUTES[index + 1..].iter().any(|other| {
                other.controller == route.controller && other.stream == route.stream
            }));
        }
        assert_eq!(MOTOR1_DSHOT_DMA_ROUTE, MOTOR_DSHOT_DMA_ROUTES[0]);
    }

    #[test]
    fn every_active_dma_route_has_a_manifest_claim() {
        for route in ACTIVE_DMA_ROUTES {
            assert!(CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::DmaStream
                    && route.matches_manifest_claim(claim.resource)
            }));
        }
    }
}
