use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use super::RenderedBoardInit;
use crate::{
    board::{
        ActiveLevel, BoardDeclaration, ClockSource, HardwareDeclaration, InputPull, InterruptEdge,
        LogicLevel, MonotonicDeclaration, OutputDrive, PhysicalPin,
    },
    resolve::{ResolvedApp, ResolvedResourceUsage},
};

const PA5: PhysicalPin = PhysicalPin::new("PA5");
const PC13: PhysicalPin = PhysicalPin::new("PC13");

pub fn validate(board: &BoardDeclaration) -> Result<()> {
    if board.clocks.source != ClockSource::Hsi || board.clocks.sysclk_hz != 84_000_000 {
        bail!(
            "STM32F401RE backend requires board `{}` to use an 84 MHz HSI clock",
            board.id
        );
    }
    let MonotonicDeclaration::SysTick { id, clock_hz } = board.monotonic;
    if id != "Mono" || clock_hz != board.clocks.sysclk_hz {
        bail!(
            "STM32F401RE backend requires board `{}` Mono to use the system clock",
            board.id
        );
    }
    for hardware in board.hardware {
        validate_hardware(board.id, hardware)?;
    }
    Ok(())
}

fn validate_hardware(board_id: &str, hardware: &HardwareDeclaration) -> Result<()> {
    match hardware {
        HardwareDeclaration::DigitalOutput {
            id,
            pin,
            drive: OutputDrive::PushPull,
            ..
        } => {
            let parsed = parse_pin(*pin)?;
            if parsed != ('A', 5) {
                bail!(
                    "STM32F401RE backend does not support digital output `{}` on {} for board `{board_id}`",
                    id.as_str(),
                    pin.as_str()
                );
            }
            Ok(())
        }
        HardwareDeclaration::DigitalInput {
            id,
            pin,
            pull: InputPull::Up,
            active: ActiveLevel::Low,
            interrupt: Some(InterruptEdge::Falling),
        } => {
            let parsed = parse_pin(*pin)?;
            if parsed != ('C', 13) {
                bail!(
                    "STM32F401RE backend does not support EXTI input `{}` on {} for board `{board_id}`",
                    id.as_str(),
                    pin.as_str()
                );
            }
            Ok(())
        }
        HardwareDeclaration::DigitalInput { id, pin, .. } => bail!(
            "STM32F401RE backend only supports active-low, pull-up, falling-edge EXTI inputs; `{}` on {} uses another configuration",
            id.as_str(),
            pin.as_str()
        ),
    }
}

pub fn render(board: &BoardDeclaration, app: &ResolvedApp<'_>) -> Result<RenderedBoardInit> {
    if app.tasks.is_empty() {
        return Ok(RenderedBoardInit {
            imports: String::new(),
            monotonic_declaration: String::new(),
            shared_struct: "struct Shared {}".to_owned(),
            shared_value: "Shared {}".to_owned(),
            local_struct: "struct Local {}".to_owned(),
            local_value: "Local {}".to_owned(),
            initialization: String::new(),
        });
    }

    let MonotonicDeclaration::SysTick {
        id,
        clock_hz: monotonic_clock_hz,
    } = board.monotonic;
    let resources_by_id = app
        .resources
        .iter()
        .map(|resource| (resource.hardware.id(), resource))
        .collect::<BTreeMap<_, _>>();
    let mut local_fields = Vec::new();
    let mut local_values = Vec::new();
    let mut shared_fields = Vec::new();
    let mut shared_values = Vec::new();
    for resource in &app.resources {
        let field = render_resource_field(resource.hardware)?;
        match resource.usage {
            ResolvedResourceUsage::Local { .. } => {
                local_fields.push(field);
                local_values.push(resource.hardware.id().to_owned());
            }
            ResolvedResourceUsage::Shared { .. } => {
                shared_fields.push(field);
                shared_values.push(resource.hardware.id().to_owned());
            }
        }
    }
    for resource in &app.software_local_resources {
        debug_assert_eq!(resource.task_ids.len(), 1);
        let declaration = resource.declaration;
        local_fields.push(format!(
            "{}: {},",
            declaration.id(),
            declaration.rust_type()
        ));
        local_values.push(format!(
            "{}: {}",
            declaration.id(),
            declaration.initial_value()
        ));
    }
    for resource in &app.software_shared_resources {
        debug_assert!(!resource.task_ids.is_empty());
        let declaration = resource.declaration;
        shared_fields.push(format!(
            "{}: {},",
            declaration.id(),
            declaration.rust_type()
        ));
        shared_values.push(format!(
            "{}: {}",
            declaration.id(),
            declaration.initial_value()
        ));
    }

    let has_resources = !app.resource_initialization_order.is_empty();
    let rcc_binding = if has_resources { "mut rcc" } else { "_rcc" };
    let mut initialization = format!(
        "let {rcc_binding} =\n    ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), {}, false);\n{id}::start(cx.core.SYST, {monotonic_clock_hz});",
        board.clocks.sysclk_hz,
    );
    let used_ports = app
        .resources
        .iter()
        .map(|resource| parse_pin(resource.hardware.pin()).map(|(port, _)| port))
        .collect::<Result<BTreeSet<_>>>()?;
    if used_ports.contains(&'A') {
        initialization.push_str("\nlet gpioa = cx.device.GPIOA.split(&mut rcc);");
    }
    if used_ports.contains(&'C') {
        initialization.push_str("\nlet gpioc = cx.device.GPIOC.split(&mut rcc);");
    }
    let has_exti_input = app.resources.iter().any(|resource| {
        matches!(
            resource.hardware,
            HardwareDeclaration::DigitalInput {
                interrupt: Some(_),
                ..
            }
        )
    });
    if has_exti_input {
        initialization.push_str(
            "\nlet mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);\nlet mut exti = cx.device.EXTI;",
        );
    }
    for resource_id in &app.resource_initialization_order {
        let resource = resources_by_id.get(resource_id).ok_or_else(|| {
            anyhow::anyhow!(
                "resolved initialization order references unknown resource `{resource_id}`"
            )
        })?;
        initialization.push('\n');
        initialization.push_str(&render_resource_initialization(resource.hardware)?);
    }

    Ok(RenderedBoardInit {
        imports: "use ferrowasp_io_core::digital::prelude::{OutputPin, StatefulOutputPin};\nuse ferrowasp_stm32f4::rtic::prelude::*;".to_owned(),
        monotonic_declaration: format!("systick_monotonic!({id}, 1_000);"),
        shared_struct: render_resource_struct("Shared", &shared_fields),
        shared_value: render_resource_value("Shared", &shared_values),
        local_struct: render_resource_struct("Local", &local_fields),
        local_value: render_resource_value("Local", &local_values),
        initialization,
    })
}

