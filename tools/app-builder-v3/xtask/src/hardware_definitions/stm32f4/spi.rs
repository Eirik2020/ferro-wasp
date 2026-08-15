//! Declarative STM32F4 SPI bus routes.

use super::{dma_route::DmaRoute, pins::PinId};

/// Identifies one STM32F4 SPI peripheral.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SpiPeripheral {
    /// SPI peripheral 1.
    Spi1,
    /// SPI peripheral 2.
    Spi2,
    /// SPI peripheral 3.
    Spi3,
}

/// Physical signal pins selected for one SPI bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiPins {
    /// Clock pin.
    pub sck: PinId,
    /// Controller-input/peripheral-output pin.
    pub miso: PinId,
    /// Controller-output/peripheral-input pin.
    pub mosi: PinId,
}

impl SpiPins {
    /// Creates a complete SPI signal-pin declaration.
    pub const fn new(sck: PinId, miso: PinId, mosi: PinId) -> Self {
        Self { sck, miso, mosi }
    }
}

/// DMA routes selected for one full-duplex SPI bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiDmaRoutes {
    /// Receive DMA route.
    pub rx: DmaRoute,
    /// Transmit DMA route.
    pub tx: DmaRoute,
}

impl SpiDmaRoutes {
    /// Creates a complete full-duplex SPI DMA declaration.
    pub const fn new(rx: DmaRoute, tx: DmaRoute) -> Self {
        Self { rx, tx }
    }
}

/// Physical peripheral, signal pins, and DMA routes of one SPI bus.
///
/// Device-specific mode and clock frequency are deliberately excluded. They
/// are selected by the service or device driver using this physical bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiBus {
    /// Hardware SPI peripheral.
    pub peripheral: SpiPeripheral,
    /// Physical SPI signal pins.
    pub pins: SpiPins,
    /// Full-duplex DMA routes.
    pub dma: SpiDmaRoutes,
}

impl SpiBus {
    /// Creates an SPI bus declaration.
    pub const fn new(peripheral: SpiPeripheral, pins: SpiPins, dma: SpiDmaRoutes) -> Self {
        Self {
            peripheral,
            pins,
            dma,
        }
    }
}
