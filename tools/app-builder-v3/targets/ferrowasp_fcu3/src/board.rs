use crate::{
    board::{BoardDeclaration, MonotonicDeclaration},
    hw_resources::{DmaChannel, Gpio, Mcu, PinId, SerialPortId, SerialProtocol, Target, UartRxDma},
};

const SYSTEM_CLOCK_HZ: u32 = 168_000_000;

pub const BOARD: BoardDeclaration = BoardDeclaration {
    id: "ferrowasp_fcu3",
    target: Target::internal_high_speed(Mcu::Stm32F405, SYSTEM_CLOCK_HZ),
    monotonic: MonotonicDeclaration::SysTick {
        id: "Mono",
        clock_hz: SYSTEM_CLOCK_HZ,
    },
    hardware: &[
        Gpio::output_low("green_led", PinId::new(1, 1)).into_resource(),
        UartRxDma::new(
            "uart2",
            SerialPortId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 5, 4),
        )
        .supports(&[SerialProtocol::Sbus])
        .into_resource(),
        UartRxDma::new(
            "uart4",
            SerialPortId::new(4),
            PinId::new(0, 1),
            DmaChannel::new(0, 2, 4),
        )
        .supports(&[SerialProtocol::Raw])
        .into_resource(),
    ],
};