fn render_resource_field(hardware: &HardwareDeclaration) -> Result<String> {
    match hardware {
        HardwareDeclaration::DigitalOutput {
            id,
            pin,
            drive: OutputDrive::PushPull,
            ..
        } if *pin == PA5 => Ok(format!("{}: PA5<Output<PushPull>>,", id.as_str())),
        HardwareDeclaration::DigitalInput { id, pin, .. } if *pin == PC13 => {
            Ok(format!("{}: PC13<Input>,", id.as_str()))
        }
        _ => unsupported_resolved_hardware(hardware),
    }
}

fn render_resource_initialization(hardware: &HardwareDeclaration) -> Result<String> {
    match hardware {
        HardwareDeclaration::DigitalOutput {
            id,
            pin,
            drive: OutputDrive::PushPull,
            initial,
            ..
        } if *pin == PA5 => {
            let initial_state = match initial {
                LogicLevel::Low => "Low",
                LogicLevel::High => "High",
            };
            let id = id.as_str();
            Ok(format!(
                "let mut {id} = gpioa.pa5.into_push_pull_output_in_state(PinState::{initial_state});\n{id}.set_internal_resistor(Pull::None);\n{id}.set_speed(Speed::Low);"
            ))
        }
        HardwareDeclaration::DigitalInput {
            id,
            pin,
            pull: InputPull::Up,
            interrupt: Some(InterruptEdge::Falling),
            ..
        } if *pin == PC13 => {
            let id = id.as_str();
            Ok(format!(
                "let {id} = Input::new(gpioc.pc13, Pull::Up);\nlet {id} = ferrowasp_stm32f4::exti::init_input({id}, &mut syscfg, &mut exti, Edge::Falling);"
            ))
        }
        _ => unsupported_resolved_hardware(hardware),
    }
}

fn unsupported_resolved_hardware<T>(hardware: &HardwareDeclaration) -> Result<T> {
    bail!(
        "STM32F401RE backend cannot render resolved resource `{}` on {}",
        hardware.id(),
        hardware.pin().as_str()
    )
}

fn parse_pin(pin: PhysicalPin) -> Result<(char, u8)> {
    let bytes = pin.as_str().as_bytes();
    if bytes.len() < 3
        || bytes[0] != b'P'
        || !bytes[1].is_ascii_uppercase()
        || !bytes[2..].iter().all(u8::is_ascii_digit)
    {
        bail!(
            "STM32F401RE backend physical pin `{}` must use a name such as `PA5`",
            pin.as_str()
        );
    }
    let number = pin.as_str()[2..].parse::<u8>().map_err(|_| {
        anyhow::anyhow!(
            "STM32F401RE backend physical pin `{}` has an invalid pin number",
            pin.as_str()
        )
    })?;
    Ok((char::from(bytes[1]), number))
}

fn render_resource_struct(name: &str, fields: &[String]) -> String {
    if fields.is_empty() {
        format!("struct {name} {{}}")
    } else {
        format!("struct {name} {{\n    {}\n}}", fields.join("\n    "))
    }
}

