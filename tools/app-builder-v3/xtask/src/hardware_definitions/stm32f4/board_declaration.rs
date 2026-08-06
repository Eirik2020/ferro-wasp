//! Reusable declarations describing an STM32F4 board's physical hardware.

use std::collections::BTreeSet;

use super::serial::{SerialPeripheral, SerialPort, SerialRoute};

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

    /// Serial hardware available for component consumption.
    pub serial: &'static [SerialHardwareDeclaration],
}

impl BoardDeclaration {
    /// Creates a board declaration from its serial hardware registry.
    pub const fn new(id: &'static str, serial: &'static [SerialHardwareDeclaration]) -> Self {
        Self { id, serial }
    }

    /// Finds serial hardware by its stable board-local identifier.
    pub fn serial(&self, id: &str) -> Option<&SerialHardwareDeclaration> {
        self.serial.iter().find(|hardware| hardware.id == id)
    }
}

pub(crate) fn validate(board: &BoardDeclaration) -> Result<(), String> {
    validate_identifier(board.id)
        .map_err(|()| format!("board ID `{}` is not a valid Rust identifier", board.id))?;

    let mut ids = BTreeSet::new();
    let mut peripherals = BTreeSet::new();
    let mut pins = BTreeSet::new();
    let mut dma_streams = BTreeSet::new();

    for hardware in board.serial {
        validate_identifier(hardware.id).map_err(|()| {
            format!(
                "board `{}` serial hardware ID `{}` is not a valid Rust identifier",
                board.id, hardware.id
            )
        })?;
        if !ids.insert(hardware.id) {
            return Err(format!(
                "board `{}` declares serial hardware `{}` more than once",
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
                    "board `{}` assigns pin `{:?}` to more than one serial signal",
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
        pins::{GpioPort, PinId},
    };

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

    #[test]
    fn valid_board_supports_serial_lookup() {
        const BOARD: BoardDeclaration = BoardDeclaration::new("selected_board", &[UART4]);

        validate(&BOARD).unwrap();
        assert_eq!(BOARD.serial("uart4"), Some(&UART4));
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
        const INVALID: BoardDeclaration = BoardDeclaration::new("invalid", &[UART4, CONFLICT]);

        let error = validate(&INVALID).unwrap_err();
        assert!(error.contains("Stream2"));
    }
}
