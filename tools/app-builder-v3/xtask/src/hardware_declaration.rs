//! Board-specific hardware declarations composed from reusable definitions.

use crate::hardware_definitions::{
    dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
    pins::{GpioPort, PinId},
    serial::{SerialPeripheral, SerialPort, SerialRoute},
};

/// UART4 routes on the selected board.
pub const UART4: SerialPort = SerialPort::new(SerialPeripheral::Uart4)
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
