use crate::{
    board::{BoardDeclaration, MonotonicDeclaration},
    hw_resources::{DmaChannel, Mcu, PinId, SerialPortId, SerialProtocol, Target, UartRxDma},
};

const HSE_FREQUENCY_HZ: u32 = 8_000_000;
const SYSTEM_CLOCK_HZ: u32 = 168_000_000;

pub const BOARD: BoardDeclaration = BoardDeclaration {
    id: "foxeer_f405_v2",
    target: Target::external_crystal(Mcu::Stm32F405, HSE_FREQUENCY_HZ, SYSTEM_CLOCK_HZ, true),
    monotonic: MonotonicDeclaration::SysTick {
        id: "Mono",
        clock_hz: SYSTEM_CLOCK_HZ,
    },
    hardware: &[
        // Initial actuator-inhibited subset of the golden board contract.
        UartRxDma::new(
            "uart2",
            SerialPortId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 5, 4),
        )
        .supports(&[SerialProtocol::Sbus])
        .into_resource(),
    ],
};
