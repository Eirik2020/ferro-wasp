//! Reusable declarations describing an STM32F4 board's physical hardware.

use std::collections::BTreeSet;

use super::{
    gpio::GpioHardwareDeclaration,
    mcu::{ClockSource, McuDeclaration},
    serial::{SerialPeripheral, SerialPort, SerialRoute},
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

/// Physical hardware registry for one board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDeclaration {
    /// Stable board identifier.
    pub id: &'static str,

    /// Selected MCU and clock configuration.
    pub mcu: McuDeclaration,

    /// GPIO hardware available for task-local ownership.
    pub gpio: &'static [GpioHardwareDeclaration],

    /// Serial hardware available for component consumption.
    pub serial: &'static [SerialHardwareDeclaration],
}

impl BoardDeclaration {
    /// Creates a board declaration from its MCU and serial hardware registry.
    pub const fn new(
        id: &'static str,
        mcu: McuDeclaration,
        serial: &'static [SerialHardwareDeclaration],
    ) -> Self {
        Self {
            id,
            mcu,
            gpio: &[],
            serial,
        }
    }

    /// Adds the board's named GPIO registry.
    pub const fn with_gpio(mut self, gpio: &'static [GpioHardwareDeclaration]) -> Self {
        self.gpio = gpio;
        self
    }

    /// Finds GPIO hardware by its stable board-local identifier.
    pub fn gpio(&self, id: &str) -> Option<&GpioHardwareDeclaration> {
        self.gpio.iter().find(|hardware| hardware.id == id)
    }

    /// Finds serial hardware by its stable board-local identifier.
    pub fn serial(&self, id: &str) -> Option<&SerialHardwareDeclaration> {
        self.serial.iter().find(|hardware| hardware.id == id)
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

    for hardware in board.serial {
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
    };

    const STM32F405_HSE: McuDeclaration = McuDeclaration::new(
        Mcu::Stm32f405,
        ClockDeclaration::hse(8_000_000, 168_000_000),
    );

    const UART4: SerialHardwareDeclaration =
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
            ));

    const LED2: GpioHardwareDeclaration =
        GpioHardwareDeclaration::output("led2", PinId::new(GpioPort::A, 5), Level::Low);
    const USER_BUTTON: GpioHardwareDeclaration =
        GpioHardwareDeclaration::input("user_button", PinId::new(GpioPort::C, 13))
            .pull_up()
            .interrupt_on(InterruptEdge::Falling);

    #[test]
    fn valid_hse_board_supports_serial_lookup() {
        const BOARD: BoardDeclaration =
            BoardDeclaration::new("selected_board", STM32F405_HSE, &[UART4])
                .with_gpio(&[LED2, USER_BUTTON]);

        validate(&BOARD).unwrap();
        assert_eq!(BOARD.gpio("led2"), Some(&LED2));
        assert_eq!(BOARD.serial("uart4"), Some(&UART4));
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
        const CONFLICT: SerialHardwareDeclaration =
            SerialHardwareDeclaration::new("other", SerialPeripheral::Usart1).rx(SerialRoute::dma(
                PinId::new(GpioPort::B, 7),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream2,
                    DmaChannel::Channel3,
                ),
            ));
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
