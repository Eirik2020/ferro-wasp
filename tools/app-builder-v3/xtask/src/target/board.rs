//! Board-owned hardware declarations consumed by application components.

use crate::hardware_definitions::stm32f4::board_prelude::*;

/// Hardware registry consumed by the example application composition.
pub const BOARD: BoardDeclaration = BoardDeclaration::new(
    "selected_board",
    McuDeclaration::new(
        Mcu::Stm32f405,
        ClockDeclaration::hse(8_000_000, 168_000_000),
    ),
    &[
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
    ],
)
.with_gpio(&[
    GpioHardwareDeclaration::output("led2", PinId::new(GpioPort::A, 5), Level::Low),
    GpioHardwareDeclaration::input("user_button", PinId::new(GpioPort::C, 13))
        .pull_up()
        .interrupt_on(InterruptEdge::Falling),
]);
