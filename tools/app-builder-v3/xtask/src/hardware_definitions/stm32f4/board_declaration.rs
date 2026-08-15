//! Reusable declarations describing an STM32F4 board's physical hardware.

use std::collections::BTreeSet;

pub use ferrowasp_core::frames::FrameRotation as ImuOrientation;
pub use ferrowasp_io_core::platform_config::ImuInstallationId;

use super::{
    dshot::DshotBankHardwareDeclaration,
    gpio::{GpioHardwareDeclaration, InterruptEdge},
    mcu::{ClockSource, McuDeclaration},
    pins::PinId,
    serial::{SerialPeripheral, SerialPort, SerialRoute},
    service_hardware::{
        AdcObservationHardwareDeclaration, AdcPeripheral, SpiNorHardwareDeclaration,
        UsbCdcHardwareDeclaration, UsbPeripheral,
    },
    spi::{SpiBus, SpiPeripheral},
    timer::TimerHardwareDeclaration,
};

/// One named serial-port resource physically present on a board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialHardwareDeclaration {
    /// Stable identifier used by component declarations and diagnostics.
    pub id: &'static str,

    /// Physical UART peripheral, pins, and DMA routes.
    pub port: SerialPort,
}

impl SerialHardwareDeclaration {
    /// Creates a named serial-port declaration without signal routes.
    pub const fn new(id: &'static str, peripheral: SerialPeripheral) -> Self {
        Self {
            id,
            port: SerialPort::new(peripheral),
        }
    }

    /// Assigns the receive route.
    pub const fn rx(mut self, route: SerialRoute) -> Self {
        self.port = self.port.rx(route);
        self
    }

    /// Assigns the transmit route.
    pub const fn tx(mut self, route: SerialRoute) -> Self {
        self.port = self.port.tx(route);
        self
    }
}

/// One named SPI bus physically present on a board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiHardwareDeclaration {
    /// Stable identifier used by device declarations.
    pub id: &'static str,

    /// Physical peripheral, pins, and DMA routes.
    pub bus: SpiBus,

    /// IMUs physically installed on this SPI endpoint.
    pub imu: &'static [ImuInstallationDeclaration],
}

impl SpiHardwareDeclaration {
    /// Creates a named SPI bus declaration.
    pub const fn new(id: &'static str, bus: SpiBus) -> Self {
        Self { id, bus, imu: &[] }
    }

    /// Sets the complete list of IMUs physically installed on this SPI endpoint.
    pub const fn with_imus(mut self, imu: &'static [ImuInstallationDeclaration]) -> Self {
        self.imu = imu;
        self
    }

    /// Finds an installed IMU by its stable board-local identifier.
    pub fn imu(&self, id: ImuInstallationId) -> Option<&ImuInstallationDeclaration> {
        self.imu.iter().find(|installation| installation.id == id)
    }
}

/// One transport endpoint exposed by a board.
///
/// Every endpoint category enters [`BoardDeclaration`] through this common
/// registry. Category-specific declarations retain their strongly typed route
/// data after lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareEndpointDeclaration {
    /// ADC observation endpoint.
    AdcObservation(AdcObservationHardwareDeclaration),

    /// UART/USART endpoint.
    Serial(SerialHardwareDeclaration),

    /// SPI bus endpoint.
    Spi(SpiHardwareDeclaration),

    /// CPU-serviced SPI NOR installation.
    SpiNor(SpiNorHardwareDeclaration),

    /// USB CDC device endpoint.
    UsbCdc(UsbCdcHardwareDeclaration),
}

impl HardwareEndpointDeclaration {
    /// Adds a typed ADC observation declaration to the common registry.
    pub const fn adc_observation(adc: AdcObservationHardwareDeclaration) -> Self {
        Self::AdcObservation(adc)
    }

    /// Adds a fully typed serial declaration to the common endpoint registry.
    pub const fn serial(serial: SerialHardwareDeclaration) -> Self {
        Self::Serial(serial)
    }

