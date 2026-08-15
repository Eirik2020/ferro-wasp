//! Selected-target reconciliation with the handwritten Foxeer application.

use std::{collections::BTreeSet, fmt::Write as _};

use anyhow::{Result, bail};
use fugit::MillisDurationU32;

use crate::rtic::task::ConfigValue;
use crate::rtic::{
    composition::TaskSafetyClass,
    resolve::{ResolvedApp, ResolvedTask, ResolvedTaskTrigger},
    safety_channel::SafetyMessage,
    state::TaskStateRecipe,
    timing::{InitDelayDeclaration, MonotonicDeclaration},
};
use crate::target::{
    app_composition::{
        CONTROL_STATE_FIELDS, CONTROL_TIMER, GOLDEN_SERVICES, PHYSICAL_ACTUATOR,
        SAFETY_STATE_FIELDS, SERIAL1_ENDPOINT, SERIAL2_ENDPOINT, SPI1_ENDPOINT,
    },
    board, platform_config,
};

/// Repository revision containing the reviewed handwritten Foxeer snapshot.
pub const GOLDEN_SOURCE_REVISION: &str = "e7f4590dee98b4ebd70114932153e066a70e7972";
/// Git tree object for the complete handwritten Foxeer application at the pin.
pub const GOLDEN_APP_TREE: &str = "c9eb1d19412cb9cb34d3a74005d4060c0fd2fcdd";
/// Git tree object for the handwritten application's Rust source at the pin.
pub const GOLDEN_SOURCE_TREE: &str = "b5467ee26b70d939234ff2f5a56ef6c71c5a8b10";

