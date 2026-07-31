use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mcu {
    Stm32F401RE,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockSource {
    Hsi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockDeclaration {
    pub source: ClockSource,
    pub sysclk_hz: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonotonicDeclaration {
    SysTick { id: &'static str, clock_hz: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pin {
    PA5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputDrive {
    PushPull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum PinState {
    Low,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveLevel {
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareDeclaration {
    GpioOutput {
        id: &'static str,
        pin: Pin,
        drive: OutputDrive,
        initial: PinState,
        active: ActiveLevel,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDeclaration {
    pub id: &'static str,
    pub mcu: Mcu,
    pub clocks: ClockDeclaration,
    pub monotonic: MonotonicDeclaration,
    pub hardware: &'static [HardwareDeclaration],
}

pub struct RenderedBoardInit {
    pub imports: String,
    pub monotonic_declaration: String,
    pub local_struct: String,
    pub local_value: String,
    pub initialization: String,
}

pub fn render_init(board: &BoardDeclaration, include_resources: bool) -> Result<RenderedBoardInit> {
    validate(board)?;

    if !include_resources {
        return Ok(RenderedBoardInit {
            imports: String::new(),
            monotonic_declaration: String::new(),
            local_struct: "struct Local {}".to_owned(),
            local_value: "Local {}".to_owned(),
            initialization: String::new(),
        });
    }

    let MonotonicDeclaration::SysTick {
        id,
        clock_hz: monotonic_clock_hz,
    } = board.monotonic;
    let HardwareDeclaration::GpioOutput {
        id: led_id,
        initial,
        ..
    } = board.hardware[0];
    let initial_state = match initial {
        PinState::Low => "Low",
        PinState::High => "High",
    };

    Ok(RenderedBoardInit {
        imports: "use ferrowasp_stm32f4::rtic::prelude::*;".to_owned(),
        monotonic_declaration: format!("systick_monotonic!({id}, 1_000);"),
        local_struct: format!("struct Local {{\n    {led_id}: PA5<Output<PushPull>>,\n}}"),
        local_value: format!("Local {{ {led_id} }}"),
        initialization: format!(
            "let mut rcc =\n    ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), {}, false);\n{id}::start(cx.core.SYST, {monotonic_clock_hz});\nlet gpioa = cx.device.GPIOA.split(&mut rcc);\nlet mut {led_id} = gpioa.pa5.into_push_pull_output_in_state(PinState::{initial_state});\n{led_id}.set_internal_resistor(Pull::None);\n{led_id}.set_speed(Speed::Low);",
            board.clocks.sysclk_hz,
        ),
    })
}

fn validate(board: &BoardDeclaration) -> Result<()> {
    if board.id != "nucleo_f401re" {
        bail!("unsupported board `{}`", board.id);
    }
    if board.mcu != Mcu::Stm32F401RE {
        bail!("board `{}` must select STM32F401RE", board.id);
    }
    if board.clocks.source != ClockSource::Hsi || board.clocks.sysclk_hz != 84_000_000 {
        bail!("board `{}` requires an 84 MHz HSI clock", board.id);
    }
    let MonotonicDeclaration::SysTick { id, clock_hz } = board.monotonic;
    if id != "Mono" || clock_hz != board.clocks.sysclk_hz {
        bail!("board `{}` requires Mono to use the system clock", board.id);
    }
    if board.hardware.len() != 1 {
        bail!(
            "board `{}` currently supports exactly one GPIO output",
            board.id
        );
    }
    let HardwareDeclaration::GpioOutput {
        id,
        pin,
        drive,
        active,
        ..
    } = board.hardware[0];
    if id != "led2"
        || pin != Pin::PA5
        || drive != OutputDrive::PushPull
        || active != ActiveLevel::High
    {
        bail!(
            "board `{}` must declare LD2 as active-high push-pull PA5",
            board.id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: BoardDeclaration = BoardDeclaration {
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

    #[test]
    fn renders_hsi_systick_and_ld2_initialization() {
        let rendered = render_init(&BOARD, true).unwrap();

        assert!(rendered.monotonic_declaration.contains("Mono"));
        assert!(rendered.local_struct.contains("led2"));
        assert_eq!(rendered.local_value, "Local { led2 }");
        assert!(rendered.initialization.contains("freeze_hsi"));
        assert!(rendered.initialization.contains("PinState::Low"));
    }

    #[test]
    fn omits_hardware_for_an_app_without_tasks() {
        let rendered = render_init(&BOARD, false).unwrap();
        assert!(rendered.imports.is_empty());
        assert_eq!(rendered.local_struct, "struct Local {}");
        assert!(rendered.initialization.is_empty());
    }
}