    /// Adds a fully typed SPI declaration to the common endpoint registry.
    pub const fn spi(spi: SpiHardwareDeclaration) -> Self {
        Self::Spi(spi)
    }

    /// Adds a CPU-serviced SPI NOR declaration to the common registry.
    pub const fn spi_nor(spi_nor: SpiNorHardwareDeclaration) -> Self {
        Self::SpiNor(spi_nor)
    }

    /// Adds a USB CDC declaration to the common registry.
    pub const fn usb_cdc(usb: UsbCdcHardwareDeclaration) -> Self {
        Self::UsbCdc(usb)
    }

    /// Returns the stable board-local endpoint identifier.
    pub const fn id(self) -> &'static str {
        match self {
            Self::AdcObservation(adc) => adc.id,
            Self::Serial(serial) => serial.id,
            Self::Spi(spi) => spi.id,
            Self::SpiNor(spi_nor) => spi_nor.id,
            Self::UsbCdc(usb) => usb.id,
        }
    }
}

/// One auto-detected MPU6500 or ICM42688-P installed on an SPI endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuInstallationDeclaration {
    /// Stable numeric identifier consumed by boot-time platform configuration.
    pub id: ImuInstallationId,

    /// GPIO chip-select pin driven by the SPI transport.
    pub chip_select: PinId,

    /// GPIO interrupt pin asserted when a sample is ready.
    pub data_ready: PinId,

    /// Electrical edge used by the data-ready interrupt.
    pub data_ready_edge: InterruptEdge,

    /// Signed axis permutation from the sensor frame into the body frame.
    pub orientation: ImuOrientation,
}

impl ImuInstallationDeclaration {
    /// Creates a complete physical IMU installation declaration.
    pub const fn new(
        id: ImuInstallationId,
        chip_select: PinId,
        data_ready: PinId,
        data_ready_edge: InterruptEdge,
        orientation: ImuOrientation,
    ) -> Self {
        Self {
            id,
            chip_select,
            data_ready,
            data_ready_edge,
            orientation,
        }
    }
}

/// Physical hardware registry for one board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDeclaration {
    /// Stable board identifier.
    pub id: &'static str,

    /// Selected MCU and clock configuration.
    pub mcu: McuDeclaration,

    /// GPIO hardware available for task-local ownership.
    pub gpio: &'static [GpioHardwareDeclaration],

    /// Named timer peripherals available for explicit application consumers.
    pub timers: &'static [TimerHardwareDeclaration],

    /// Indivisible advanced-timer/DMA motor-output banks.
    pub dshot_banks: &'static [DshotBankHardwareDeclaration],

    /// Transport endpoints available for component and service consumption.
    pub endpoints: &'static [HardwareEndpointDeclaration],
}

impl BoardDeclaration {
    /// Creates a board declaration from its MCU and transport endpoint registry.
    pub const fn new(
        id: &'static str,
        mcu: McuDeclaration,
        endpoints: &'static [HardwareEndpointDeclaration],
    ) -> Self {
        Self {
            id,
            mcu,
            gpio: &[],
            timers: &[],
            dshot_banks: &[],
            endpoints,
        }
    }

    /// Sets the board's complete named GPIO registry.
    pub const fn with_gpio(mut self, gpio: &'static [GpioHardwareDeclaration]) -> Self {
        self.gpio = gpio;
        self
    }

    /// Sets the board's complete named timer registry.
    pub const fn with_timers(mut self, timers: &'static [TimerHardwareDeclaration]) -> Self {
        self.timers = timers;
        self
    }

    /// Sets the board's complete physical DShot-bank registry.
    pub const fn with_dshot_banks(
        mut self,
        banks: &'static [DshotBankHardwareDeclaration],
    ) -> Self {
        self.dshot_banks = banks;
        self
    }

    /// Finds GPIO hardware by its stable board-local identifier.
    pub fn gpio(&self, id: &str) -> Option<&GpioHardwareDeclaration> {
        self.gpio.iter().find(|hardware| hardware.id == id)
    }