fn render_resource_value(name: &str, resources: &[String]) -> String {
    if resources.is_empty() {
        format!("{name} {{}}")
    } else {
        format!("{name} {{ {} }}", resources.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{
            AppDeclaration, InitDeclaration, SoftwareResourceDeclaration,
            SoftwareResourcesDeclaration,
        },
        board::ResourceId,
        resolve::resolve,
        task::{TaskDeclaration, TaskTrigger},
    };

    const LED2: HardwareDeclaration =
        HardwareDeclaration::digital_output(ResourceId::new("led2"), PA5);
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "nucleo_f401re",
        mcu: crate::board::Mcu::Stm32F401RE,
        clocks: crate::board::ClockDeclaration {
            source: ClockSource::Hsi,
            sysclk_hz: 84_000_000,
        },
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[LED2],
    };
    const BLINK: TaskDeclaration = TaskDeclaration {
        id: "blink_led",
        priority: 1,
        trigger: TaskTrigger::Spawned,
        args: &[],
        local_resources: &["led2"],
        shared_resources: &[],
    };

    #[test]
    fn maps_a5_to_stm32f4_type_and_initialization() {
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        validate(&BOARD).unwrap();
        let resolved = resolve(&BOARD, &app).unwrap();
        let rendered = render(&BOARD, &resolved).unwrap();

        assert!(rendered.local_struct.contains("PA5<Output<PushPull>>"));
        assert!(rendered.initialization.contains("gpioa.pa5"));
        assert!(rendered.initialization.contains("PinState::Low"));
    }

    #[test]
    fn maps_pc13_to_exti_input_and_renders_shared_state() {
        const SOFTWARE_SHARED: &[SoftwareResourceDeclaration] =
            &[SoftwareResourceDeclaration::bool("blink_enabled", true)];
        const BUTTON: HardwareDeclaration =
            HardwareDeclaration::exti_input(ResourceId::new("user_button"), PC13);
        const BUTTON_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[BUTTON],
            ..BOARD
        };
        const BUTTON_TASK: TaskDeclaration = TaskDeclaration {
            id: "button_exti",
            priority: 2,
            trigger: TaskTrigger::Interrupt { binds: "EXTI15_10" },
            args: &[],
            local_resources: &["user_button"],
            shared_resources: &["blink_enabled"],
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BUTTON_TASK],
            software_resources: SoftwareResourcesDeclaration {
                shared: SOFTWARE_SHARED,
                local: &[],
            },
        };

        validate(&BUTTON_BOARD).unwrap();
        let resolved = resolve(&BUTTON_BOARD, &app).unwrap();
        let rendered = render(&BUTTON_BOARD, &resolved).unwrap();

        assert!(rendered.local_struct.contains("PC13<Input>"));
        assert!(rendered.shared_struct.contains("blink_enabled: bool"));
        assert!(rendered.shared_value.contains("blink_enabled: true"));
        assert!(rendered.initialization.contains("GPIOC.split"));
        assert!(rendered.initialization.contains("exti::init_input"));
        assert!(rendered.initialization.contains("Edge::Falling"));
        assert!(!rendered.initialization.contains("GPIOA.split"));
    }

    #[test]
    fn rejects_unsupported_stm32f401_pin() {
        const UNSUPPORTED: BoardDeclaration = BoardDeclaration {
            hardware: &[HardwareDeclaration::digital_output(
                ResourceId::new("unsupported"),
                PhysicalPin::new("PB0"),
            )],
            ..BOARD
        };

        let error = validate(&UNSUPPORTED).unwrap_err();
        assert!(error.to_string().contains("does not support"));
        assert!(error.to_string().contains("PB0"));
    }

    #[test]
    fn unused_board_hardware_is_not_initialized() {
        const UNUSED: HardwareDeclaration =
            HardwareDeclaration::digital_output(ResourceId::new("unused"), PA5)
                .with_initial(LogicLevel::High);
        const BOARD_WITH_UNUSED: BoardDeclaration = BoardDeclaration {
            hardware: &[UNUSED, LED2],
            ..BOARD
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        validate(&BOARD_WITH_UNUSED).unwrap();
        let resolved = resolve(&BOARD_WITH_UNUSED, &app).unwrap();
        let rendered = render(&BOARD_WITH_UNUSED, &resolved).unwrap();

        assert!(!rendered.local_struct.contains("unused"));
        assert!(!rendered.initialization.contains("unused"));
        assert!(rendered.initialization.contains("led2"));
    }

    #[test]
    fn empty_application_renders_without_backend_imports() {
        let resolved = resolve(&BOARD, &AppDeclaration::EMPTY).unwrap();
        let rendered = render(&BOARD, &resolved).unwrap();

        assert!(rendered.imports.is_empty());
        assert_eq!(rendered.shared_struct, "struct Shared {}");
        assert_eq!(rendered.local_struct, "struct Local {}");
        assert!(rendered.initialization.is_empty());
    }

    #[test]
    fn rejects_malformed_stm32_pin_names() {
        const MALFORMED: BoardDeclaration = BoardDeclaration {
            hardware: &[HardwareDeclaration::digital_output(
                ResourceId::new("malformed"),
                PhysicalPin::new("pa5"),
            )],
            ..BOARD
        };

        let error = validate(&MALFORMED).unwrap_err();
        assert!(error.to_string().contains("must use a name such as `PA5`"));
    }
}
