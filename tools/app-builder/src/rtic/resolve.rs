//! Deterministic expansion and resolution of an [`AppComposition`].
//!
//! Resolution assigns names to component-owned tasks and resources, resolves
//! hardware interrupt vectors, and rejects collisions before source rendering.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};

use crate::{
    backends::stm32f4::{
        board_declaration::{
            BoardDeclaration, ImuInstallationDeclaration, SerialHardwareDeclaration,
            SpiHardwareDeclaration,
        },
        dma_route::{DmaController, DmaRoute, DmaStream},
        dshot::DshotBankHardwareDeclaration,
        dshot_actuator::DshotActuatorDeclaration,
        endpoints::imu::{
            self as imu_endpoint, ImuEndpointDeclaration, ImuEndpointResourceOwnership,
            ImuEndpointResourceRole, ImuEndpointResourceVisibility,
        },
        endpoints::serial::{
            self as serial_endpoint, SerialEndpointDeclaration, SerialEndpointResourceOwnership,
            SerialEndpointResourceRole, SerialEndpointResourceVisibility,
        },
        gpio::GpioMode,
        periodic_control::{
            self as periodic_control, PeriodicControlDeclaration, PeriodicControlResourceRole,
        },
        pins::PinId,
        serial::SerialPeripheral,
        service_hardware::{
            AdcObservationHardwareDeclaration, SpiNorHardwareDeclaration, UsbCdcHardwareDeclaration,
        },
        timer::TimerHardwareDeclaration,
    },
    input_catalog::foxeer_golden_services::GoldenServicesDeclaration,
    rtic::{
        component::ComponentDeclaration,
        composition::{
            self, AppComposition, SharedValue, TaskDeclaration, TaskSafetyClass, TaskTrigger,
        },
        platform_config::PlatformConfig,
        safety_channel::{SafetyChannelDeclaration, SafetyMessage},
        state::{TaskStateDeclaration, TaskStateRecipe, TaskStateRole},
        task::{ConfigValue, SpawnArgument, TaskContract},
        timing::{InitDelayDeclaration, MonotonicDeclaration},
    },
};

/// One application-owned RTIC shared resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedSharedResource {
    /// Generated RTIC field identifier.
    pub id: &'static str,

    /// Initial value returned by `init`.
    pub initial: SharedValue,
}

/// Sole task-local endpoint of an authoritative safety channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSafetyHandle {
    /// Generated RTIC local-resource identifier.
    pub id: &'static str,

    /// Concrete task that exclusively owns this handle.
    pub owner_task: String,
}

/// One validated authoritative SPSC channel and its exclusive owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSafetyChannel {
    /// Private storage identifier used only during RTIC initialization.
    pub id: &'static str,

    /// Authoritative message family carried by the channel.
    pub message: SafetyMessage,

    /// Number of messages accepted without an intervening receive.
    pub usable_capacity: usize,

    /// Sole producer handle and owner.
    pub producer: ResolvedSafetyHandle,

    /// Sole consumer handle and owner.
    pub consumer: ResolvedSafetyHandle,
}

/// One board GPIO selected by a task-local binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedGpioResource {
    /// Board declaration for the selected GPIO.
    pub hardware: &'static crate::backends::stm32f4::gpio::GpioHardwareDeclaration,

    /// Concrete task that owns this local resource.
    pub owner_task: &'static str,
}

/// One application-owned persistent local-state value and its sole owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedTaskState {
    /// Generated RTIC local-resource identifier.
    pub id: &'static str,
    /// Concrete task with exclusive ownership.
    pub owner_task: &'static str,
    /// Reviewed typed initialization recipe.
    pub recipe: TaskStateRecipe,
}

/// One endpoint resource with its generated, instance-qualified ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEndpointResource {
    /// Generated RTIC or initialization-local identifier.
    pub id: String,

    /// Semantic resource role from the reusable endpoint definition.
    pub role: SerialEndpointResourceRole,

    /// RTIC ownership class selected by the endpoint definition.
    pub ownership: SerialEndpointResourceOwnership,

    /// Whether application consumers may bind the resource.
    pub visibility: SerialEndpointResourceVisibility,
}

/// One expanded DMA-backed serial endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSerialEndpoint {
    /// Application endpoint declaration.
    pub declaration: SerialEndpointDeclaration,

    /// Board serial hardware consumed by this endpoint.
    pub hardware: &'static SerialHardwareDeclaration,

    /// Active, instance-qualified endpoint resources.
    pub resources: Vec<ResolvedEndpointResource>,
}

/// One IMU endpoint resource with its generated, instance-qualified ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedImuEndpointResource {
    /// Generated RTIC or initialization-local identifier.
    pub id: String,
    /// Semantic role from the reusable endpoint definition.
    pub role: ImuEndpointResourceRole,
    /// RTIC ownership class selected by the endpoint definition.
    pub ownership: ImuEndpointResourceOwnership,
    /// Whether application consumers may bind the resource.
    pub visibility: ImuEndpointResourceVisibility,
}

/// One expanded DMA-backed SPI IMU endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedImuEndpoint {
    /// Application endpoint declaration.
    pub declaration: ImuEndpointDeclaration,
    /// Board IMU installation selected at boot.
    pub hardware: &'static ImuInstallationDeclaration,
    /// SPI endpoint that owns the selected IMU installation.
    pub spi: &'static SpiHardwareDeclaration,
    /// Instance-qualified endpoint resources.
    pub resources: Vec<ResolvedImuEndpointResource>,
}

/// One periodic-control resource with its generated, instance-qualified ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPeriodicControlResource {
    /// Generated RTIC local-resource identifier.
    pub id: String,
    /// Semantic role from the reusable periodic-control definition.
    pub role: PeriodicControlResourceRole,
}

/// One expanded timer-backed periodic control scheduler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPeriodicControl {
    /// Application periodic-control declaration.
    pub declaration: PeriodicControlDeclaration,
    /// Explicit board timer consumed by the component.
    pub hardware: &'static TimerHardwareDeclaration,
    /// Instance-qualified private task resources.
    pub resources: Vec<ResolvedPeriodicControlResource>,
}

/// One reviewed physical DShot/legacy-telemetry component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDshotActuator {
    /// Application enable gate, priorities, and hardware selection.
    pub declaration: DshotActuatorDeclaration,
    /// Exact four-lane physical timer/pin/DMA bank.
    pub hardware: &'static DshotBankHardwareDeclaration,
    /// Exact receive-only USART1 telemetry route.
    pub telemetry: &'static SerialHardwareDeclaration,
}

/// Mandatory Foxeer observation, persistence, USB, and watchdog services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedGoldenServices {
    /// Selected service priorities, timing, and physical identities.
    pub declaration: GoldenServicesDeclaration,
    /// Exact ADC1 observation route.
    pub adc: &'static AdcObservationHardwareDeclaration,
    /// Exact CPU-serviced SPI2 NOR route.
    pub flash: &'static SpiNorHardwareDeclaration,
    /// Exact OTG_FS USB CDC route.
    pub usb: &'static UsbCdcHardwareDeclaration,
    /// Exact TIM6 watchdog timer.
    pub watchdog: &'static TimerHardwareDeclaration,
}

