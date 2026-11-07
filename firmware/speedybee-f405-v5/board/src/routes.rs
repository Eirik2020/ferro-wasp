pub use super::serial::SERIAL_ROUTES;
use ferrowasp_stm32::board_routes::{DmaDirection, DmaRoute, SpiRoute};

pub const ACTIVE_IO_DMA_ROUTES: [DmaRoute; 6] = [
    DmaRoute {
        controller: 2,
        stream: 5,
        channel: 4,
        direction: DmaDirection::PeripheralToMemory,
        owner: "USART1 configurator RX",
    },
    DmaRoute {
        controller: 2,
        stream: 7,
        channel: 4,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "USART1 configurator TX",
    },
    DmaRoute {
        controller: 2,
        stream: 2,
        channel: 5,
        direction: DmaDirection::PeripheralToMemory,
        owner: "USART6 receiver RX",
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

// Every motor lane rides DMA1: TIM3's channels on streams 2 and 7, TIM2's on
// streams 1 and 6 — four distinct streams, so the four DShot frames pace
// independently and DMA2 stays with the IMU, the UARTs and the ADC.
pub const MOTOR_DSHOT_DMA_ROUTES: [DmaRoute; 4] = [
    DmaRoute {
        controller: 1,
        stream: 2,
        channel: 5,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 1 TIM3_CH4 DShot",
    },
    DmaRoute {
        controller: 1,
        stream: 7,
        channel: 5,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 2 TIM3_CH3 DShot",
    },
    DmaRoute {
        controller: 1,
        stream: 1,
        channel: 3,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 3 TIM2_CH3 DShot",
    },
    DmaRoute {
        controller: 1,
        stream: 6,
        channel: 3,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 4 TIM2_CH4 DShot",
    },
];

pub const ESC_TELEMETRY_DMA_ROUTE: DmaRoute = DmaRoute {
    controller: 1,
    stream: 0,
    channel: 4,
    direction: DmaDirection::PeripheralToMemory,
    owner: "UART5 BLHeli ESC telemetry RX",
};

pub const ACTIVE_DMA_ROUTES: [DmaRoute; 11] = [
    ACTIVE_IO_DMA_ROUTES[0],
    ACTIVE_IO_DMA_ROUTES[1],
    ACTIVE_IO_DMA_ROUTES[2],
    ACTIVE_IO_DMA_ROUTES[3],
    ACTIVE_IO_DMA_ROUTES[4],
    ACTIVE_IO_DMA_ROUTES[5],
    MOTOR_DSHOT_DMA_ROUTES[0],
    MOTOR_DSHOT_DMA_ROUTES[1],
    MOTOR_DSHOT_DMA_ROUTES[2],
    MOTOR_DSHOT_DMA_ROUTES[3],
    ESC_TELEMETRY_DMA_ROUTE,
];

pub const SPI1_IMU: SpiRoute = SpiRoute {
    peripheral: "SPI1",
    sck_pin: "PA5 AF5",
    miso_pin: "PA6 AF5",
    mosi_pin: "PA7 AF5",
    cs_pin: "PA4 GPIO output",
    mode: 3,
    rx_dma: "DMA2 Stream 0 Channel 3",
    tx_dma: "DMA2 Stream 3 Channel 3",
    device: "WHO_AM_I probe; the fitted ICM42688-P answers 0x47",
};

pub const SPI3_FLASH: SpiRoute = SpiRoute {
    peripheral: "SPI3",
    sck_pin: "PB3 AF6",
    miso_pin: "PB4 AF6",
    mosi_pin: "PB5 AF6",
    cs_pin: "PC12 GPIO output",
    mode: 0,
    rx_dma: "none; bounded priority-1 CPU transaction",
    tx_dma: "none; bounded priority-1 CPU transaction",
    device: "onboard JEDEC SPI NOR blackbox and configuration storage",
};

pub const ACTIVE_SPI_ROUTES: &[SpiRoute] = &[SPI1_IMU, SPI3_FLASH];

#[cfg(test)]
mod tests {
    use super::super::manifest::CLAIMS;
    use super::*;
    use ferrowasp_stm32::board_manifest::{ResourceKind, find_duplicate_claim};

    #[test]
    fn active_routes_have_no_exclusive_claim_conflicts() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
        assert_eq!(ACTIVE_DMA_ROUTES.len(), 11);
        assert_eq!(ACTIVE_SPI_ROUTES.len(), 2);
        for (index, route) in ACTIVE_DMA_ROUTES.iter().enumerate() {
            assert!(!ACTIVE_DMA_ROUTES[index + 1..].iter().any(|other| {
                other.controller == route.controller && other.stream == route.stream
            }));
        }
    }

    #[test]
    fn dshot_telemetry_and_flash_routes_are_active() {
        assert_eq!(&ACTIVE_DMA_ROUTES[6..10], &MOTOR_DSHOT_DMA_ROUTES);
        assert_eq!(ACTIVE_DMA_ROUTES[10], ESC_TELEMETRY_DMA_ROUTE);
        assert_eq!(ACTIVE_SPI_ROUTES[1], SPI3_FLASH);
        for route in MOTOR_DSHOT_DMA_ROUTES {
            assert_eq!(route.controller, 1);
        }
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
