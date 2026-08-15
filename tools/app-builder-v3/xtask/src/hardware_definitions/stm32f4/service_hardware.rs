//! Board-owned physical declarations for mandatory Foxeer flight services.

use super::{dma_route::DmaRoute, pins::PinId, spi::SpiPeripheral};

/// Supported ADC peripheral.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AdcPeripheral {
    /// ADC peripheral 1.
    Adc1,
}

/// Exact voltage/current ADC observation route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdcObservationHardwareDeclaration {
    /// Stable board-local identifier.
    pub id: &'static str,
    /// ADC peripheral.
    pub peripheral: AdcPeripheral,
    /// Voltage-sense pin.
    pub voltage_pin: PinId,
    /// Current-sense pin.
    pub current_pin: PinId,
    /// Voltage ADC channel number.
    pub voltage_channel: u8,
    /// Current ADC channel number.
    pub current_channel: u8,
    /// Peripheral-to-memory DMA route.
    pub dma: DmaRoute,
}

impl AdcObservationHardwareDeclaration {
    /// Creates one complete ADC observation declaration.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: &'static str,
        peripheral: AdcPeripheral,
        voltage_pin: PinId,
        voltage_channel: u8,
        current_pin: PinId,
        current_channel: u8,
        dma: DmaRoute,
    ) -> Self {
        Self {
            id,
            peripheral,
            voltage_pin,
            current_pin,
            voltage_channel,
            current_channel,
            dma,
        }
    }
}

/// SPI electrical mode selected for a non-DMA device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiMode {
    /// Clock idle low, sample on the first transition.
    Mode0,
}

/// Exact CPU-serviced onboard SPI NOR route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiNorHardwareDeclaration {
    /// Stable board-local identifier.
    pub id: &'static str,
    /// SPI peripheral.
    pub peripheral: SpiPeripheral,
    /// Clock pin.
    pub sck: PinId,
    /// Controller-input/peripheral-output pin.
    pub miso: PinId,
    /// Controller-output/peripheral-input pin.
    pub mosi: PinId,
    /// GPIO chip-select pin.
    pub chip_select: PinId,
    /// Reviewed device mode.
    pub mode: SpiMode,
    /// Maximum selected bus clock.
    pub frequency_hz: u32,
}

impl SpiNorHardwareDeclaration {
    /// Creates one complete SPI NOR installation.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: &'static str,
        peripheral: SpiPeripheral,
        sck: PinId,
        miso: PinId,
        mosi: PinId,
        chip_select: PinId,
        mode: SpiMode,
        frequency_hz: u32,
    ) -> Self {
        Self {
            id,
            peripheral,
            sck,
            miso,
            mosi,
            chip_select,
            mode,
            frequency_hz,
        }
    }
}

/// Supported USB device peripheral.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum UsbPeripheral {
    /// STM32F405 full-speed OTG peripheral in device mode.
    OtgFs,
}

/// Stable USB CDC identity strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbCdcIdentityDeclaration {
    /// USB manufacturer string.
    pub manufacturer: &'static str,
    /// USB product string.
    pub product: &'static str,
    /// USB serial string.
    pub serial_number: &'static str,
}

/// Exact USB full-speed route and identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbCdcHardwareDeclaration {
    /// Stable board-local identifier.
    pub id: &'static str,
    /// USB peripheral.
    pub peripheral: UsbPeripheral,
    /// D- pin.
    pub dm: PinId,
    /// D+ pin.
    pub dp: PinId,
    /// Stable product identity.
    pub identity: UsbCdcIdentityDeclaration,
}

impl UsbCdcHardwareDeclaration {
    /// Creates one complete USB CDC route.
    pub const fn new(
        id: &'static str,
        peripheral: UsbPeripheral,
        dm: PinId,
        dp: PinId,
        identity: UsbCdcIdentityDeclaration,
    ) -> Self {
        Self {
            id,
            peripheral,
            dm,
            dp,
            identity,
        }
    }
}