impl ResolvedPeriodicControl {
    /// Returns the generated ID for one required semantic resource.
    pub fn resource_id(&self, role: PeriodicControlResourceRole) -> Result<&str> {
        self.resources
            .iter()
            .find(|resource| resource.role == role)
            .map(|resource| resource.id.as_str())
            .with_context(|| {
                format!(
                    "periodic control `{}` has no {role:?} resource",
                    self.declaration.id
                )
            })
    }
}

impl ResolvedImuEndpoint {
    /// Returns the generated ID for one required semantic resource.
    pub fn resource_id(&self, role: ImuEndpointResourceRole) -> Result<&str> {
        self.resources
            .iter()
            .find(|resource| resource.role == role)
            .map(|resource| resource.id.as_str())
            .with_context(|| {
                format!(
                    "IMU endpoint `{}` has no {role:?} resource",
                    self.declaration.id
                )
            })
    }
}

impl ResolvedSerialEndpoint {
    /// Returns the generated ID for one required semantic resource.
    pub fn resource_id(&self, role: SerialEndpointResourceRole) -> Result<&str> {
        self.resources
            .iter()
            .find(|resource| resource.role == role)
            .map(|resource| resource.id.as_str())
            .with_context(|| {
                format!(
                    "serial endpoint `{}` has no active {role:?} resource",
                    self.declaration.id
                )
            })
    }
}

/// One logical-to-concrete task resource binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedResourceBinding {
    /// Field name used by the reusable body.
    pub logical: &'static str,

    /// Generated RTIC resource field.
    pub target: String,
}

/// One resolved task configuration value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedConfigBinding {
    /// Field name used below `cx.config`.
    pub logical: &'static str,

    /// Concrete value substituted into the generated body.
    pub value: ConfigValue,
}

/// One resolved logical spawn operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSpawnBinding {
    /// Logical module used by the reusable task body.
    pub logical: &'static str,

    /// Concrete generated task destination.
    pub target: String,

    /// Ordered arguments required by the spawn edge.
    pub arguments: &'static [SpawnArgument],
}

/// Entry mechanism for one resolved task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolvedTaskTrigger {
    /// Hardware task bound to this PAC interrupt identifier.
    Interrupt(String),

    /// Software task entered through RTIC spawn.
    Software,
}

/// One standalone or component-expanded concrete RTIC task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTask {
    /// Generated RTIC task identifier.
    pub id: String,

    /// Reusable task contract and source location.
    pub contract: &'static TaskContract,

    /// Interrupt or software-spawn entry mechanism.
    pub trigger: ResolvedTaskTrigger,

    /// RTIC scheduling priority.
    pub priority: u8,

    /// Safety authority retained from concrete application composition.
    pub safety_class: TaskSafetyClass,

    /// Resolved task-local resource bindings.
    pub local: Vec<ResolvedResourceBinding>,

    /// Resolved task-shared resource bindings.
    pub shared: Vec<ResolvedResourceBinding>,

    /// Values substituted for `cx.config` accesses.
    pub config: Vec<ResolvedConfigBinding>,

    /// Concrete destinations substituted for logical spawn modules.
    pub spawns: Vec<ResolvedSpawnBinding>,

    /// Component instance that created this task, when applicable.
    pub owner_component: Option<&'static str>,
}

/// Fully expanded and validated input to STM32F4 source rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedApp {
    /// Selected board and MCU configuration.
    pub board: &'static BoardDeclaration,

    /// Service-to-port assignments loaded by generated RTIC initialization.
    pub platform: PlatformConfig,

    /// Selected application monotonic.
    pub monotonic: MonotonicDeclaration,

    /// Optional general-purpose synchronous initialization delay.
    pub init_delay: Option<InitDelayDeclaration>,

    /// Application-owned shared values.
    pub shared_resources: Vec<ResolvedSharedResource>,

    /// Exclusive bounded channels in authoritative safety paths.
    pub safety_channels: Vec<ResolvedSafetyChannel>,

    /// Board GPIOs consumed by standalone tasks.
    pub gpio_resources: Vec<ResolvedGpioResource>,

    /// Application-owned persistent task-local state.
    pub task_state: Vec<ResolvedTaskState>,

    /// Expanded serial endpoint instances.
    pub serial_endpoints: Vec<ResolvedSerialEndpoint>,

    /// Expanded SPI IMU endpoint instances.
    pub imu_endpoints: Vec<ResolvedImuEndpoint>,

    /// Expanded timer-backed periodic control schedulers.
    pub periodic_controls: Vec<ResolvedPeriodicControl>,

    /// Reviewed physical actuator components.
    pub dshot_actuators: Vec<ResolvedDshotActuator>,

    /// Mandatory bounded golden service suite.
    pub golden_services: Vec<ResolvedGoldenServices>,

    /// Standalone and endpoint-owned tasks in deterministic render order.
    pub tasks: Vec<ResolvedTask>,

    /// Software task IDs spawned once by RTIC initialization.
    pub init_spawns: Vec<String>,

    /// Free interrupt vectors reserved as RTIC software dispatchers.
    pub dispatchers: Vec<String>,
}

/// Expands components and resolves every resource, task, and interrupt name.
pub fn resolve(composition: &'static AppComposition) -> Result<ResolvedApp> {
    composition::validate(composition).context("validate application composition")?;
    let platform = (composition.platform_config)();

    let shared_resources = composition
        .shared_resources
        .iter()
        .map(|resource| ResolvedSharedResource {
            id: resource.id,
            initial: resource.initial,
        })
        .collect::<Vec<_>>();
    let mut gpio_resources = Vec::new();
    let mut serial_endpoints = Vec::new();
    let mut imu_endpoints = Vec::new();
    let mut periodic_controls = Vec::new();
    let mut dshot_actuators = Vec::new();
    let mut golden_services = Vec::new();
    let mut tasks = composition
        .tasks
        .iter()
        .map(|task| resolve_standalone_task(composition.board, task, &mut gpio_resources))
        .collect::<Result<Vec<_>>>()?;
    let mut init_spawns = composition
        .init_spawns
        .iter()
        .map(|spawn| spawn.task.to_owned())
        .collect::<Vec<_>>();

    for component in composition.components {
        match component {
            ComponentDeclaration::DshotActuator(declaration) => {
                let hardware = composition
                    .board
                    .dshot_bank(declaration.hardware_id)
                    .context("validated DShot hardware disappeared")?;
                let telemetry = composition
                    .board
                    .serial(declaration.telemetry_hardware_id)
                    .context("validated ESC telemetry hardware disappeared")?;
                dshot_actuators.push(ResolvedDshotActuator {
                    declaration: *declaration,
                    hardware,
                    telemetry,
                });
            }
            ComponentDeclaration::ImuEndpoint(declaration) => {
                let endpoint = resolve_imu_endpoint(composition.board, platform, *declaration)?;
                tasks.extend(expand_imu_endpoint_tasks(&endpoint)?);
                imu_endpoints.push(endpoint);
            }
            ComponentDeclaration::GoldenServices(declaration) => {
                let adc = composition
                    .board
                    .adc_observation(declaration.adc_hardware_id)
                    .context("validated ADC hardware disappeared")?;
                let flash = composition
                    .board
                    .spi_nor(declaration.flash_hardware_id)
                    .context("validated SPI NOR hardware disappeared")?;
                let usb = composition
                    .board
                    .usb_cdc(declaration.usb_hardware_id)
                    .context("validated USB hardware disappeared")?;
                let watchdog = composition
                    .board
                    .timer(declaration.watchdog_hardware_id)
                    .context("validated watchdog timer disappeared")?;
                golden_services.push(ResolvedGoldenServices {
                    declaration: *declaration,
                    adc,
                    flash,
                    usb,
                    watchdog,
                });
            }
            ComponentDeclaration::PeriodicControl(declaration) => {
                let control = resolve_periodic_control(composition.board, *declaration)?;
                tasks.push(expand_periodic_control_task(
                    &control,
                    composition.task_state,
                )?);
                periodic_controls.push(control);
            }
            ComponentDeclaration::SerialEndpoint(declaration) => {
                let endpoint = resolve_serial_endpoint(composition.board, *declaration)?;
                let endpoint_tasks = expand_serial_endpoint_tasks(&endpoint)?;
                init_spawns.push(format!("{}_tx_worker", declaration.id));
                tasks.extend(endpoint_tasks);
                serial_endpoints.push(endpoint);
            }
        }
    }

    let task_state = resolve_task_state(composition.task_state, &mut tasks, &periodic_controls)?;
    let safety_channels = composition
        .safety_channels
        .iter()
        .map(|channel| resolve_safety_channel(&tasks, *channel))
        .collect::<Result<Vec<_>>>()?;

    validate_unique_generated_ids(
        &shared_resources,
        &safety_channels,
        &gpio_resources,
        &task_state,
        &serial_endpoints,
        &imu_endpoints,
        &periodic_controls,
        &tasks,
    )?;
    validate_interrupt_ownership(&tasks)?;
    let dispatchers = select_dispatchers(&tasks)?;

    Ok(ResolvedApp {
        board: composition.board,
        platform,
        monotonic: composition.monotonic,
        init_delay: composition.init_delay,
        shared_resources,
        safety_channels,
        gpio_resources,
        task_state,
        serial_endpoints,
        imu_endpoints,
        periodic_controls,
        dshot_actuators,
        golden_services,
        tasks,
        init_spawns,
        dispatchers,
    })
}