#[derive(Clone, Copy)]
enum ExpectedTrigger {
    Software,
    Interrupt(&'static str),
}

const EXPECTED_TASKS: &[(&str, u8, TaskSafetyClass, ExpectedTrigger)] = &[
    (
        "actuator_fault_reporter",
        16,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "actuator_output",
        15,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "adc_observation_dma",
        1,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("DMA2_STREAM4"),
    ),
    (
        "adc_observation_poll",
        1,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "control_loop",
        14,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Interrupt("TIM4"),
    ),
    (
        "dshot_motor1_dma_complete",
        16,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Interrupt("DMA2_STREAM1"),
    ),
    (
        "dshot_motor2_dma_complete",
        16,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Interrupt("DMA2_STREAM7"),
    ),
    (
        "dshot_motor3_dma_complete",
        16,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Interrupt("DMA2_STREAM2"),
    ),
    (
        "dshot_motor4_dma_complete",
        16,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Interrupt("DMA2_STREAM6"),
    ),
    (
        "dshot_service",
        13,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "esc_manager",
        4,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "esc_uart_rx_dma",
        5,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("DMA2_STREAM5"),
    ),
    (
        "esc_uart_rx_idle",
        5,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("USART1"),
    ),
    (
        "golden_flash",
        1,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "heartbeat",
        1,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "imu_control_bridge",
        11,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "io_watchdog",
        9,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("TIM6_DAC"),
    ),
    (
        "msp_osd",
        3,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "safety_master",
        16,
        TaskSafetyClass::SafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "serial1_rx_bridge",
        12,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "serial1_rx_dma_irq",
        13,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("DMA1_STREAM5"),
    ),
    (
        "serial1_rx_idle_irq",
        13,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("USART2"),
    ),
    (
        "serial1_tx_dma_irq",
        13,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("DMA1_STREAM6"),
    ),
    (
        "serial1_tx_worker",
        4,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "serial2_rx_bridge",
        4,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "serial2_rx_dma_irq",
        6,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("DMA1_STREAM2"),
    ),
    (
        "serial2_rx_idle_irq",
        6,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("UART4"),
    ),
    (
        "serial2_tx_dma_irq",
        6,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("DMA1_STREAM4"),
    ),
    (
        "serial2_tx_worker",
        4,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "spi1_data_ready",
        14,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("EXTI4"),
    ),
    (
        "spi1_owner_service",
        13,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "spi1_parser",
        11,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "spi1_poll",
        12,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "spi1_rx_dma_irq",
        13,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("DMA2_STREAM0"),
    ),
    (
        "spi1_timeout",
        13,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Software,
    ),
    (
        "usb_cdc",
        5,
        TaskSafetyClass::NonSafetyCritical,
        ExpectedTrigger::Interrupt("OTG_FS"),
    ),
];

struct Coverage {
    area: &'static str,
    handwritten: &'static str,
    generated: &'static str,
    disposition: &'static str,
}

const COVERAGE: &[Coverage] = &[
    Coverage {
        area: "Physical resources",
        handwritten: "STM32F405 at 168 MHz; reviewed USART1/2, UART4, SPI1/2, ADC1, OTG_FS, TIM1/2/4/5/6/8, GPIO and DMA routes.",
        generated: "The typed board declaration locks the same routes; the reusable USART2 endpoint additionally reserves unused TX DMA1 Stream6.",
        disposition: "Reviewed intentional deviation",
    },
    Coverage {
        area: "Initialization",
        handwritten: "Hand-wired peripheral construction, split signals/queues, SysTick 1 kHz monotonic, TIM2 microsecond timebase and TIM5 delay.",
        generated: "Typed constructors and split handles initialize the same services; TIM2 is the 1 MHz RTIC monotonic and TIM5 remains the 1 MHz init delay.",
        disposition: "Reviewed intentional decomposition/timer deviation",
    },
    Coverage {
        area: "Tasks and priorities",
        handwritten: "Protocol-specific tasks include RC input p10, safety p16, control p14, actuator p15, DShot p13/p16 and mandatory services at their recorded priorities.",
        generated: "The exact generated task inventory and priorities are validation-locked; SBUS uses endpoint IRQ p13, bridge p12 and safety-owned parsing p16.",
        disposition: "Reviewed intentional decomposition/priority deviation",
    },
    Coverage {
        area: "Transports",
        handwritten: "USART2 SBUS, UART4 MSP V1 DisplayPort, SPI1 IMU, SPI2 NOR, USART1 ESC telemetry and OTG_FS CDC are directly wired.",
        generated: "Typed endpoint/component routes preserve those protocols, ownership and bounded discontinuity handling.",
        disposition: "Equivalent semantic routes; reviewed endpoint decomposition",
    },
    Coverage {
        area: "Capacities",
        handwritten: "Shared backends provide four RX buffers/depth four, UART4 TX depth 16, SPI buffers/queues, motor/ESC queues and flash queues.",
        generated: "The same backend capacities are locked, plus seven explicit SPSC safety channels with validated capacities and sole owners.",
        disposition: "Equivalent shared capacities; intentional explicit safety channels",
    },
    Coverage {
        area: "Safety state",
        handwritten: "At the pinned default feature set all board verification flags are true, so physical actuator output can arm after runtime guards and ESC qualification.",
        generated: "Safety state starts output-inhibited, the physical component gate is false, and control is forced disarmed.",
        disposition: "Intentional pre-cutover inhibition",
    },
    Coverage {
        area: "Fault behavior",
        handwritten: "Protocol faults invalidate inputs; stale/invalid control fails closed; actuator and ESC faults revoke authority and stop output.",
        generated: "The same reusable fail-closed policies are retained, with a bounded central actuator-fault channel and explicit transport-discontinuity paths.",
        disposition: "Equivalent outcome; reviewed event-routing deviation",
    },
    Coverage {
        area: "Timing",
        handwritten: "800 Hz TIM4 scheduler, 400 Hz control, 2 ms DShot/ESC, 10 ms OSD, 100 ms ADC, 2 s heartbeat and 8 kHz watchdog.",
        generated: "Those cadences are validation-locked; safety/actuator add bounded 10 ms policy loops and the IMU bridge polls at 1 ms.",
        disposition: "Equivalent required cadences; reviewed bounded polling",
    },
    Coverage {
        area: "Feature gates",
        handwritten: "Compile-time profiles cover flight, props-off bench, fault injection, RTT diagnostics and optional MSPv2 configuration.",
        generated: "One fixed mandatory-service composition has no generated feature matrix and both output gates remain false.",
        disposition: "Intentional single inhibited profile",
    },
    Coverage {
        area: "Logging and configuration",
        handwritten: "Sequenced tuning, append-only flight records, copy-on-write config, USB CLI, scratch test and optional MSPv2 RPC.",
        generated: "The same record/config primitives and bounded queues are used; scratch test and MSPv2 RPC are omitted, and forced disarm prevents flight-scoped page assembly.",
        disposition: "Reviewed intentional diagnostic omissions",
    },
];

struct Difference {
    area: &'static str,
    golden: &'static str,
    generated: &'static str,
    rationale: &'static str,
}

const DIFFERENCES: &[Difference] = &[
    Difference {
        area: "Actuator authority",
        golden: "Safety-gated DShot on TIM1/TIM8, four motor GPIOs and DMA completion paths; the pinned default feature set enables output after runtime guards and ESC qualification.",
        generated: "The exact four-lane DShot/USART1 path is generated through the shared backend, but both safety policy and the physical component independently inhibit output.",
        rationale: "The architecture is compile-checked while software, electrical, props-off, and target evidence gates remain closed.",
    },
    Difference {
        area: "Arming visibility in control",
        golden: "Control observes the live safety arming state and can submit gated actuator requests.",
        generated: "The same control step is called with `armed = false`; the request path is modeled but cannot become active.",
        rationale: "This checkpoint proves the control/safety spine without enabling output.",
    },
    Difference {
        area: "RC and safety task shape",
        golden: "Priority-10 `rc_input` parses SBUS; priority-16 `safety_master` evaluates authoritative state from signals.",
        generated: "Priority-16 `safety_master` exclusively owns the SBUS reader, parser recovery, RC link state, arming state, and permit.",
        rationale: "The current generated spine keeps parser faults and policy state in one exclusive authority boundary.",
    },
    Difference {
        area: "Safety scheduling",
        golden: "Safety is event-driven through typed `SafetyEvent` spawns from relevant producers.",
        generated: "The safety master combines direct SBUS consumption with a bounded 10 ms timeout/policy loop and publishes exclusive fresh authority observations to the actuator.",
        rationale: "Polling is explicit and bounded at this checkpoint; event fan-in can be restored without moving safety authority.",
    },
    Difference {
        area: "Health transport",
        golden: "Safety reads live health prerequisites through golden signal/state readers.",
        generated: "Control publishes observation-only `PreArmHealthReport` values over a bounded SPSC channel with a 20 ms freshness limit.",
        rationale: "The observation channel cannot grant authority and makes producer/consumer ownership explicit.",
    },
    Difference {
        area: "Transport composition",
        golden: "USART2 SBUS, UART4 MSP and SPI1 IMU services are hand-wired in the app.",
        generated: "Typed endpoint graphs are physical at composition time; boot platform configuration assigns portable services to connector handles.",
        rationale: "This is the builder's validated service-routing boundary; physical routes remain board facts.",
    },
    Difference {
        area: "Serial task decomposition",
        golden: "Protocol-specific interrupt and worker tasks own the queues directly.",
        generated: "Reusable DMA endpoint IRQ, bridge and TX-worker tasks feed portable bounded channels.",
        rationale: "The endpoint mechanism is shared across services while preserving bounded DMA and discontinuity handling.",
    },
    Difference {
        area: "IMU decomposition",
        golden: "SPI1 data-ready, polling, owner, DMA, timeout and parser logic is wired directly in the app.",
        generated: "The same physical SPI1/EXTI4/DMA2_STREAM0 flow is expanded from one typed IMU endpoint component.",
        rationale: "Generated resource ownership is explicit; the verified sensor-to-body transform is still applied exactly once.",
    },
    Difference {
        area: "OSD scope",
        golden: "Priority-3 DisplayPort telemetry includes receive handling, menus and tuning behavior.",
        generated: "Priority-3 DisplayPort telemetry handles bounded UART4 receive/replies, stale-safe telemetry, overlay updates, and the disarmed tuning menu through reusable endpoint handles.",
        rationale: "The task decomposition differs, but UART ownership, behavior, priority, and the no-safety-authority boundary are preserved.",
    },
    Difference {
        area: "Diagnostics and persistence",
        golden: "Includes blackbox/config flash, detailed task identities and application diagnostics.",
        generated: "Owns SPI2 NOR through one priority-1 task, recovers append-only logs/config, verifies copy-on-write saves, and exposes bounded USB CDC text diagnostics and whitelisted commands.",
        rationale: "The generated candidate omits the optional destructive scratch test and MSPv2 configurator profile; all mutations remain disarmed-only.",
    },
    Difference {
        area: "Runtime tuning and control logging",
        golden: "Control applies sequenced runtime tuning profiles and publishes blackbox records.",
        generated: "Control applies sequenced stored/menu tuning and publishes rate records into the bounded SPI2 queue, with explicit drop accounting.",
        rationale: "The generated build remains permanently disarmed, so flight-scoped page assembly cannot start until the independent output gates are separately approved.",
    },
    Difference {
        area: "Remaining services",
        golden: "Includes USB CDC, ADC sensing, heartbeat, IO watchdog, ESC manager and USART1 ESC telemetry.",
        generated: "Includes the same mandatory service identities and priorities, explicit ADC freshness suppression, OTG_FS CDC, TIM6 watchdog, heartbeat, ESC manager and exact USART1 telemetry.",
        rationale: "The generated services are observation, diagnostics, persistence, or recovery paths and receive no actuator authority.",
    },
    Difference {
        area: "RTIC dispatchers",
        golden: "Dispatcher selection follows the handwritten task graph.",
        generated: "Dispatchers are deterministically allocated after resolved interrupt ownership.",
        rationale: "Dispatcher identity is an implementation detail, but collision-free allocation remains validated.",
    },
    Difference {
        area: "Feature profiles",
        golden: "Compile-time features select configurator, telemetry, diagnostics and guarded physical-output behavior.",
        generated: "One mandatory-service composition is rendered with text CDC configuration and physical output inhibited; optional MSPv2/diagnostic feature combinations are not generated.",
        rationale: "No feature combination may silently expand actuator authority; later profiles require explicit compositions and validation.",
    },
];

/// Rejects drift in the selected output-inhibited safety-authority spine.
pub fn validate(resolved: &ResolvedApp) -> Result<()> {
    validate_complete_graph(resolved)?;

    let safety = task(resolved, "safety_master")?;
    require_task(safety, 16, TaskSafetyClass::SafetyCritical, true)?;
    require_local(safety, "foxeer_safety_state")?;
    require_local(safety, "prearm_health_consumer")?;
    require_local(safety, "rc_sbus_reader")?;
    require_local(safety, "rc_sbus_rx_discontinuities")?;
    require_local(safety, "actuator_guard_producer")?;
    require_local(safety, "actuator_completion_consumer")?;
    require_local(safety, "actuator_fault_consumer")?;
    if !resolved.task_state.iter().any(|state| {
        state.owner_task == "safety_master"
            && state.id == "foxeer_safety_state"
            && state.recipe == TaskStateRecipe::OutputInhibitedFoxeerSafetyMasterV1
    }) {
        bail!("selected safety policy must retain its independent output-inhibited constructor");
    }

    let control = task(resolved, "control_loop")?;
    require_task(control, 14, TaskSafetyClass::SafetyCritical, false)?;
    require_local(control, "motor_cmd_producer")?;
    require_local(control, "prearm_health_producer")?;

    let actuator = task(resolved, "actuator_output")?;
    require_task(actuator, 15, TaskSafetyClass::SafetyCritical, true)?;
    require_local(actuator, "motor_cmd_consumer")?;
    require_local(actuator, "actuator_guard_consumer")?;
    require_local(actuator, "actuator_completion_producer")?;
    if actuator.shared.len() != 1 || actuator.shared[0].target != "dshot_motors" {
        bail!("the priority-15 actuator must be the sole command adapter for `dshot_motors`");
    }
    require_disabled_config(actuator)?;

    let motor_channel = resolved
        .safety_channels
        .iter()
        .find(|channel| channel.id == "control_to_actuator")
        .ok_or_else(|| anyhow::anyhow!("missing `control_to_actuator` safety channel"))?;
    if motor_channel.message != SafetyMessage::MotorCommand
        || motor_channel.usable_capacity != ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY
        || motor_channel.producer.owner_task != "control_loop"
        || motor_channel.consumer.owner_task != "actuator_output"
    {
        bail!("control-to-actuator safety-channel authority or capacity drifted");
    }

    for (id, producer, consumer) in [
        (
            "safety_to_actuator_guard",
            "safety_master",
            "actuator_output",
        ),
        ("actuator_to_safety", "actuator_output", "safety_master"),
        (
            "actuator_fault_to_safety",
            "actuator_fault_reporter",
            "safety_master",
        ),
    ] {
        let channel = resolved
            .safety_channels
            .iter()
            .find(|channel| channel.id == id)
            .ok_or_else(|| anyhow::anyhow!("missing `{id}` safety channel"))?;
        if channel.usable_capacity != ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY
            || channel.producer.owner_task != producer
            || channel.consumer.owner_task != consumer
        {
            bail!("physical-actuator safety channel `{id}` drifted");
        }
    }

    let [physical] = resolved.dshot_actuators.as_slice() else {
        bail!("selected candidate must declare exactly one physical DShot actuator")
    };
    if physical.declaration.output_enabled {
        bail!("selected physical actuator must remain disabled by default");
    }
    let expected_tasks = [
        ("dshot_motor1_dma_complete", 16, "DMA2_STREAM1"),
        ("dshot_motor2_dma_complete", 16, "DMA2_STREAM7"),
        ("dshot_motor3_dma_complete", 16, "DMA2_STREAM2"),
        ("dshot_motor4_dma_complete", 16, "DMA2_STREAM6"),
        ("esc_uart_rx_dma", 5, "DMA2_STREAM5"),
        ("esc_uart_rx_idle", 5, "USART1"),
    ];
    for (id, priority, interrupt) in expected_tasks {
        let task = task(resolved, id)?;
        require_task(task, priority, task.safety_class, false)?;
        if !matches!(&task.trigger, ResolvedTaskTrigger::Interrupt(actual) if actual == interrupt) {
            bail!("physical task `{id}` interrupt drifted from `{interrupt}`");
        }
    }
    let service = task(resolved, "dshot_service")?;
    require_task(service, 13, TaskSafetyClass::SafetyCritical, true)?;
    require_disabled_config(service)?;
    let manager = task(resolved, "esc_manager")?;
    require_task(manager, 4, TaskSafetyClass::SafetyCritical, true)?;
    require_disabled_config(manager)?;

    let [services] = resolved.golden_services.as_slice() else {
        bail!("selected candidate must declare exactly one golden service suite")
    };
    if services.adc.id != "adc1_observation"
        || services.flash.id != "spi2_nor"
        || services.usb.id != "usb_cdc"
        || services.watchdog.id != "tim6"
    {
        bail!("golden ADC/SPI2/USB/TIM6 hardware selection drifted");
    }

    let adc_dma = task(resolved, "adc_observation_dma")?;
    require_task(adc_dma, 1, TaskSafetyClass::NonSafetyCritical, false)?;
    if !matches!(&adc_dma.trigger, ResolvedTaskTrigger::Interrupt(vector) if vector == "DMA2_STREAM4")
    {
        bail!("ADC observation interrupt drifted from DMA2_STREAM4");
    }
    let adc_poll = task(resolved, "adc_observation_poll")?;
    require_task(adc_poll, 1, TaskSafetyClass::NonSafetyCritical, true)?;

    let osd = task(resolved, "msp_osd")?;
    require_task(osd, 3, TaskSafetyClass::NonSafetyCritical, true)?;
    require_local(osd, "msp_v1_osd_reader")?;
    require_local(osd, "msp_v1_osd_writer")?;

    let flash = task(resolved, "golden_flash")?;
    require_task(flash, 1, TaskSafetyClass::NonSafetyCritical, true)?;
    require_local(flash, "flash_device")?;
    require_local(control, "flash_record_producer")?;

    let usb = task(resolved, "usb_cdc")?;
    require_task(usb, 5, TaskSafetyClass::NonSafetyCritical, false)?;
    if !matches!(&usb.trigger, ResolvedTaskTrigger::Interrupt(vector) if vector == "OTG_FS") {
        bail!("USB diagnostic task interrupt drifted from OTG_FS");
    }

    let heartbeat = task(resolved, "heartbeat")?;
    require_task(heartbeat, 1, TaskSafetyClass::NonSafetyCritical, true)?;

    let watchdog = task(resolved, "io_watchdog")?;
    require_task(watchdog, 9, TaskSafetyClass::NonSafetyCritical, false)?;
    require_local(watchdog, "io_watchdog")?;
    if !matches!(&watchdog.trigger, ResolvedTaskTrigger::Interrupt(vector) if vector == "TIM6_DAC")
    {
        bail!("I/O watchdog interrupt drifted from TIM6_DAC");
    }
    Ok(())
}

fn validate_complete_graph(resolved: &ResolvedApp) -> Result<()> {
    if resolved.board != &board::BOARD || resolved.board.id != "foxeer_f405_v2" {
        bail!("selected board drifted from the pinned Foxeer F405 V2 declaration");
    }
    if resolved.platform != platform_config::platform_config() {
        bail!("boot-time protocol routing drifted from the pinned Foxeer service map");
    }
    if resolved.monotonic != MonotonicDeclaration::timer(board::TIM2.id, 1_000_000)
        || resolved.init_delay != Some(InitDelayDeclaration::timer(board::TIM5.id, 1_000_000))
    {
        bail!("generated monotonic or initialization-delay timing drifted");
    }

    if resolved.serial_endpoints.len() != 2
        || resolved
            .serial_endpoints
            .iter()
            .find(|endpoint| endpoint.declaration.id == SERIAL1_ENDPOINT.id)
            .is_none_or(|endpoint| endpoint.declaration != SERIAL1_ENDPOINT)
        || resolved
            .serial_endpoints
            .iter()
            .find(|endpoint| endpoint.declaration.id == SERIAL2_ENDPOINT.id)
            .is_none_or(|endpoint| endpoint.declaration != SERIAL2_ENDPOINT)
    {
        bail!("generated serial endpoint declarations drifted");
    }
    let [imu] = resolved.imu_endpoints.as_slice() else {
        bail!("selected candidate must declare exactly one SPI1 IMU endpoint")
    };
    if imu.declaration != SPI1_ENDPOINT {
        bail!("generated SPI1 IMU endpoint declaration drifted");
    }
    let [control] = resolved.periodic_controls.as_slice() else {
        bail!("selected candidate must declare exactly one periodic control component")
    };
    if control.declaration != CONTROL_TIMER {
        bail!("generated TIM4 control declaration drifted");
    }
    let [physical] = resolved.dshot_actuators.as_slice() else {
        bail!("selected candidate must declare exactly one physical actuator component")
    };
    if physical.declaration != PHYSICAL_ACTUATOR || physical.hardware != &board::FOXEER_DSHOT {
        bail!("generated DShot/ESC physical declaration drifted");
    }
    let [services] = resolved.golden_services.as_slice() else {
        bail!("selected candidate must declare exactly one golden service component")
    };
    if services.declaration != GOLDEN_SERVICES
        || services.adc != &board::FOXEER_ADC1
        || services.flash != &board::FOXEER_SPI2_NOR
        || services.usb != &board::FOXEER_USB_CDC
        || services.watchdog != &board::TIM6
    {
        bail!("generated ADC/SPI2/USB/TIM6 service declaration drifted");
    }

    validate_task_inventory(resolved)?;
    validate_initialization(resolved)?;
    validate_state_inventory(resolved)?;
    validate_channel_inventory(resolved)?;
    validate_capacities(resolved)?;
    validate_timing(resolved)?;
    validate_fault_and_authority_paths(resolved)?;
    Ok(())
}

fn validate_task_inventory(resolved: &ResolvedApp) -> Result<()> {
    if resolved.tasks.len() != EXPECTED_TASKS.len() {
        bail!(
            "generated task inventory has {} tasks; expected {}",
            resolved.tasks.len(),
            EXPECTED_TASKS.len()
        );
    }
    for (id, priority, safety_class, trigger) in EXPECTED_TASKS {
        let actual = task(resolved, id)?;
        let trigger_matches = match (trigger, &actual.trigger) {
            (ExpectedTrigger::Software, ResolvedTaskTrigger::Software) => true,
            (ExpectedTrigger::Interrupt(expected), ResolvedTaskTrigger::Interrupt(actual)) => {
                expected == actual
            }
            _ => false,
        };
        if actual.priority != *priority || actual.safety_class != *safety_class || !trigger_matches
        {
            bail!("generated task `{id}` priority, safety class, or trigger drifted");
        }
    }
    Ok(())
}

fn validate_initialization(resolved: &ResolvedApp) -> Result<()> {
    let expected_spawns = BTreeSet::from([
        "adc_observation_poll",
        "dshot_service",
        "esc_manager",
        "golden_flash",
        "heartbeat",
        "imu_control_bridge",
        "msp_osd",
        "safety_master",
        "serial1_tx_worker",
        "serial2_tx_worker",
    ]);
    let actual_spawns = resolved
        .init_spawns
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual_spawns != expected_spawns || resolved.init_spawns.len() != expected_spawns.len() {
        bail!("generated initialization spawn inventory drifted");
    }

    let expected_dispatchers = BTreeSet::from([
        "CAN1_RX0", "CAN1_RX1", "CAN1_TX", "CAN2_TX", "EXTI0", "EXTI1", "EXTI2", "EXTI3",
    ]);
    let actual_dispatchers = resolved
        .dispatchers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual_dispatchers != expected_dispatchers {
        bail!("generated RTIC dispatcher allocation drifted");
    }
    Ok(())
}

fn validate_state_inventory(resolved: &ResolvedApp) -> Result<()> {
    let expected_len = CONTROL_STATE_FIELDS.len() + SAFETY_STATE_FIELDS.len();
    if resolved.task_state.len() != expected_len {
        bail!("generated persistent state inventory drifted");
    }
    for (owner, field) in CONTROL_STATE_FIELDS
        .iter()
        .map(|field| ("control_loop", field))
        .chain(
            SAFETY_STATE_FIELDS
                .iter()
                .map(|field| ("safety_master", field)),
        )
    {
        if !resolved.task_state.iter().any(|actual| {
            actual.id == field.id && actual.owner_task == owner && actual.recipe == field.recipe
        }) {
            bail!("generated state `{}` or its constructor drifted", field.id);
        }
    }
    Ok(())
}

fn validate_channel_inventory(resolved: &ResolvedApp) -> Result<()> {
    let expected = [
        (
            "actuator_fault_to_safety",
            SafetyMessage::ActuatorPreparation,
            3,
            "actuator_fault_reporter",
            "safety_master",
        ),
        (
            "actuator_to_safety",
            SafetyMessage::ActuatorPreparation,
            3,
            "actuator_output",
            "safety_master",
        ),
        (
            "control_to_actuator",
            SafetyMessage::MotorCommand,
            3,
            "control_loop",
            "actuator_output",
        ),
        (
            "control_to_safety_health",
            SafetyMessage::PreArmHealth,
            4,
            "control_loop",
            "safety_master",
        ),
        (
            "imu_to_control",
            SafetyMessage::ImuSample,
            4,
            "imu_control_bridge",
            "control_loop",
        ),
        (
            "safety_to_actuator_guard",
            SafetyMessage::ActuatorGuard,
            3,
            "safety_master",
            "actuator_output",
        ),
        (
            "sbus_to_control",
            SafetyMessage::SbusInput,
            4,
            "safety_master",
            "control_loop",
        ),
    ];
    if resolved.safety_channels.len() != expected.len() {
        bail!("generated safety-channel inventory drifted");
    }
    for (id, message, capacity, producer, consumer) in expected {
        let Some(actual) = resolved
            .safety_channels
            .iter()
            .find(|channel| channel.id == id)
        else {
            bail!("missing pinned safety channel `{id}`");
        };
        if actual.message != message
            || actual.usable_capacity != capacity
            || actual.producer.owner_task != producer
            || actual.consumer.owner_task != consumer
        {
            bail!("generated safety channel `{id}` drifted");
        }
    }
    Ok(())
}

fn validate_capacities(resolved: &ResolvedApp) -> Result<()> {
    for endpoint in &resolved.serial_endpoints {
        if endpoint.declaration.rx_buffer_count != 4
            || endpoint.declaration.rx_queue_depth != 4
            || endpoint.declaration.tx_queue_depth != 16
        {
            bail!(
                "serial endpoint `{}` capacity drifted",
                endpoint.declaration.id
            );
        }
    }
    if ferrowasp_stm32f4::memory::UART_RX_BUFFERS_PER_PORT != 4
        || ferrowasp_stm32f4::memory::OWNED_UART_RX_QUEUE_DEPTH != 4
        || ferrowasp_stm32f4::memory::OWNED_UART_TX_QUEUE_DEPTH != 16
        || ferrowasp_stm32f4::memory::SPI_RX_BUFFERS != 5
        || ferrowasp_stm32f4::memory::SPI_RX_QUEUE_CAPACITY != 4
        || ferrowasp_tasks::flash_storage::RECORD_QUEUE_CAPACITY != 64
        || ferrowasp_tasks::flash_storage::COMMAND_QUEUE_CAPACITY != 8
        || ferrowasp_tasks::flash_storage::RESPONSE_QUEUE_CAPACITY != 32
        || ferrowasp_tasks::flash_storage::USB_COMMAND_LINE_CAPACITY != 96
        || ferrowasp_tasks::flash_storage::USB_RESPONSE_CAPACITY != 64
        || ferrowasp_tasks::esc_manager::ESC_REQUEST_QUEUE_CAPACITY != 4
        || ferrowasp_tasks::esc_manager::ESC_ACK_QUEUE_CAPACITY != 4
        || ferrowasp_tasks::esc_manager::ESC_TELEMETRY_UPDATE_QUEUE_CAPACITY != 16
        || ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY != 3
    {
        bail!("shared Foxeer transport, storage, ESC, or actuator capacity drifted");
    }
    Ok(())
}

fn validate_timing(resolved: &ResolvedApp) -> Result<()> {
    for (id, logical, milliseconds) in [
        ("safety_master", "poll_interval", 10),
        ("imu_control_bridge", "poll_interval", 1),
        ("msp_osd", "interval", 10),
        ("adc_observation_poll", "interval", 100),
        ("golden_flash", "interval", 1),
        ("heartbeat", "interval", 2_000),
        ("actuator_output", "poll_interval", 10),
        ("dshot_service", "interval", 2),
        ("esc_manager", "interval", 2),
    ] {
        require_millis_config(task(resolved, id)?, logical, milliseconds)?;
    }
    if ferrowasp_tasks::service_telemetry::ADC_OBSERVATION_MAX_AGE_MS != 500 {
        bail!("ADC observation freshness limit drifted");
    }
    Ok(())
}

fn validate_fault_and_authority_paths(resolved: &ResolvedApp) -> Result<()> {
    for id in [
        "actuator_output",
        "dshot_motor1_dma_complete",
        "dshot_motor2_dma_complete",
        "dshot_motor3_dma_complete",
        "dshot_motor4_dma_complete",
        "dshot_service",
        "esc_manager",
    ] {
        if !task(resolved, id)?
            .spawns
            .iter()
            .any(|spawn| spawn.target == "actuator_fault_reporter")
        {
            bail!("physical task `{id}` lost its terminal fault path");
        }
    }

    let motor_hardware_owners = BTreeSet::from([
        "actuator_output",
        "dshot_motor1_dma_complete",
        "dshot_motor2_dma_complete",
        "dshot_motor3_dma_complete",
        "dshot_motor4_dma_complete",
        "dshot_service",
    ]);
    for task in &resolved.tasks {
        let owns_motor_hardware = task
            .local
            .iter()
            .chain(&task.shared)
            .any(|binding| binding.target == "dshot_motors");
        if owns_motor_hardware && !motor_hardware_owners.contains(task.id.as_str()) {
            bail!("task `{}` gained unreviewed motor-hardware access", task.id);
        }
    }
    for id in [
        "adc_observation_dma",
        "adc_observation_poll",
        "golden_flash",
        "heartbeat",
        "io_watchdog",
        "msp_osd",
        "usb_cdc",
    ] {
        if task(resolved, id)?.safety_class != TaskSafetyClass::NonSafetyCritical {
            bail!("service task `{id}` gained safety authority");
        }
    }
    Ok(())
}

/// Renders selected-target parity facts and every reviewed intentional difference.
pub fn render() -> String {
    let mut output = String::new();
    writeln!(output, "## Golden Foxeer reconciliation\n").unwrap();
    writeln!(output, "### Pinned handwritten source state\n").unwrap();
    writeln!(
        output,
        "- Repository revision: `{GOLDEN_SOURCE_REVISION}`.\n- Complete `apps/foxeer-f405-v2` tree: `{GOLDEN_APP_TREE}`.\n- Rust source tree `apps/foxeer-f405-v2/src`: `{GOLDEN_SOURCE_TREE}`.\n- Review scope: `Cargo.toml`, `src/main.rs`, `src/lib.rs`, and `src/board/**` from that tree. Generated dependency-lockfile churn is not used as behavioral evidence.\n"
    )
    .unwrap();

    writeln!(output, "### Semantic graph diff\n").unwrap();
    writeln!(
        output,
        "| Required area | Pinned handwritten app | Generated candidate | Disposition |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- |").unwrap();
    for coverage in COVERAGE {
        writeln!(
            output,
            "| {} | {} | {} | {} |",
            coverage.area, coverage.handwritten, coverage.generated, coverage.disposition
        )
        .unwrap();
    }

    writeln!(output, "\n### Preserved boundaries\n").unwrap();
    for boundary in [
        "Foxeer F405 V2 MCU/clock facts and the selected USART2, UART4, SPI1, SPI2, ADC1, OTG_FS, TIM2, TIM4, TIM5, and TIM6 routes.",
        "The verified IMU sensor-to-body orientation, followed exactly once by the legacy body-to-controller pitch mapping.",
        "The TIM4 priority-14 800 Hz scheduler and 400 Hz control cadence.",
        "Bounded SBUS recovery, freshness, arming-transition, guard-order, controller, Quad-X mixing, motor-order, and command-capacity semantics.",
        "Only safety-owned state can grant an actuator permit; outer control code can only publish typed requests.",
        "TIM1/TIM8 DShot and USART1 telemetry use the reviewed physical routes, while two independent software gates remain disabled.",
        "ADC observations expire after 500 ms; MSP, USB, flash, heartbeat, and watchdog services remain outside actuator authority.",
    ] {
        writeln!(output, "- {boundary}").unwrap();
    }

    writeln!(output, "\n### Locked capacities and cadences\n").unwrap();
    writeln!(output, "| Surface | Generated lock | Golden relationship |").unwrap();
    writeln!(output, "| --- | --- | --- |").unwrap();
    for (surface, generated, golden) in [
        (
            "Serial RX/TX",
            "4 DMA RX buffers, RX queue depth 4 and TX queue depth 16 per typed endpoint",
            "Shared STM32F4 backend values; USART2 TX is generated but unused",
        ),
        (
            "SPI1 IMU",
            "5 DMA RX buffers and queue storage 4",
            "Same shared STM32F4 backend",
        ),
        (
            "Safety channels",
            "SBUS/IMU/health usable 4; motor/guard/completion/fault usable 3",
            "Generated explicit SPSC split of golden signal and motor-command paths",
        ),
        (
            "ESC queues",
            "request 4, acknowledgement 4, telemetry update 16 storage slots",
            "Same reusable ESC manager types",
        ),
        (
            "Flash/USB queues",
            "record 64, command 8, response 32 storage slots; command line 96 and response 64 bytes",
            "Same reusable storage primitives; MSPv2 RPC queues omitted",
        ),
        (
            "Control/services",
            "800/400 Hz control; 2 ms DShot/ESC; 10 ms OSD; 100 ms ADC; 2 s heartbeat; 8 kHz watchdog",
            "Golden cadences preserved",
        ),
    ] {
        writeln!(output, "| {surface} | {generated} | {golden} |").unwrap();
    }

    writeln!(output, "\n### Intentional differences\n").unwrap();
    writeln!(
        output,
        "| Area | Handwritten golden app | Generated candidate | Rationale |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- |").unwrap();
    for difference in DIFFERENCES {
        writeln!(
            output,
            "| {} | {} | {} | {} |",
            difference.area, difference.golden, difference.generated, difference.rationale
        )
        .unwrap();
    }
    writeln!(output, "\n### Reconciliation result\n").unwrap();
    writeln!(
        output,
        "Every requested semantic area is either validation-locked as equivalent or listed above as an intentional deviation. No unexplained difference remains within this pinned graph review. The handwritten Foxeer application remains authoritative; this report does not authorize an automatic cutover or enable either generated actuator gate.\n"
    )
    .unwrap();
    writeln!(output, "\n### Evidence boundary\n").unwrap();
    writeln!(
        output,
        "This report and the associated host/Thumb checks establish deterministic declaration, ownership, rendering, type-checking, and link/check evidence only. They do not establish electrical routing, DMA behavior on silicon, sensor validity, worst-case timing, actuator behavior, props-off bench behavior, or flight readiness.\n"
    )
    .unwrap();
    output
}

fn task<'a>(resolved: &'a ResolvedApp, id: &str) -> Result<&'a ResolvedTask> {
    resolved
        .tasks
        .iter()
        .find(|task| task.id == id)
        .ok_or_else(|| anyhow::anyhow!("missing required `{id}` task"))
}

