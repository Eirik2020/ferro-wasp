//! DMA and SPI routes. On the STM32H743 any stream can serve any request
//! through DMAMUX1, so a route's `channel` is the DMAMUX request line.

pub use super::serial::SERIAL_ROUTES;
use ferrowasp_stm32f4::board_routes::{DmaDirection, DmaRoute, SpiRoute};

pub const ACTIVE_IO_DMA_ROUTES: [DmaRoute; 6] = [
    DmaRoute {
        controller: 1,
        stream: 0,
        channel: 71,
        direction: DmaDirection::PeripheralToMemory,
        owner: "UART6 RC RX",
    },
    DmaRoute {
        controller: 1,
        stream: 6,
        channel: 72,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "UART6 RC TX (CRSF telemetry)",
    },
    DmaRoute {
        controller: 1,
        stream: 1,
        channel: 45,
        direction: DmaDirection::PeripheralToMemory,
        owner: "USART3 MSP RX",
    },
    DmaRoute {
        controller: 1,
        stream: 3,
        channel: 46,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "USART3 MSP TX",
    },
    DmaRoute {
        controller: 1,
        stream: 4,
        channel: 37,
        direction: DmaDirection::PeripheralToMemory,
        owner: "SPI1 IMU RX",
    },
    DmaRoute {
        controller: 1,
        stream: 5,
        channel: 38,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "SPI1 IMU TX",
    },
];

pub const MOTOR_DSHOT_DMA_ROUTES: [DmaRoute; 4] = [
    DmaRoute {
        controller: 2,
        stream: 0,
        channel: 25,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 1 TIM3_CH3 DShot",
    },
    DmaRoute {
        controller: 2,
        stream: 1,
        channel: 26,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 2 TIM3_CH4 DShot",
    },
    DmaRoute {
        controller: 2,
        stream: 2,
        channel: 55,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 3 TIM5_CH1 DShot",
    },
    DmaRoute {
        controller: 2,
        stream: 3,
        channel: 56,
        direction: DmaDirection::MemoryToPeripheral,
        owner: "Motor 4 TIM5_CH2 DShot",
    },
];

pub const ESC_TELEMETRY_DMA_ROUTE: DmaRoute = DmaRoute {
    controller: 1,
    stream: 2,
    channel: 81,
    direction: DmaDirection::PeripheralToMemory,
    owner: "UART8 BLHeli ESC telemetry RX",
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
    mosi_pin: "PD7 AF5",
    cs_pin: "PC15 GPIO output",
    mode: 3,
    rx_dma: "DMA1 Stream 4 DMAMUX request 37",
    tx_dma: "DMA1 Stream 5 DMAMUX request 38",
    device: "WHO_AM_I probe; MPU6500 0x70, ICM42688-P 0x47 or MPU-6000 0x68 data path",
};

pub const ACTIVE_SPI_ROUTES: &[SpiRoute] = &[SPI1_IMU];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::manifest::CLAIMS;
    use ferrowasp_stm32f4::board_manifest::{ResourceKind, find_duplicate_claim};

    #[test]
    fn active_routes_have_no_exclusive_claim_conflicts() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
        assert_eq!(ACTIVE_DMA_ROUTES.len(), 11);
        for (index, route) in ACTIVE_DMA_ROUTES.iter().enumerate() {
            assert!(!ACTIVE_DMA_ROUTES[index + 1..].iter().any(|other| {
                other.controller == route.controller && other.stream == route.stream
            }));
        }
    }

    #[test]
    fn dshot_and_telemetry_routes_are_active() {
        assert_eq!(&ACTIVE_DMA_ROUTES[5..9], &MOTOR_DSHOT_DMA_ROUTES);
        assert_eq!(ACTIVE_DMA_ROUTES[9], ESC_TELEMETRY_DMA_ROUTE);
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