fn resolve_safety_channel(
    tasks: &[ResolvedTask],
    channel: SafetyChannelDeclaration,
) -> Result<ResolvedSafetyChannel> {
    let owner = |resource: &str| {
        tasks
            .iter()
            .find(|task| task.local.iter().any(|binding| binding.target == resource))
            .map(|task| task.id.clone())
            .with_context(|| {
                format!(
                    "safety channel `{}` resource `{resource}` lost its validated owner",
                    channel.id
                )
            })
    };
    Ok(ResolvedSafetyChannel {
        id: channel.id,
        message: channel.message,
        usable_capacity: channel.usable_capacity,
        producer: ResolvedSafetyHandle {
            id: channel.producer,
            owner_task: owner(channel.producer)?,
        },
        consumer: ResolvedSafetyHandle {
            id: channel.consumer,
            owner_task: owner(channel.consumer)?,
        },
    })
}

fn resolve_task_state(
    declarations: &'static [TaskStateDeclaration],
    tasks: &mut [ResolvedTask],
    periodic_controls: &[ResolvedPeriodicControl],
) -> Result<Vec<ResolvedTaskState>> {
    let mut resolved = Vec::new();
    for declaration in declarations {
        let task = tasks
            .iter_mut()
            .find(|task| task.id == declaration.owner_task)
            .with_context(|| {
                format!(
                    "task-state schema `{}` version {} targets missing task `{}`",
                    declaration.schema.id, declaration.schema.version, declaration.owner_task
                )
            })?;
        for field in declaration.fields {
            let existing = task
                .local
                .iter()
                .find(|binding| binding.logical == field.id || binding.target == field.id);
            if existing
                .is_some_and(|binding| binding.logical != field.id || binding.target != field.id)
            {
                bail!(
                    "task-state field `{}` collides with an existing local binding on task `{}`",
                    field.id,
                    task.id
                );
            }
            if field.role == TaskStateRole::SamplesPerControlLoop
                && let Some(owner_component) = task.owner_component
                && let Some(control) = periodic_controls
                    .iter()
                    .find(|control| control.declaration.id == owner_component)
            {
                let TaskStateRecipe::U32(samples) = field.recipe else {
                    unreachable!("task-state schema validation fixed this recipe type")
                };
                let expected = control.declaration.ticks_per_control();
                if samples != expected {
                    bail!(
                        "task-state field `{}` supplies {samples} scheduler ticks per control step, but periodic control `{owner_component}` resolves to {expected}",
                        field.id
                    );
                }
            }
            if existing.is_none() {
                task.local.push(ResolvedResourceBinding {
                    logical: field.id,
                    target: field.id.to_owned(),
                });
            }
            resolved.push(ResolvedTaskState {
                id: field.id,
                owner_task: declaration.owner_task,
                recipe: field.recipe,
            });
        }
    }
    Ok(resolved)
}

fn resolve_imu_endpoint(
    board: &'static BoardDeclaration,
    platform: PlatformConfig,
    declaration: ImuEndpointDeclaration,
) -> Result<ResolvedImuEndpoint> {
    let spi_id = declaration.hardware_id;
    let spi = board.spi(spi_id).with_context(|| {
        format!(
            "SPI endpoint `{}` hardware `{}` disappeared after validation",
            declaration.id, spi_id
        )
    })?;
    let assignment = platform
        .spi
        .into_iter()
        .flatten()
        .find(|assignment| match assignment.port {
            crate::rtic::platform_config::SpiPort::Spi1 => declaration.id == "spi1",
        })
        .with_context(|| {
            format!(
                "SPI endpoint `{}` has no boot-time service assignment",
                declaration.id
            )
        })?;
    let crate::rtic::platform_config::SpiService::Imu(installation_id) = assignment.service;
    let hardware = spi.imu(installation_id).with_context(|| {
        format!(
            "SPI endpoint `{}` IMU installation {} disappeared after validation",
            declaration.id,
            installation_id.get()
        )
    })?;
    let resources = declaration
        .definition
        .resources
        .iter()
        .map(|resource| ResolvedImuEndpointResource {
            id: if resource.role == ImuEndpointResourceRole::Sample {
                platform
                    .spi
                    .into_iter()
                    .flatten()
                    .find(|assignment| match assignment.port {
                        crate::rtic::platform_config::SpiPort::Spi1 => declaration.id == "spi1",
                    })
                    .map_or_else(
                        || {
                            format!(
                                "{}_{}",
                                declaration.id,
                                imu_endpoint::resource_suffix(resource.role)
                            )
                        },
                        |assignment| assignment.service.sample_resource().to_owned(),
                    )
            } else {
                format!(
                    "{}_{}",
                    declaration.id,
                    imu_endpoint::resource_suffix(resource.role)
                )
            },
            role: resource.role,
            ownership: resource.ownership,
            visibility: resource.visibility,
        })
        .collect::<Vec<_>>();
    let mut roles = BTreeSet::new();
    for resource in &resources {
        if !roles.insert(format!("{:?}", resource.role)) {
            bail!(
                "IMU endpoint definition `{}` repeats resource role `{:?}`",
                declaration.definition.id,
                resource.role
            );
        }
    }
    Ok(ResolvedImuEndpoint {
        declaration,
        hardware,
        spi,
        resources,
    })
}

