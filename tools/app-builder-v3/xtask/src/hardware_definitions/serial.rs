//! STM32F4 serial-port routing declarations.
//!
//! These definitions describe the peripheral and physical TX/RX routes chosen
//! by a board. They do not configure baud rate, protocol framing, GPIO modes,
//! interrupt priorities, or DMA transfers; those are selected when the target
//! application is generated.

use super::{dma_route::DmaRoute, pins::PinId};

/// Identifies an STM32F4 UART or USART peripheral.
///
/// The identifier selects the hardware block only. It does not select pins or
/// establish that a route is valid for the selected MCU package.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SerialPeripheral {
    /// USART1.
    Usart1,
    /// USART2.
    Usart2,
    /// USART3.
    Usart3,
    /// UART4.
    Uart4,
    /// UART5.
    Uart5,
    /// USART6.
    Usart6,
    /// UART7.
    Uart7,
    /// UART8.
    Uart8,
}

/// Describes one serial signal route.
///
/// A route associates either a TX or RX signal with its physical GPIO pin and,
/// when used, its DMA route. The backend validates alternate-function support,
/// peripheral compatibility, and conflicts between declarations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialRoute {
    /// GPIO pin carrying the serial signal.
    pub pin: PinId,

    /// Optional DMA route allocated to the signal.
    ///
    /// `None` declares an interrupt-driven or otherwise non-DMA signal.
    pub dma: Option<DmaRoute>,
}

impl SerialRoute {
    /// Creates a serial route without DMA.
    ///
    /// Use this for a signal whose transfer handling does not require a DMA
    /// stream.
    pub const fn new(pin: PinId) -> Self {
        Self { pin, dma: None }
    }

    /// Creates a DMA-backed serial route.
    ///
    /// The provided DMA route is a declaration; its compatibility with the
    /// serial peripheral is checked by the STM32 backend.
    pub const fn dma(pin: PinId, dma: DmaRoute) -> Self {
        Self {
            pin,
            dma: Some(dma),
        }
    }
}

/// Describes the selected routes for an STM32F4 serial peripheral.
///
/// The App Builder uses this information to instantiate STM32F4 serial
/// hardware tasks and generate the required RTIC interrupt bindings.
///
/// # Examples
///
/// ```
/// use xtask::hardware_definitions::{
///     dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
///     pins::{GpioPort, PinId},
///     serial::{SerialPeripheral, SerialPort, SerialRoute},
/// };
///
/// let uart4 = SerialPort::new(SerialPeripheral::Uart4)
///     .tx(SerialRoute::dma(
///         PinId::new(GpioPort::A, 0),
///         DmaRoute::new(DmaController::Dma1, DmaStream::Stream4, DmaChannel::Channel4),
///     ))
///     .rx(SerialRoute::new(PinId::new(GpioPort::A, 1)));
///
/// assert_eq!(uart4.peripheral, SerialPeripheral::Uart4);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialPort {
    /// UART or USART peripheral used by the port.
    pub peripheral: SerialPeripheral,

    /// Optional receive route.
    ///
    /// `None` means the declaration does not expose a receive signal.
    pub rx: Option<SerialRoute>,

    /// Optional transmit route.
    ///
    /// `None` means the declaration does not expose a transmit signal.
    pub tx: Option<SerialRoute>,
}

impl SerialPort {
    /// Creates a serial-port declaration without assigned signal routes.
    pub const fn new(peripheral: SerialPeripheral) -> Self {
        Self {
            peripheral,
            rx: None,
            tx: None,
        }
    }

    /// Assigns the receive route and returns the updated declaration.
    ///
    /// Calling this method more than once replaces the previously assigned RX
    /// route.
    pub const fn rx(mut self, route: SerialRoute) -> Self {
        self.rx = Some(route);
        self
    }

    /// Assigns the transmit route and returns the updated declaration.
    ///
    /// Calling this method more than once replaces the previously assigned TX
    /// route.
    pub const fn tx(mut self, route: SerialRoute) -> Self {
        self.tx = Some(route);
        self
    }
}
