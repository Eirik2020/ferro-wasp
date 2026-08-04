use crate::{
    board::{BoardDeclaration, MonotonicDeclaration},
    hw_resources::{
        DmaChannel, Gpio, InterruptEdge, Mcu, PinId, SerialPortId, SerialProtocol, Target,
        UartRxDma,
    },
};

const SYSTEM_CLOCK_HZ: u32 = 84_000_000;

pub const BOARD: BoardDeclaration = BoardDeclaration {
    id: "nucleo_f401re",
    target: Target::internal_high_speed(Mcu::Stm32F401, SYSTEM_CLOCK_HZ),
    monotonic: MonotonicDeclaration::SysTick {
        id: "Mono",
        clock_hz: SYSTEM_CLOCK_HZ,
    },
    hardware: &[
        Gpio::output_low("led3", PinId::new(0, 5)).into_resource(),
        Gpio::input("user_button", PinId::new(2, 13))
            .pull_up()
            .interrupt_on(InterruptEdge::Falling)
            .into_resource(),
        UartRxDma::new(
            "uart2_endpoint",
            SerialPortId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 5, 4),
        )
        .supports(&[SerialProtocol::Sbus, SerialProtocol::Raw])
        .into_resource(),
    ],
};
