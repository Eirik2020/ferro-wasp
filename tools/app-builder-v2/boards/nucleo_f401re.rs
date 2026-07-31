use crate::board::{
    ActiveLevel, BoardDeclaration, ClockDeclaration, ClockSource, HardwareDeclaration, Mcu,
    MonotonicDeclaration, OutputDrive, Pin, PinState,
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
    hardware: &[HardwareDeclaration::GpioOutput {
        id: "led2",
        pin: Pin::PA5,
        drive: OutputDrive::PushPull,
        initial: PinState::Low,
        active: ActiveLevel::High,
    }],
};