    /// Finds timer hardware by its stable board-local identifier.
    pub fn timer(&self, id: &str) -> Option<&TimerHardwareDeclaration> {
        self.timers.iter().find(|hardware| hardware.id == id)
    }

    /// Finds one physical DShot bank by its stable board-local identifier.
    pub fn dshot_bank(&self, id: &str) -> Option<&DshotBankHardwareDeclaration> {
        self.dshot_banks.iter().find(|hardware| hardware.id == id)
    }

    /// Finds serial hardware by its stable board-local identifier.
    pub fn serial(&self, id: &str) -> Option<&SerialHardwareDeclaration> {
        self.serial_endpoints().find(|hardware| hardware.id == id)
    }

    /// Finds an SPI bus by its stable board-local identifier.
    pub fn spi(&self, id: &str) -> Option<&SpiHardwareDeclaration> {
        self.spi_endpoints().find(|hardware| hardware.id == id)
    }

    /// Finds an ADC observation endpoint by its stable board-local identifier.
    pub fn adc_observation(&self, id: &str) -> Option<&AdcObservationHardwareDeclaration> {
        self.adc_observations().find(|hardware| hardware.id == id)
    }

    /// Finds a CPU-serviced SPI NOR installation.
    pub fn spi_nor(&self, id: &str) -> Option<&SpiNorHardwareDeclaration> {
        self.spi_nor_endpoints().find(|hardware| hardware.id == id)
    }

    /// Finds a USB CDC endpoint.
    pub fn usb_cdc(&self, id: &str) -> Option<&UsbCdcHardwareDeclaration> {
        self.usb_cdc_endpoints().find(|hardware| hardware.id == id)
    }

    /// Iterates over all serial endpoints in declaration order.
    pub fn serial_endpoints(&self) -> impl Iterator<Item = &SerialHardwareDeclaration> {
        self.endpoints.iter().filter_map(|endpoint| match endpoint {
            HardwareEndpointDeclaration::Serial(serial) => Some(serial),
            HardwareEndpointDeclaration::AdcObservation(_)
            | HardwareEndpointDeclaration::Spi(_)
            | HardwareEndpointDeclaration::SpiNor(_)
            | HardwareEndpointDeclaration::UsbCdc(_) => None,
        })
    }

    /// Iterates over all SPI endpoints in declaration order.
    pub fn spi_endpoints(&self) -> impl Iterator<Item = &SpiHardwareDeclaration> {
        self.endpoints.iter().filter_map(|endpoint| match endpoint {
            HardwareEndpointDeclaration::Serial(_) => None,
            HardwareEndpointDeclaration::Spi(spi) => Some(spi),
            HardwareEndpointDeclaration::AdcObservation(_)
            | HardwareEndpointDeclaration::SpiNor(_)
            | HardwareEndpointDeclaration::UsbCdc(_) => None,
        })
    }

    /// Iterates over typed ADC observation endpoints.
    pub fn adc_observations(&self) -> impl Iterator<Item = &AdcObservationHardwareDeclaration> {
        self.endpoints.iter().filter_map(|endpoint| match endpoint {
            HardwareEndpointDeclaration::AdcObservation(adc) => Some(adc),
            _ => None,
        })
    }

    /// Iterates over CPU-serviced SPI NOR endpoints.
    pub fn spi_nor_endpoints(&self) -> impl Iterator<Item = &SpiNorHardwareDeclaration> {
        self.endpoints.iter().filter_map(|endpoint| match endpoint {
            HardwareEndpointDeclaration::SpiNor(spi_nor) => Some(spi_nor),
            _ => None,
        })
    }

    /// Iterates over USB CDC endpoints.
    pub fn usb_cdc_endpoints(&self) -> impl Iterator<Item = &UsbCdcHardwareDeclaration> {
        self.endpoints.iter().filter_map(|endpoint| match endpoint {
            HardwareEndpointDeclaration::UsbCdc(usb) => Some(usb),
            _ => None,
        })
    }

