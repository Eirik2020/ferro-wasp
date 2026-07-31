use crate::board::{
    BoardDeclaration, ClockDeclaration, ClockSource, HardwareDeclaration, Mcu,
    MonotonicDeclaration, PhysicalPin, ResourceId,
};

pub const BOARD: BoardDeclaration = BoardDeclaration {
    id: "nucleo_f401re",
    mcu: Mcu::Stm32F401RE,
    clocks: ClockDeclaration {
        source: ClockSource::Hsi,
        sysclk_hz: 84_000_000,
    },
    monotonic: MonotonicDeclaration::SysTick {
        id: "Mono",
        clock_hz: 84_000_000,
    },
    hardware: &[
        HardwareDeclaration::digital_output(ResourceId::new("led3"), PhysicalPin::new("PA5")),
        HardwareDeclaration::exti_input(ResourceId::new("user_button"), PhysicalPin::new("PC13")),
    ],
};