fn resolve_standalone_task(
    board: &'static BoardDeclaration,
    task: &'static TaskDeclaration,
    gpio_resources: &mut Vec<ResolvedGpioResource>,
) -> Result<ResolvedTask> {
    let local = task
        .local
        .iter()
        .map(|binding| {
            if let Some(hardware) = board.gpio(binding.target()) {
                gpio_resources.push(ResolvedGpioResource {
                    hardware,
                    owner_task: task.id,
                });
            }
            Ok(ResolvedResourceBinding {
                logical: binding.logical(),
                target: binding.target().to_owned(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let trigger = match task.trigger {
        TaskTrigger::Software => ResolvedTaskTrigger::Software,
        TaskTrigger::Interrupt(interrupt) => {
            let binding = format!("{interrupt:?}");
            validate_exti_binding(task.id, &binding, board, task)?;
            ResolvedTaskTrigger::Interrupt(binding)
        }
    };

    Ok(ResolvedTask {
        id: task.id.to_owned(),
        contract: task.contract,
        trigger,
        priority: task.priority,
        safety_class: task.safety_class,
        local,
        shared: task
            .shared
            .iter()
            .map(|binding| ResolvedResourceBinding {
                logical: binding.logical(),
                target: binding.target().to_owned(),
            })
            .collect(),
        config: task
            .config
            .iter()
            .map(|binding| ResolvedConfigBinding {
                logical: binding.logical(),
                value: binding.value(),
            })
            .collect(),
        spawns: task
            .spawns
            .iter()
            .map(|binding| ResolvedSpawnBinding {
                logical: binding.logical(),
                target: binding.target().to_owned(),
                arguments: binding.arguments(),
            })
            .collect(),
        owner_component: None,
    })
}

fn validate_exti_binding(
    task_id: &str,
    binding: &str,
    board: &BoardDeclaration,
    task: &TaskDeclaration,
) -> Result<()> {
    if !binding.starts_with("EXTI") {
        return Ok(());
    }
    let interrupt_gpio = task
        .local
        .iter()
        .filter_map(|resource| board.gpio(resource.target()))
        .find(|gpio| matches!(gpio.mode, GpioMode::Input { interrupt: Some(_) }))
        .with_context(|| format!("EXTI task `{task_id}` has no interrupt-enabled GPIO"))?;
    let expected = exti_binding(interrupt_gpio.pin);
    if binding != expected {
        bail!(
            "EXTI task `{task_id}` binds `{binding}`, but GPIO `{}` on {:?} resolves to `{expected}`",
            interrupt_gpio.id,
            interrupt_gpio.pin
        );
    }
    Ok(())
}

fn exti_binding(pin: PinId) -> &'static str {
    match pin.pin {
        0 => "EXTI0",
        1 => "EXTI1",
        2 => "EXTI2",
        3 => "EXTI3",
        4 => "EXTI4",
        5..=9 => "EXTI9_5",
        10..=15 => "EXTI15_10",
        _ => unreachable!("PinId validates the STM32 GPIO pin range"),
    }
}

fn resolve_serial_endpoint(
    board: &'static BoardDeclaration,
    declaration: SerialEndpointDeclaration,
) -> Result<ResolvedSerialEndpoint> {
    let hardware_id = declaration.hardware_id;
    let hardware = board.serial(hardware_id).with_context(|| {
        format!(
            "serial endpoint `{}` hardware `{}` disappeared after validation",
            declaration.id, hardware_id
        )
    })?;
    if declaration.rx_buffer_count != 4 || declaration.rx_queue_depth != 4 {
        bail!(
            "serial endpoint `{}` requests RX storage {}/{}, but the current STM32F4 backend supports 4 buffers and depth 4",
            declaration.id,
            declaration.rx_buffer_count,
            declaration.rx_queue_depth
        );
    }
    if declaration.tx_queue_depth != 16 {
        bail!(
            "serial endpoint `{}` requests TX depth {}, but the current STM32F4 backend supports depth 16",
            declaration.id,
            declaration.tx_queue_depth
        );
    }

    let resources = declaration
        .definition
        .resources
        .iter()
        .map(|resource| ResolvedEndpointResource {
            id: format!(
                "{}_{}",
                declaration.id,
                serial_endpoint::resource_suffix(resource.role)
            ),
            role: resource.role,
            ownership: resource.ownership,
            visibility: resource.visibility,
        })
        .collect::<Vec<_>>();
    let mut roles = BTreeSet::new();
    for resource in &resources {
        if !roles.insert(format!("{:?}", resource.role)) {
            bail!(
                "serial endpoint definition `{}` repeats resource role `{:?}`",
                declaration.definition.id,
                resource.role
            );
        }
    }

    Ok(ResolvedSerialEndpoint {
        declaration,
        hardware,
        resources,
    })
}

fn resolve_periodic_control(
    board: &'static BoardDeclaration,
    declaration: PeriodicControlDeclaration,
) -> Result<ResolvedPeriodicControl> {
    let hardware = board.timer(declaration.hardware_id).with_context(|| {
        format!(
            "periodic control `{}` hardware `{}` disappeared after validation",
            declaration.id, declaration.hardware_id
        )
    })?;
    let resources = declaration
        .definition
        .resources
        .iter()
        .map(|resource| ResolvedPeriodicControlResource {
            id: format!(
                "{}_{}",
                declaration.id,
                periodic_control::resource_suffix(resource.role)
            ),
            role: resource.role,
        })
        .collect::<Vec<_>>();
    let mut roles = BTreeSet::new();
    for resource in &resources {
        if !roles.insert(format!("{:?}", resource.role)) {
            bail!(
                "periodic control definition `{}` repeats resource role `{:?}`",
                declaration.definition.id,
                resource.role
            );
        }
    }
    Ok(ResolvedPeriodicControl {
        declaration,
        hardware,
        resources,
    })
}

fn expand_periodic_control_task(
    control: &ResolvedPeriodicControl,
    task_state: &[TaskStateDeclaration],
) -> Result<ResolvedTask> {
    use PeriodicControlResourceRole::{Phase, Scheduler};

    let declaration = control.declaration;
    let task_id = format!("{}_loop", declaration.id);
    let mut local = vec![
        ("scheduler", control.resource_id(Scheduler)?),
        ("phase", control.resource_id(Phase)?),
    ];
    local.extend(
        declaration
            .local
            .iter()
            .map(|binding| (binding.logical(), binding.target())),
    );
    if let Some(state) = task_state.iter().find(|state| state.owner_task == task_id) {
        for field in state.fields {
            if let Some(requirement) = declaration
                .task
                .local
                .iter()
                .find(|requirement| requirement.id == field.id)
                && !normalized_rust_types_match(
                    requirement.rust_type,
                    field.recipe.state_type().rust_type(),
                )
            {
                bail!(
                    "component task `{task_id}` state field `{}` requires `{}`, but its recipe supplies `{}`",
                    field.id,
                    requirement.rust_type,
                    field.recipe.state_type().rust_type()
                );
            }
        }
        local.extend(state.fields.iter().filter_map(|field| {
            declaration
                .task
                .local
                .iter()
                .any(|requirement| requirement.id == field.id)
                .then_some((field.id, field.id))
        }));
    }

    let mut task = component_task_with_bindings(
        declaration.id,
        task_id,
        declaration.task,
        ResolvedTaskTrigger::Interrupt(control.hardware.peripheral.update_interrupt().to_owned()),
        declaration.interrupt_priority,
        local,
        declaration
            .shared
            .iter()
            .map(|binding| (binding.logical(), binding.target()))
            .collect(),
        vec![(
            "ticks_per_control",
            ConfigValue::U32(declaration.ticks_per_control()),
        )],
        declaration
            .spawns
            .iter()
            .map(|binding| (binding.logical(), binding.target()))
            .collect(),
    )?;
    task.safety_class = declaration.safety_class;
    Ok(task)
}

fn normalized_rust_types_match(left: &str, right: &str) -> bool {
    left.chars()
        .filter(|character| !character.is_whitespace())
        .eq(right.chars().filter(|character| !character.is_whitespace()))
}

fn expand_serial_endpoint_tasks(endpoint: &ResolvedSerialEndpoint) -> Result<Vec<ResolvedTask>> {
    let declaration = endpoint.declaration;
    let rx_route = endpoint
        .hardware
        .port
        .rx
        .context("validated serial endpoint lost its RX route")?;
    let rx_dma = rx_route
        .dma
        .context("validated serial endpoint lost its RX DMA route")?;
    let rx = endpoint.resource_id(SerialEndpointResourceRole::RxService)?;
    let rx_bridge = endpoint.resource_id(SerialEndpointResourceRole::RxParser)?;
    let rx_bridge_id = format!("{}_rx_bridge", declaration.id);
    let mut tasks = vec![
        component_task_with_bindings(
            declaration.id,
            format!("{}_rx_idle_irq", declaration.id),
            declaration.definition.tasks.peripheral_irq,
            ResolvedTaskTrigger::Interrupt(serial_interrupt(endpoint.hardware.port.peripheral)),
            declaration.interrupt_priority,
            vec![],
            vec![("rx", rx)],
            vec![],
            vec![("bridge", rx_bridge_id.as_str())],
        )?,
        component_task_with_bindings(
            declaration.id,
            format!("{}_rx_dma_irq", declaration.id),
            declaration.definition.tasks.rx_dma_irq,
            ResolvedTaskTrigger::Interrupt(dma_interrupt(rx_dma)),
            declaration.interrupt_priority,
            vec![],
            vec![("rx", rx)],
            vec![],
            vec![("bridge", rx_bridge_id.as_str())],
        )?,
        component_task(
            declaration.id,
            rx_bridge_id,
            declaration.definition.tasks.rx_bridge,
            ResolvedTaskTrigger::Software,
            declaration.bridge_priority,
            vec![("bridge", rx_bridge)],
            vec![],
        )?,
    ];

    let tx_route = endpoint
        .hardware
        .port
        .tx
        .context("validated serial endpoint lost its TX route")?;
    let tx_dma_route = tx_route
        .dma
        .context("validated serial endpoint lost its TX DMA route")?;
    let tx_dma = endpoint.resource_id(SerialEndpointResourceRole::TxDma)?;
    let completion = endpoint.resource_id(SerialEndpointResourceRole::TxCompletion)?;
    let owner = endpoint.resource_id(SerialEndpointResourceRole::TxOwner)?;
    tasks.push(component_task(
        declaration.id,
        format!("{}_tx_dma_irq", declaration.id),
        declaration.definition.tasks.tx_dma_irq,
        ResolvedTaskTrigger::Interrupt(dma_interrupt(tx_dma_route)),
        declaration.interrupt_priority,
        vec![("completion", completion)],
        vec![("tx_dma", tx_dma)],
    )?);
    tasks.push(component_task(
        declaration.id,
        format!("{}_tx_worker", declaration.id),
        declaration.definition.tasks.tx_worker,
        ResolvedTaskTrigger::Software,
        declaration
            .worker_priority
            .context("validated endpoint lost its worker priority")?,
        vec![("owner", owner)],
        vec![("tx_dma", tx_dma)],
    )?);

    Ok(tasks)
}

fn expand_imu_endpoint_tasks(endpoint: &ResolvedImuEndpoint) -> Result<Vec<ResolvedTask>> {
    use ImuEndpointResourceRole as Role;

    let declaration = endpoint.declaration;
    let owner = endpoint.resource_id(Role::Owner)?;
    let kind = endpoint.resource_id(Role::Kind)?;
    let parser = endpoint.resource_id(Role::Parser)?;
    let sample = endpoint.resource_id(Role::Sample)?;
    let device = endpoint.resource_id(Role::Device)?;
    let data_ready = endpoint.resource_id(Role::DataReady)?;
    let unavailable_logged = endpoint.resource_id(Role::UnavailableLogged)?;
    let poll_id = format!("{}_poll", declaration.id);
    let timeout_id = format!("{}_timeout", declaration.id);
    let parser_id = format!("{}_parser", declaration.id);

    Ok(vec![
        component_task_with_bindings(
            declaration.id,
            format!("{}_data_ready", declaration.id),
            declaration.definition.tasks.data_ready,
            ResolvedTaskTrigger::Interrupt(exti_binding(endpoint.hardware.data_ready).to_owned()),
            declaration.data_ready_priority,
            vec![("data_ready", data_ready)],
            vec![("kind", kind)],
            vec![],
            vec![("poll", poll_id.as_str())],
        )?,
        component_task_with_bindings(
            declaration.id,
            poll_id,
            declaration.definition.tasks.poll,
            ResolvedTaskTrigger::Software,
            declaration.poll_priority,
            vec![
                ("device", device),
                ("unavailable_logged", unavailable_logged),
            ],
            vec![("kind", kind)],
            vec![],
            vec![("timeout", timeout_id.as_str())],
        )?,
        component_task_with_bindings(
            declaration.id,
            format!("{}_owner_service", declaration.id),
            declaration.definition.tasks.owner_service,
            ResolvedTaskTrigger::Software,
            declaration.owner_priority,
            vec![],
            vec![("owner", owner)],
            vec![],
            vec![],
        )?,
        component_task_with_bindings(
            declaration.id,
            format!("{}_rx_dma_irq", declaration.id),
            declaration.definition.tasks.rx_dma_irq,
            ResolvedTaskTrigger::Interrupt(dma_interrupt(endpoint.spi.bus.dma.rx)),
            declaration.owner_priority,
            vec![],
            vec![("owner", owner)],
            vec![],
            vec![("parse", parser_id.as_str())],
        )?,
        component_task_with_bindings(
            declaration.id,
            timeout_id,
            declaration.definition.tasks.timeout,
            ResolvedTaskTrigger::Software,
            declaration.owner_priority,
            vec![],
            vec![("owner", owner)],
            vec![(
                "check_after",
                ConfigValue::Millis(fugit::MillisDurationU32::millis(1)),
            )],
            vec![],
        )?,
        component_task_with_bindings(
            declaration.id,
            parser_id,
            declaration.definition.tasks.parser,
            ResolvedTaskTrigger::Software,
            declaration.parser_priority,
            vec![("parser", parser)],
            vec![("kind", kind), ("sample", sample)],
            vec![(
                "orientation",
                ConfigValue::FrameRotation(endpoint.hardware.orientation),
            )],
            vec![],
        )?,
    ])
}

fn component_task(
    owner: &'static str,
    id: String,
    contract: &'static TaskContract,
    trigger: ResolvedTaskTrigger,
    priority: u8,
    local: Vec<(&'static str, &str)>,
    shared: Vec<(&'static str, &str)>,
) -> Result<ResolvedTask> {
    component_task_with_bindings(
        owner,
        id,
        contract,
        trigger,
        priority,
        local,
        shared,
        vec![],
        vec![],
    )
}

#[allow(clippy::too_many_arguments)]
fn component_task_with_bindings(
    owner: &'static str,
    id: String,
    contract: &'static TaskContract,
    trigger: ResolvedTaskTrigger,
    priority: u8,
    local: Vec<(&'static str, &str)>,
    shared: Vec<(&'static str, &str)>,
    config: Vec<(&'static str, ConfigValue)>,
    spawns: Vec<(&'static str, &str)>,
) -> Result<ResolvedTask> {
    validate_contract_resources(&id, "local", contract.local, &local)?;
    validate_contract_resources(&id, "shared", contract.shared, &shared)?;
    validate_contract_config(&id, contract, &config)?;
    validate_contract_spawns(&id, contract, &spawns)?;
    Ok(ResolvedTask {
        id,
        contract,
        trigger,
        priority,
        safety_class: TaskSafetyClass::NonSafetyCritical,
        local: local
            .into_iter()
            .map(|(logical, target)| ResolvedResourceBinding {
                logical,
                target: target.to_owned(),
            })
            .collect(),
        shared: shared
            .into_iter()
            .map(|(logical, target)| ResolvedResourceBinding {
                logical,
                target: target.to_owned(),
            })
            .collect(),
        config: config
            .into_iter()
            .map(|(logical, value)| ResolvedConfigBinding { logical, value })
            .collect(),
        spawns: spawns
            .into_iter()
            .map(|(logical, target)| {
                let requirement = contract
                    .spawns
                    .iter()
                    .find(|requirement| requirement.id == logical)
                    .expect("validated spawn binding must have a contract requirement");
                ResolvedSpawnBinding {
                    logical,
                    target: target.to_owned(),
                    arguments: requirement.arguments,
                }
            })
            .collect(),
        owner_component: Some(owner),
    })
}

fn validate_contract_config(
    task: &str,
    contract: &TaskContract,
    bindings: &[(&str, ConfigValue)],
) -> Result<()> {
    let required = contract
        .config
        .iter()
        .map(|requirement| (requirement.id, requirement.rust_type))
        .collect::<BTreeMap<_, _>>();
    let supplied = bindings
        .iter()
        .map(|(logical, value)| (*logical, value.rust_type()))
        .collect::<BTreeMap<_, _>>();
    if required != supplied {
        bail!(
            "component task `{task}` config bindings {:?} do not match contract {:?}",
            supplied,
            required
        );
    }
    Ok(())
}

fn validate_contract_spawns(
    task: &str,
    contract: &TaskContract,
    bindings: &[(&str, &str)],
) -> Result<()> {
    let required = contract
        .spawns
        .iter()
        .map(|requirement| requirement.id)
        .collect::<BTreeSet<_>>();
    let supplied = bindings
        .iter()
        .map(|(logical, _)| *logical)
        .collect::<BTreeSet<_>>();
    if required != supplied {
        bail!(
            "component task `{task}` spawn bindings {:?} do not match contract {:?}",
            supplied,
            required
        );
    }
    Ok(())
}

fn validate_contract_resources(
    task: &str,
    kind: &str,
    requirements: &[crate::rtic::task::ResourceRequirement],
    bindings: &[(&str, &str)],
) -> Result<()> {
    let required = requirements
        .iter()
        .map(|requirement| requirement.id)
        .collect::<BTreeSet<_>>();
    let supplied = bindings
        .iter()
        .map(|(logical, _)| *logical)
        .collect::<BTreeSet<_>>();
    if required != supplied {
        bail!(
            "component task `{task}` {kind} bindings {:?} do not match contract {:?}",
            supplied,
            required
        );
    }
    Ok(())
}

fn serial_interrupt(peripheral: SerialPeripheral) -> String {
    match peripheral {
        SerialPeripheral::Usart1 => "USART1",
        SerialPeripheral::Usart2 => "USART2",
        SerialPeripheral::Usart3 => "USART3",
        SerialPeripheral::Uart4 => "UART4",
        SerialPeripheral::Uart5 => "UART5",
        SerialPeripheral::Usart6 => "USART6",
        SerialPeripheral::Uart7 => "UART7",
        SerialPeripheral::Uart8 => "UART8",
    }
    .to_owned()
}

fn dma_interrupt(route: DmaRoute) -> String {
    format!(
        "DMA{}_STREAM{}",
        match route.controller {
            DmaController::Dma1 => 1,
            DmaController::Dma2 => 2,
        },
        match route.stream {
            DmaStream::Stream0 => 0,
            DmaStream::Stream1 => 1,
            DmaStream::Stream2 => 2,
            DmaStream::Stream3 => 3,
            DmaStream::Stream4 => 4,
            DmaStream::Stream5 => 5,
            DmaStream::Stream6 => 6,
            DmaStream::Stream7 => 7,
        }
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_unique_generated_ids(
    shared: &[ResolvedSharedResource],
    safety_channels: &[ResolvedSafetyChannel],
    gpio: &[ResolvedGpioResource],
    task_state: &[ResolvedTaskState],
    endpoints: &[ResolvedSerialEndpoint],
    imu_endpoints: &[ResolvedImuEndpoint],
    periodic_controls: &[ResolvedPeriodicControl],
    tasks: &[ResolvedTask],
) -> Result<()> {
    let mut resource_ids = BTreeMap::<&str, &str>::new();
    for resource in shared {
        insert_unique(
            &mut resource_ids,
            resource.id,
            "application shared resource",
        )?;
    }
    for channel in safety_channels {
        insert_unique(&mut resource_ids, channel.id, "safety-channel storage")?;
        insert_unique(
            &mut resource_ids,
            channel.producer.id,
            "safety-channel producer",
        )?;
        insert_unique(
            &mut resource_ids,
            channel.consumer.id,
            "safety-channel consumer",
        )?;
    }
    for resource in gpio {
        insert_unique(&mut resource_ids, resource.hardware.id, "board GPIO")?;
    }
    for state in task_state {
        insert_unique(&mut resource_ids, state.id, "application task-local state")?;
    }
    for endpoint in endpoints {
        for resource in &endpoint.resources {
            insert_unique(&mut resource_ids, &resource.id, "serial endpoint resource")?;
        }
    }
    for endpoint in imu_endpoints {
        for resource in &endpoint.resources {
            insert_unique(&mut resource_ids, &resource.id, "IMU endpoint resource")?;
        }
    }
    for control in periodic_controls {
        for resource in &control.resources {
            insert_unique(&mut resource_ids, &resource.id, "periodic control resource")?;
        }
    }

    let mut task_ids = BTreeMap::<&str, &str>::new();
    for task in tasks {
        insert_unique(&mut task_ids, &task.id, "resolved task")?;
    }
    Ok(())
}

fn insert_unique<'a>(
    ids: &mut BTreeMap<&'a str, &'static str>,
    id: &'a str,
    kind: &'static str,
) -> Result<()> {
    if let Some(existing) = ids.insert(id, kind) {
        bail!("generated ID `{id}` is used by both {existing} and {kind}");
    }
    Ok(())
}

fn validate_interrupt_ownership(tasks: &[ResolvedTask]) -> Result<()> {
    let mut owners = BTreeMap::<&str, &str>::new();
    for task in tasks {
        let ResolvedTaskTrigger::Interrupt(binding) = &task.trigger else {
            continue;
        };
        if let Some(existing) = owners.insert(binding, &task.id) {
            bail!(
                "resolved tasks `{existing}` and `{}` both bind interrupt `{binding}`",
                task.id
            );
        }
    }
    Ok(())
}

fn select_dispatchers(tasks: &[ResolvedTask]) -> Result<Vec<String>> {
    let used_interrupts = tasks
        .iter()
        .filter_map(|task| match &task.trigger {
            ResolvedTaskTrigger::Interrupt(binding) => Some(binding.as_str()),
            ResolvedTaskTrigger::Software => None,
        })
        .collect::<BTreeSet<_>>();
    let count = tasks
        .iter()
        .filter(|task| matches!(task.trigger, ResolvedTaskTrigger::Software))
        .map(|task| task.priority)
        .collect::<BTreeSet<_>>()
        .len()
        .max(1);
    let dispatchers = [
        "EXTI0", "EXTI1", "EXTI2", "EXTI3", "EXTI4", "CAN1_TX", "CAN2_TX", "CAN1_RX0", "CAN1_RX1",
        "CAN1_SCE", "CAN2_RX0", "CAN2_RX1",
    ]
    .into_iter()
    .filter(|candidate| !used_interrupts.contains(candidate))
    .take(count)
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if dispatchers.len() != count {
        bail!(
            "STM32F4 target needs {count} RTIC dispatchers but its reviewed dispatcher pool is exhausted"
        );
    }
    Ok(dispatchers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backends::stm32f4::periodic_control::PERIODIC_CONTROL_TIMER,
        input_catalog::foxeer_f405_v2::{app_composition::APP_COMPOSITION, board},
        rtic::{
            composition::{AppComposition, TaskDeclaration},
            platform_config::empty_platform_config,
            state::{
                TaskStateDeclaration, TaskStateField, TaskStateRecipe, TaskStateRequirement,
                TaskStateRole, TaskStateSchema, TaskStateType,
            },
            task::{TaskContract, TaskSource},
            timing::MonotonicDeclaration,
        },
    };

    const TIMING_ONLY_CONTROL: PeriodicControlDeclaration = PERIODIC_CONTROL_TIMER
        .declare("control", board::TIM4.id)
        .scheduler_hz(800)
        .control_hz(400)
        .interrupt_priority(14);

    #[test]
    fn current_composition_expands_serial_endpoint_tasks_and_resources() {
        let resolved = resolve(&APP_COMPOSITION).unwrap();

        assert!(
            resolved
                .tasks
                .iter()
                .any(|task| task.id == "serial2_tx_worker")
        );
        assert!(
            resolved
                .init_spawns
                .contains(&"serial2_tx_worker".to_owned())
        );
        assert_eq!(resolved.serial_endpoints.len(), 2);
        assert_eq!(resolved.imu_endpoints.len(), 1);
        assert_eq!(resolved.periodic_controls.len(), 1);
        assert_eq!(resolved.safety_channels.len(), 7);
        assert_eq!(resolved.dshot_actuators.len(), 1);
        assert!(!resolved.dshot_actuators[0].declaration.output_enabled);
        assert_eq!(resolved.task_state.len(), 17);
        assert!(
            resolved
                .task_state
                .iter()
                .filter(|state| state.owner_task == "control_loop")
                .count()
                == 16
        );
        assert!(resolved.task_state.iter().any(|state| {
            state.owner_task == "safety_master" && state.id == "foxeer_safety_state"
        }));
        assert!(resolved.tasks.iter().any(|task| task.id == "spi1_poll"));
        let control = resolved
            .tasks
            .iter()
            .find(|task| task.id == "control_loop")
            .unwrap();
        assert_eq!(
            control.trigger,
            ResolvedTaskTrigger::Interrupt("TIM4".to_owned())
        );
        assert_eq!(control.priority, 14);
        assert!(
            control
                .local
                .iter()
                .any(|binding| binding.target == "flight_controller")
        );
        assert_eq!(
            control.config,
            [ResolvedConfigBinding {
                logical: "ticks_per_control",
                value: ConfigValue::U32(2),
            }]
        );
        let serial2 = resolved
            .serial_endpoints
            .iter()
            .find(|endpoint| endpoint.declaration.id == "serial2")
            .unwrap();
        assert_eq!(
            serial2
                .resource_id(SerialEndpointResourceRole::TxDma)
                .unwrap(),
            "serial2_tx_dma"
        );
        assert!(
            resolved
                .tasks
                .iter()
                .any(|task| task.id == "serial1_rx_bridge")
        );
        assert!(resolved.tasks.iter().any(|task| task.id == "safety_master"));
        assert!(
            resolved
                .tasks
                .iter()
                .any(|task| task.id == "imu_control_bridge")
        );
        let sbus = resolved
            .safety_channels
            .iter()
            .find(|channel| channel.id == "sbus_to_control")
            .unwrap();
        assert_eq!(sbus.producer.owner_task, "safety_master");
        assert_eq!(sbus.consumer.owner_task, "control_loop");
        let imu = resolved
            .safety_channels
            .iter()
            .find(|channel| channel.id == "imu_to_control")
            .unwrap();
        assert_eq!(imu.producer.owner_task, "imu_control_bridge");
        assert_eq!(imu.consumer.owner_task, "control_loop");
        let motor = resolved
            .safety_channels
            .iter()
            .find(|channel| channel.id == "control_to_actuator")
            .unwrap();
        assert_eq!(motor.message, SafetyMessage::MotorCommand);
        assert_eq!(motor.usable_capacity, 3);
        assert_eq!(motor.producer.id, "motor_cmd_producer");
        assert_eq!(motor.producer.owner_task, "control_loop");
        assert_eq!(motor.consumer.id, "motor_cmd_consumer");
        assert_eq!(motor.consumer.owner_task, "actuator_output");
        let health = resolved
            .safety_channels
            .iter()
            .find(|channel| channel.id == "control_to_safety_health")
            .unwrap();
        assert_eq!(health.message, SafetyMessage::PreArmHealth);
        assert_eq!(health.producer.owner_task, "control_loop");
        assert_eq!(health.consumer.owner_task, "safety_master");
        assert_eq!(
            control
                .local
                .iter()
                .filter(|binding| binding.target == "motor_cmd_producer")
                .count(),
            1
        );
        assert_eq!(control.spawns.len(), 1);
        assert_eq!(control.spawns[0].logical, "actuator_wake");
        assert_eq!(control.spawns[0].target, "actuator_output");
        assert_eq!(
            control.spawns[0].arguments,
            [SpawnArgument::new(
                "request",
                "ferrowasp_core::safety::ActuatorCmd",
            )]
        );

        let actuator = resolved
            .tasks
            .iter()
            .find(|task| task.id == "actuator_output")
            .unwrap();
        assert_eq!(actuator.trigger, ResolvedTaskTrigger::Software);
        assert_eq!(actuator.priority, 15);
        assert_eq!(actuator.safety_class, TaskSafetyClass::SafetyCritical);
        assert_eq!(actuator.contract.id, "physical_actuator");
        assert_eq!(actuator.local.len(), 4);
        assert_eq!(actuator.local[0].target, "motor_cmd_consumer");
        assert_eq!(actuator.shared.len(), 1);
        assert_eq!(actuator.shared[0].target, "dshot_motors");
        assert_eq!(actuator.spawns.len(), 1);
        assert_eq!(actuator.spawns[0].target, "actuator_fault_reporter");
        assert!(actuator.owner_component.is_none());
        assert!(!resolved.init_spawns.contains(&"actuator_output".to_owned()));

        let safety = resolved
            .tasks
            .iter()
            .find(|task| task.id == "safety_master")
            .unwrap();
        assert_eq!(safety.priority, 16);
        assert_eq!(safety.safety_class, TaskSafetyClass::SafetyCritical);
        assert!(resolved.init_spawns.contains(&"safety_master".to_owned()));
        assert!(
            safety
                .local
                .iter()
                .any(|binding| { binding.target == "rc_sbus_rx_discontinuities" })
        );
        assert!(
            safety
                .local
                .iter()
                .any(|binding| { binding.target == "foxeer_safety_state" })
        );
        assert_eq!(safety.spawns.len(), 1);
        assert_eq!(safety.spawns[0].target, "actuator_output");
    }

    #[test]
    fn current_interrupts_allocate_priority_16_safety_and_priority_15_actuator_dispatchers() {
        let resolved = resolve(&APP_COMPOSITION).unwrap();
        assert_eq!(
            resolved.dispatchers,
            [
                "EXTI0", "EXTI1", "EXTI2", "EXTI3", "CAN1_TX", "CAN2_TX", "CAN1_RX0", "CAN1_RX1"
            ]
        );
    }

    #[test]
    fn physical_actuator_is_sole_adapter_and_disabled_by_default() {
        let resolved = resolve(&APP_COMPOSITION).unwrap();
        let actuator = resolved
            .tasks
            .iter()
            .find(|task| task.id == "actuator_output")
            .unwrap();

        assert_eq!(
            resolved
                .board
                .timers
                .iter()
                .map(|timer| timer.id)
                .collect::<Vec<_>>(),
            ["tim2", "tim4", "tim5", "tim6"]
        );
        assert_eq!(resolved.periodic_controls.len(), 1);
        assert_eq!(resolved.periodic_controls[0].hardware.id, "tim4");
        assert!(
            resolved
                .gpio_resources
                .iter()
                .all(|resource| resource.owner_task != "actuator_output")
        );
        assert_eq!(actuator.local.len(), 4);
        assert_eq!(actuator.local[0].target, "motor_cmd_consumer");
        assert_eq!(actuator.shared.len(), 1);
        assert_eq!(actuator.shared[0].target, "dshot_motors");
        assert!(actuator.owner_component.is_none());
        assert_eq!(resolved.dshot_actuators.len(), 1);
        assert!(!resolved.dshot_actuators[0].declaration.output_enabled);
        assert_eq!(resolved.dshot_actuators[0].hardware.id, "foxeer_dshot");
        assert!(
            resolved
                .tasks
                .iter()
                .filter(|task| {
                    task.shared
                        .iter()
                        .any(|binding| binding.target == "dshot_motors")
                        && task.id == "actuator_output"
                })
                .count()
                == 1
        );
    }

    #[test]
    fn ambiguous_timer_interrupt_binding_is_rejected() {
        const CONTRACT: TaskContract = TaskContract {
            id: "competing_timer_task",
            source: TaskSource::new("test.rs", "competing_timer_task"),
            local: &[],
            shared: &[],
            config: &[],
            spawns: &[],
        };
        const TASK: TaskDeclaration = TaskDeclaration::interrupt(
            "competing_timer_task",
            &CONTRACT,
            stm32f4xx_hal::pac::Interrupt::TIM4,
        )
        .priority(1);
        const AMBIGUOUS: AppComposition = AppComposition {
            board: &board::BOARD,
            platform_config: empty_platform_config,
            monotonic: MonotonicDeclaration::systick(1_000),
            init_delay: None,
            components: &[ComponentDeclaration::periodic_control(TIMING_ONLY_CONTROL)],
            task_state: &[],
            shared_resources: &[],
            safety_channels: &[],
            tasks: &[TASK],
            init_spawns: &[],
        };

        let error = resolve(&AMBIGUOUS).unwrap_err().to_string();
        assert!(error.contains("both bind interrupt `TIM4`"));
    }

    #[test]
    fn state_cadence_must_match_the_periodic_control_declaration() {
        const SCHEMA: TaskStateSchema = TaskStateSchema {
            id: "cadence_state",
            version: 1,
            requirements: &[TaskStateRequirement::new(
                TaskStateRole::SamplesPerControlLoop,
                TaskStateType::U32,
            )],
        };
        const STATE: TaskStateDeclaration = SCHEMA.declare(
            "control_loop",
            &[TaskStateField::new(
                "samples_per_control_loop",
                TaskStateRole::SamplesPerControlLoop,
                TaskStateRecipe::U32(3),
            )],
        );
        const INVALID: AppComposition = AppComposition {
            board: &board::BOARD,
            platform_config: empty_platform_config,
            monotonic: MonotonicDeclaration::systick(1_000),
            init_delay: None,
            components: &[ComponentDeclaration::periodic_control(TIMING_ONLY_CONTROL)],
            task_state: &[STATE],
            shared_resources: &[],
            safety_channels: &[],
            tasks: &[],
            init_spawns: &[],
        };

        let error = resolve(&INVALID).unwrap_err().to_string();
        assert!(error.contains("supplies 3 scheduler ticks"));
        assert!(error.contains("resolves to 2"));
    }

    #[test]
    fn task_state_requires_a_resolved_owner_task() {
        const SCHEMA: TaskStateSchema = TaskStateSchema {
            id: "sequence_state",
            version: 1,
            requirements: &[TaskStateRequirement::new(
                TaskStateRole::ImuLastSequence,
                TaskStateType::U32,
            )],
        };
        const STATE: TaskStateDeclaration = SCHEMA.declare(
            "missing_task",
            &[TaskStateField::new(
                "imu_last_sequence",
                TaskStateRole::ImuLastSequence,
                TaskStateRecipe::U32(0),
            )],
        );
        const INVALID: AppComposition = AppComposition {
            board: &board::BOARD,
            platform_config: empty_platform_config,
            monotonic: MonotonicDeclaration::systick(1_000),
            init_delay: None,
            components: &[],
            task_state: &[STATE],
            shared_resources: &[],
            safety_channels: &[],
            tasks: &[],
            init_spawns: &[],
        };

        let error = resolve(&INVALID).unwrap_err().to_string();
        assert!(error.contains("targets missing task `missing_task`"));
    }
}