fn require_task(
    task: &ResolvedTask,
    priority: u8,
    safety_class: TaskSafetyClass,
    software: bool,
) -> Result<()> {
    let trigger_matches = matches!(
        (&task.trigger, software),
        (ResolvedTaskTrigger::Software, true) | (ResolvedTaskTrigger::Interrupt(_), false)
    );
    if task.priority != priority || task.safety_class != safety_class || !trigger_matches {
        bail!(
            "task `{}` safety role, priority, or trigger drifted",
            task.id
        );
    }
    Ok(())
}

fn require_local(task: &ResolvedTask, target: &str) -> Result<()> {
    if !task.local.iter().any(|binding| binding.target == target) {
        bail!("task `{}` does not exclusively own `{target}`", task.id);
    }
    Ok(())
}

fn require_millis_config(task: &ResolvedTask, logical: &str, milliseconds: u32) -> Result<()> {
    let expected = ConfigValue::Millis(MillisDurationU32::millis(milliseconds));
    if !task
        .config
        .iter()
        .any(|binding| binding.logical == logical && binding.value == expected)
    {
        bail!(
            "task `{}` configuration `{logical}` drifted from {milliseconds} ms",
            task.id
        );
    }
    Ok(())
}

fn require_disabled_config(task: &ResolvedTask) -> Result<()> {
    if !task.config.iter().any(|binding| {
        binding.logical == "output_enabled" && binding.value == ConfigValue::Bool(false)
    }) {
        bail!(
            "task `{}` does not retain its independent disabled output gate",
            task.id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rtic::resolve, target::app_composition::APP_COMPOSITION};

    #[test]
    fn selected_spine_matches_the_reviewed_authority_shape() {
        let resolved = resolve::resolve(&APP_COMPOSITION).unwrap();
        validate(&resolved).unwrap();
    }

    #[test]
    fn selected_reconciliation_rejects_an_enabled_physical_gate() {
        let mut resolved = resolve::resolve(&APP_COMPOSITION).unwrap();
        resolved.dshot_actuators[0].declaration.output_enabled = true;

        assert!(validate(&resolved).is_err());
    }

    #[test]
    fn selected_reconciliation_rejects_task_and_capacity_drift() {
        let mut priority_drift = resolve::resolve(&APP_COMPOSITION).unwrap();
        task_mut(&mut priority_drift, "control_loop").priority = 13;
        assert!(validate(&priority_drift).is_err());

        let mut capacity_drift = resolve::resolve(&APP_COMPOSITION).unwrap();
        capacity_drift
            .safety_channels
            .iter_mut()
            .find(|channel| channel.id == "sbus_to_control")
            .unwrap()
            .usable_capacity = 3;
        assert!(validate(&capacity_drift).is_err());
    }

    #[test]
    fn reconciliation_is_source_pinned_and_covers_every_required_area() {
        let report = render();
        for area in [
            "Physical resources",
            "Initialization",
            "Tasks and priorities",
            "Transports",
            "Capacities",
            "Safety state",
            "Fault behavior",
            "Timing",
            "Feature gates",
            "Logging and configuration",
        ] {
            assert!(report.contains(area), "missing semantic diff area {area}");
        }
        for surface in [
            "DShot",
            "USART1 ESC telemetry",
            "USB CDC",
            "ADC sensing",
            "heartbeat",
            "IO watchdog",
            "blackbox/config flash",
            "DisplayPort telemetry",
        ] {
            assert!(
                report.contains(surface),
                "missing reconciliation for {surface}"
            );
        }
        assert!(report.contains(GOLDEN_SOURCE_REVISION));
        assert!(report.contains(GOLDEN_APP_TREE));
        assert!(report.contains(GOLDEN_SOURCE_TREE));
        assert!(report.contains("No unexplained difference remains"));
        assert!(report.contains("does not authorize an automatic cutover"));
        assert!(report.contains("They do not establish electrical routing"));
    }

    fn task_mut<'a>(resolved: &'a mut ResolvedApp, id: &str) -> &'a mut ResolvedTask {
        resolved
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .unwrap()
    }
}
