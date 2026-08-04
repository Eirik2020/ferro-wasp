use crate::{
    board::{BoardDeclaration, MonotonicDeclaration},
    hw_resources::{DmaChannel, Gpio, InterruptEdge, Mcu, PinId, Target, UartId, UartRxDma},
};

pub const BOARD: BoardDeclaration = BoardDeclaration {
    id: "nucleo_f401re",
    target: Target::internal_high_speed(Mcu::Stm32F401, 84_000_000),
    monotonic: MonotonicDeclaration::SysTick {
        id: "Mono",
        clock_hz: 84_000_000,
    },
    hardware: &[
        Gpio::output_low("led3", PinId::new(0, 5)).into_resource(),
        Gpio::input("user_button", PinId::new(2, 13))
            .pull_up()
            .interrupt_on(InterruptEdge::Falling)
            .into_resource(),
        UartRxDma::sbus(
            "sbus_rx",
            UartId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 5, 4),
        )
        .into_resource(),
    ],
};