    /// Finds an installed IMU and its owning SPI endpoint.
    pub fn imu(
        &self,
        id: ImuInstallationId,
    ) -> Option<(&SpiHardwareDeclaration, &ImuInstallationDeclaration)> {
        self.spi_endpoints()
            .find_map(|spi| spi.imu(id).map(|installation| (spi, installation)))
    }
}

pub(crate) fn validate(board: &BoardDeclaration) -> Result<(), String> {
    validate_identifier(board.id)
        .map_err(|()| format!("board ID `{}` is not a valid Rust identifier", board.id))?;
    if board.mcu.clock.system_frequency_hz == 0 {
        return Err(format!(
            "board `{}` system clock frequency must be nonzero",
            board.id
        ));
    }
    if let ClockSource::Hse { frequency_hz: 0 } = board.mcu.clock.source {
        return Err(format!(
            "board `{}` HSE input frequency must be nonzero",
            board.id
        ));
    }

    let mut ids = BTreeSet::new();
    let mut peripherals = BTreeSet::new();
    let mut pins = BTreeSet::new();
    let mut dma_streams = BTreeSet::new();

    for hardware in board.gpio {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` GPIO hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        if !pins.insert(hardware.pin) {
            return Err(format!(
                "board `{}` assigns physical pin `{:?}` more than once",
                board.id, hardware.pin
            ));
        }
    }

    let mut timer_peripherals = BTreeSet::new();
    for hardware in board.timers {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` timer hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        if !timer_peripherals.insert(hardware.peripheral) {
            return Err(format!(
                "board `{}` declares timer peripheral `{:?}` more than once",
                board.id, hardware.peripheral
            ));
        }
    }

    for hardware in board.dshot_banks {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` DShot bank ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        let mut outputs = BTreeSet::new();
        let mut logical_motors = BTreeSet::new();
        let mut timer_channels = BTreeSet::new();
        for lane in hardware.lanes {
            if !(1..=4).contains(&lane.physical_output) || !outputs.insert(lane.physical_output) {
                return Err(format!(
                    "board DShot bank `{}` has an invalid or repeated physical output {}",
                    hardware.id, lane.physical_output
                ));
            }
            if !(1..=4).contains(&lane.logical_motor) || !logical_motors.insert(lane.logical_motor)
            {
                return Err(format!(
                    "board DShot bank `{}` has an invalid or repeated logical motor {}",
                    hardware.id, lane.logical_motor
                ));
            }
            if !timer_channels.insert(lane.timer_channel) {
                return Err(format!(
                    "board DShot bank `{}` assigns timer channel `{:?}` more than once",
                    hardware.id, lane.timer_channel
                ));
            }
            if !pins.insert(lane.pin) {
                return Err(format!(
                    "board `{}` assigns DShot pin `{:?}` more than once",
                    board.id, lane.pin
                ));
            }
            if !dma_streams.insert((lane.dma.controller, lane.dma.stream)) {
                return Err(format!(
                    "board `{}` assigns `{:?} {:?}` to more than one DMA signal",
                    board.id, lane.dma.controller, lane.dma.stream
                ));
            }
        }
    }

    for hardware in board.serial_endpoints() {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` serial hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        if !peripherals.insert(hardware.port.peripheral) {
            return Err(format!(
                "board `{}` assigns peripheral `{:?}` to more than one serial declaration",
                board.id, hardware.port.peripheral
            ));
        }
        if hardware.port.rx.is_none() && hardware.port.tx.is_none() {
            return Err(format!(
                "board serial hardware `{}` declares neither RX nor TX",
                hardware.id
            ));
        }

        for route in [hardware.port.rx, hardware.port.tx].into_iter().flatten() {
            if !pins.insert(route.pin) {
                return Err(format!(
                    "board `{}` assigns physical pin `{:?}` more than once",
                    board.id, route.pin
                ));
            }
            if let Some(dma) = route.dma
                && !dma_streams.insert((dma.controller, dma.stream))
            {
                return Err(format!(
                    "board `{}` assigns `{:?} {:?}` to more than one serial signal",
                    board.id, dma.controller, dma.stream
                ));
            }
        }
    }

    let mut spi_peripherals = BTreeSet::<SpiPeripheral>::new();
    let mut imu_ids = BTreeSet::<ImuInstallationId>::new();
    for hardware in board.spi_endpoints() {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` SPI hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        if !spi_peripherals.insert(hardware.bus.peripheral) {
            return Err(format!(
                "board `{}` assigns SPI peripheral `{:?}` more than once",
                board.id, hardware.bus.peripheral
            ));
        }
        for pin in [
            hardware.bus.pins.sck,
            hardware.bus.pins.miso,
            hardware.bus.pins.mosi,
        ] {
            if !pins.insert(pin) {
                return Err(format!(
                    "board `{}` assigns physical pin `{:?}` more than once",
                    board.id, pin
                ));
            }
        }
        for dma in [hardware.bus.dma.rx, hardware.bus.dma.tx] {
            if !dma_streams.insert((dma.controller, dma.stream)) {
                return Err(format!(
                    "board `{}` assigns `{:?} {:?}` to more than one DMA signal",
                    board.id, dma.controller, dma.stream
                ));
            }
        }

        for installation in hardware.imu {
            if installation.id.get() == 0 {
                return Err(format!(
                    "board SPI hardware `{}` declares reserved IMU installation ID 0",
                    hardware.id
                ));
            }
            if !imu_ids.insert(installation.id) {
                return Err(format!(
                    "board `{}` declares IMU installation ID {} more than once",
                    board.id,
                    installation.id.get()
                ));
            }
            validate_imu_orientation(installation.orientation).map_err(|reason| {
                format!(
                    "board SPI hardware `{}` IMU installation {} has invalid orientation: {reason}",
                    hardware.id,
                    installation.id.get()
                )
            })?;
            for pin in [installation.chip_select, installation.data_ready] {
                if !pins.insert(pin) {
                    return Err(format!(
                        "board `{}` assigns physical pin `{:?}` more than once",
                        board.id, pin
                    ));
                }
            }
        }
    }

    let mut adc_peripherals = BTreeSet::<AdcPeripheral>::new();
    for hardware in board.adc_observations() {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` ADC hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        if !adc_peripherals.insert(hardware.peripheral) {
            return Err(format!(
                "board `{}` assigns ADC peripheral `{:?}` more than once",
                board.id, hardware.peripheral
            ));
        }
        if hardware.voltage_channel > 15
            || hardware.current_channel > 15
            || hardware.voltage_channel == hardware.current_channel
        {
            return Err(format!(
                "board ADC hardware `{}` has invalid or repeated ADC channels",
                hardware.id
            ));
        }
        for pin in [hardware.voltage_pin, hardware.current_pin] {
            if !pins.insert(pin) {
                return Err(format!(
                    "board `{}` assigns physical pin `{:?}` more than once",
                    board.id, pin
                ));
            }
        }
        if !dma_streams.insert((hardware.dma.controller, hardware.dma.stream)) {
            return Err(format!(
                "board `{}` assigns `{:?} {:?}` to more than one DMA signal",
                board.id, hardware.dma.controller, hardware.dma.stream
            ));
        }
    }

    for hardware in board.spi_nor_endpoints() {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` SPI NOR hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        if !spi_peripherals.insert(hardware.peripheral) {
            return Err(format!(
                "board `{}` assigns SPI peripheral `{:?}` more than once",
                board.id, hardware.peripheral
            ));
        }
        if hardware.frequency_hz == 0 {
            return Err(format!(
                "board SPI NOR hardware `{}` frequency must be nonzero",
                hardware.id
            ));
        }
        for pin in [
            hardware.sck,
            hardware.miso,
            hardware.mosi,
            hardware.chip_select,
        ] {
            if !pins.insert(pin) {
                return Err(format!(
                    "board `{}` assigns physical pin `{:?}` more than once",
                    board.id, pin
                ));
            }
        }
    }

    let mut usb_peripherals = BTreeSet::<UsbPeripheral>::new();
    for hardware in board.usb_cdc_endpoints() {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` USB hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares hardware resource `{}` more than once",
                board.id, hardware.id
            ));
        }
        if !usb_peripherals.insert(hardware.peripheral) {
            return Err(format!(
                "board `{}` assigns USB peripheral `{:?}` more than once",
                board.id, hardware.peripheral
            ));
        }
        for pin in [hardware.dm, hardware.dp] {
            if !pins.insert(pin) {
                return Err(format!(
                    "board `{}` assigns physical pin `{:?}` more than once",
                    board.id, pin
                ));
            }
        }
        if hardware.identity.manufacturer.is_empty()
            || hardware.identity.product.is_empty()
            || hardware.identity.serial_number.is_empty()
        {
            return Err(format!(
                "board USB hardware `{}` identity strings must be nonempty",
                hardware.id
            ));
        }
    }

    Ok(())
}

fn validate_imu_orientation(orientation: ImuOrientation) -> Result<(), &'static str> {
    let mut axes = BTreeSet::new();
    for axis in orientation.source_axes {
        if axis > 2 {
            return Err("source axes must be in the range 0 through 2");
        }
        if !axes.insert(axis) {
            return Err("each source axis must be used exactly once");
        }
    }
    if orientation
        .signs
        .into_iter()
        .any(|sign| !matches!(sign, -1 | 1))
    {
        return Err("axis signs must be either -1 or 1");
    }
    Ok(())
}

fn validate_identifier(id: &str) -> Result<(), ()> {
    let mut characters = id.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic());
    let valid_tail =
        characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    (valid_start && valid_tail).then_some(()).ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_definitions::stm32f4::{
        dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
        gpio::{GpioHardwareDeclaration, InterruptEdge, Level},
        mcu::{ClockDeclaration, Mcu, McuDeclaration},
        pins::{GpioPort, PinId},
        spi::{SpiBus, SpiDmaRoutes, SpiPeripheral, SpiPins},
        timer::{TimerHardwareDeclaration, TimerPeripheral},
    };

    const STM32F405_HSE: McuDeclaration = McuDeclaration::new(
        Mcu::Stm32f405,
        ClockDeclaration::hse(8_000_000, 168_000_000),
    );

    const UART4: HardwareEndpointDeclaration = HardwareEndpointDeclaration::serial(
        SerialHardwareDeclaration::new("uart4", SerialPeripheral::Uart4)
            .rx(SerialRoute::dma(
                PinId::new(GpioPort::A, 1),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream2,
                    DmaChannel::Channel4,
                ),
            ))
            .tx(SerialRoute::dma(
                PinId::new(GpioPort::A, 0),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream4,
                    DmaChannel::Channel4,
                ),
            )),
    );

    const LED2: GpioHardwareDeclaration =
        GpioHardwareDeclaration::output("led2", PinId::new(GpioPort::A, 5), Level::Low);
    const USER_BUTTON: GpioHardwareDeclaration =
        GpioHardwareDeclaration::input("user_button", PinId::new(GpioPort::C, 13))
            .pull_up()
            .interrupt_on(InterruptEdge::Falling);

    const IMU_1: ImuInstallationDeclaration = ImuInstallationDeclaration::new(
        ImuInstallationId::new(1),
        PinId::new(GpioPort::A, 4),
        PinId::new(GpioPort::C, 4),
        InterruptEdge::Rising,
        ImuOrientation::IDENTITY,
    );

    const INIT_DELAY_TIMER: TimerHardwareDeclaration =
        TimerHardwareDeclaration::new("tim5", TimerPeripheral::Tim5);

    const SPI1: HardwareEndpointDeclaration = HardwareEndpointDeclaration::spi(
        SpiHardwareDeclaration::new(
            "spi1",
            SpiBus::new(
                SpiPeripheral::Spi1,
                SpiPins::new(
                    PinId::new(GpioPort::A, 5),
                    PinId::new(GpioPort::A, 6),
                    PinId::new(GpioPort::A, 7),
                ),
                SpiDmaRoutes::new(
                    DmaRoute::new(
                        DmaController::Dma2,
                        DmaStream::Stream0,
                        DmaChannel::Channel3,
                    ),
                    DmaRoute::new(
                        DmaController::Dma2,
                        DmaStream::Stream3,
                        DmaChannel::Channel3,
                    ),
                ),
            ),
        )
        .with_imus(&[IMU_1]),
    );

    #[test]
    fn valid_hse_board_supports_serial_lookup() {
        const BOARD: BoardDeclaration =
            BoardDeclaration::new("selected_board", STM32F405_HSE, &[UART4])
                .with_gpio(&[LED2, USER_BUTTON]);

        validate(&BOARD).unwrap();
        assert_eq!(BOARD.gpio("led2"), Some(&LED2));
        assert_eq!(BOARD.serial("uart4").map(|serial| serial.id), Some("uart4"));
    }

    #[test]
    fn valid_hsi_clock_is_accepted() {
        const BOARD: BoardDeclaration = BoardDeclaration::new(
            "hsi_board",
            McuDeclaration::new(Mcu::Stm32f405, ClockDeclaration::hsi(16_000_000)),
            &[],
        );

        validate(&BOARD).unwrap();
    }

    #[test]
    fn foxeer_spi_imu_routes_are_valid_and_resolvable() {
        const BOARD: BoardDeclaration =
            BoardDeclaration::new("foxeer_subset", STM32F405_HSE, &[UART4, SPI1])
                .with_timers(&[INIT_DELAY_TIMER]);

        validate(&BOARD).unwrap();
        assert_eq!(BOARD.spi("spi1").map(|spi| spi.id), Some("spi1"));
        assert_eq!(
            BOARD.timer("tim5").map(|timer| timer.peripheral),
            Some(TimerPeripheral::Tim5)
        );
        assert_eq!(
            BOARD
                .imu(ImuInstallationId::new(1))
                .map(|(spi, installation)| (spi.id, installation)),
            Some(("spi1", &IMU_1))
        );
    }

    #[test]
    fn multiple_imus_do_not_require_an_endpoint_owned_timer() {
        const IMU_2: ImuInstallationDeclaration = ImuInstallationDeclaration::new(
            ImuInstallationId::new(2),
            PinId::new(GpioPort::B, 0),
            PinId::new(GpioPort::B, 1),
            InterruptEdge::Rising,
            ImuOrientation::ROTATE_Z_90,
        );
        const MULTI_IMU_SPI1: HardwareEndpointDeclaration = match SPI1 {
            HardwareEndpointDeclaration::Spi(spi) => {
                HardwareEndpointDeclaration::Spi(spi.with_imus(&[IMU_1, IMU_2]))
            }
            _ => unreachable!(),
        };
        const BOARD: BoardDeclaration =
            BoardDeclaration::new("multi_imu", STM32F405_HSE, &[UART4, MULTI_IMU_SPI1]);

        validate(&BOARD).unwrap();
        let spi = BOARD.spi("spi1").unwrap();
        assert_eq!(spi.imu.len(), 2);
    }

    #[test]
    fn one_timer_peripheral_cannot_be_declared_twice() {
        const BOARD: BoardDeclaration =
            BoardDeclaration::new("duplicate_timer", STM32F405_HSE, &[]).with_timers(&[
                TimerHardwareDeclaration::new("first", TimerPeripheral::Tim2),
                TimerHardwareDeclaration::new("second", TimerPeripheral::Tim2),
            ]);

        let error = validate(&BOARD).unwrap_err();
        assert!(error.contains("declares timer peripheral `Tim2` more than once"));
    }

    #[test]
    fn zero_imu_installation_id_is_rejected() {
        const ZERO_ID_IMU: ImuInstallationDeclaration = ImuInstallationDeclaration::new(
            ImuInstallationId::new(0),
            PinId::new(GpioPort::A, 4),
            PinId::new(GpioPort::C, 4),
            InterruptEdge::Rising,
            ImuOrientation::IDENTITY,
        );
        const INVALID_SPI: HardwareEndpointDeclaration = match SPI1 {
            HardwareEndpointDeclaration::Spi(spi) => {
                HardwareEndpointDeclaration::Spi(spi.with_imus(&[ZERO_ID_IMU]))
            }
            _ => unreachable!(),
        };
        const INVALID: BoardDeclaration =
            BoardDeclaration::new("zero_imu_id", STM32F405_HSE, &[INVALID_SPI]);

        let error = validate(&INVALID).unwrap_err();
        assert!(error.contains("reserved IMU installation ID 0"));
    }

    #[test]
    fn orientation_must_use_each_sensor_axis_once() {
        const INVALID_IMU: ImuInstallationDeclaration = ImuInstallationDeclaration::new(
            ImuInstallationId::new(1),
            PinId::new(GpioPort::A, 4),
            PinId::new(GpioPort::C, 4),
            InterruptEdge::Rising,
            ImuOrientation::new([0, 0, 2], [1, 1, 1]),
        );
        const INVALID_SPI: HardwareEndpointDeclaration = match SPI1 {
            HardwareEndpointDeclaration::Spi(spi) => {
                HardwareEndpointDeclaration::Spi(spi.with_imus(&[INVALID_IMU]))
            }
            _ => unreachable!(),
        };
        const INVALID: BoardDeclaration =
            BoardDeclaration::new("invalid_orientation", STM32F405_HSE, &[INVALID_SPI]);

        let error = validate(&INVALID).unwrap_err();
        assert!(error.contains("each source axis must be used exactly once"));
    }

    #[test]
    fn zero_system_clock_frequency_is_rejected() {
        const BOARD: BoardDeclaration = BoardDeclaration::new(
            "zero_system_clock",
            McuDeclaration::new(Mcu::Stm32f405, ClockDeclaration::hsi(0)),
            &[],
        );

        let error = validate(&BOARD).unwrap_err();
        assert!(error.contains("system clock frequency must be nonzero"));
    }

    #[test]
    fn zero_hse_input_frequency_is_rejected() {
        const BOARD: BoardDeclaration = BoardDeclaration::new(
            "zero_hse_input",
            McuDeclaration::new(Mcu::Stm32f405, ClockDeclaration::hse(0, 168_000_000)),
            &[],
        );

        let error = validate(&BOARD).unwrap_err();
        assert!(error.contains("HSE input frequency must be nonzero"));
    }

    #[test]
    fn duplicate_dma_stream_is_rejected_even_with_a_different_channel() {
        const CONFLICT: HardwareEndpointDeclaration = HardwareEndpointDeclaration::serial(
            SerialHardwareDeclaration::new("other", SerialPeripheral::Usart1).rx(SerialRoute::dma(
                PinId::new(GpioPort::B, 7),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream2,
                    DmaChannel::Channel3,
                ),
            )),
        );
        const INVALID: BoardDeclaration =
            BoardDeclaration::new("invalid", STM32F405_HSE, &[UART4, CONFLICT]);

        let error = validate(&INVALID).unwrap_err();
        assert!(error.contains("Stream2"));
    }

    #[test]
    fn duplicate_hardware_id_across_categories_is_rejected() {
        const CONFLICT: GpioHardwareDeclaration =
            GpioHardwareDeclaration::output("uart4", PinId::new(GpioPort::B, 0), Level::Low);
        const INVALID: BoardDeclaration =
            BoardDeclaration::new("invalid", STM32F405_HSE, &[UART4]).with_gpio(&[CONFLICT]);

        let error = validate(&INVALID).unwrap_err();
        assert!(error.contains("hardware resource `uart4` more than once"));
    }

    #[test]
    fn gpio_cannot_reuse_a_serial_pin() {
        const CONFLICT: GpioHardwareDeclaration =
            GpioHardwareDeclaration::output("other", PinId::new(GpioPort::A, 0), Level::Low);
        const INVALID: BoardDeclaration =
            BoardDeclaration::new("invalid", STM32F405_HSE, &[UART4]).with_gpio(&[CONFLICT]);

        let error = validate(&INVALID).unwrap_err();
        assert!(error.contains("assigns physical pin"));
    }
}
