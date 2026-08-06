//! Deterministic expansion and resolution of an [`AppComposition`].
//!
//! Resolution assigns names to component-owned tasks and resources, resolves
//! hardware interrupt vectors, and rejects collisions before source rendering.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};

use crate::{
    hardware_definitions::stm32f4::{
        board_declaration::{BoardDeclaration, SerialHardwareDeclaration},
        dma_route::{DmaController, DmaRoute, DmaStream},
        gpio::GpioMode,
        hw_endpoint::serial_endpoint::{
            SerialEndpointDeclaration, SerialEndpointDirection, SerialEndpointResourceActivation,
            SerialEndpointResourceOwnership, SerialEndpointResourceRole,
            SerialEndpointResourceVisibility,
        },
        pins::PinId,
        serial::SerialPeripheral,
    },
    rtic::{
        component::ComponentDeclaration,
        composition::{self, AppComposition, SharedValue, TaskDeclaration, TaskTrigger},
        task::{ConfigValue, SpawnArgument, TaskContract},
        timing::MonotonicDeclaration,
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

/// One board GPIO selected by a task-local binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedGpioResource {
    /// Board declaration for the selected GPIO.
    pub hardware: &'static crate::hardware_definitions::stm32f4::gpio::GpioHardwareDeclaration,

    /// Concrete task that owns this local resource.
    pub owner_task: &'static str,
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

    /// Selected application monotonic.
    pub monotonic: MonotonicDeclaration,

    /// Application-owned shared values.
    pub shared_resources: Vec<ResolvedSharedResource>,

    /// Board GPIOs consumed by standalone tasks.
    pub gpio_resources: Vec<ResolvedGpioResource>,

    /// Expanded serial endpoint instances.
    pub serial_endpoints: Vec<ResolvedSerialEndpoint>,

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
            ComponentDeclaration::SerialEndpoint(declaration) => {
                let endpoint = resolve_serial_endpoint(composition.board, *declaration)?;
                let endpoint_tasks = expand_serial_endpoint_tasks(&endpoint)?;
                if matches!(
                    declaration.direction,
                    SerialEndpointDirection::Bidirectional
                ) {
                    init_spawns.push(format!("{}_tx_worker", declaration.id));
                }
                tasks.extend(endpoint_tasks);
                serial_endpoints.push(endpoint);
            }
        }
    }

    validate_unique_generated_ids(
        &shared_resources,
        &gpio_resources,
        &serial_endpoints,
        &tasks,
    )?;
    validate_interrupt_ownership(&tasks)?;
    let dispatchers = select_dispatchers(&tasks)?;

    Ok(ResolvedApp {
        board: composition.board,
        monotonic: composition.monotonic,
        shared_resources,
        gpio_resources,
        serial_endpoints,
        tasks,
        init_spawns,
        dispatchers,
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
            let hardware = board.gpio(binding.target()).with_context(|| {
                format!(
                    "task `{}` local resource `{}` disappeared after composition validation",
                    task.id,
                    binding.target()
                )
            })?;
            gpio_resources.push(ResolvedGpioResource {
                hardware,
                owner_task: task.id,
            });
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
    let hardware = board.serial(declaration.hardware_id).with_context(|| {
        format!(
            "serial endpoint `{}` hardware `{}` disappeared after validation",
            declaration.id, declaration.hardware_id
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
    if matches!(
        declaration.direction,
        SerialEndpointDirection::Bidirectional
    ) && declaration.tx_queue_depth != 16
    {
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
        .filter(|resource| {
            matches!(
                resource.activation,
                SerialEndpointResourceActivation::Always
            ) || matches!(
                declaration.direction,
                SerialEndpointDirection::Bidirectional
            )
        })
        .map(|resource| ResolvedEndpointResource {
            id: format!(
                "{}_{}",
                declaration.id,
                endpoint_resource_suffix(resource.role)
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

fn endpoint_resource_suffix(role: SerialEndpointResourceRole) -> &'static str {
    match role {
        SerialEndpointResourceRole::RxService => "rx",
        SerialEndpointResourceRole::RxParser => "rx_parser",
        SerialEndpointResourceRole::RxBuffers => "rx_buffers",
        SerialEndpointResourceRole::RxFreeQueue => "rx_free_queue",
        SerialEndpointResourceRole::RxFilledQueue => "rx_filled_queue",
        SerialEndpointResourceRole::RxChannel => "rx_channel",
        SerialEndpointResourceRole::RxProducer => "rx_producer",
        SerialEndpointResourceRole::RxReader => "rx_reader",
        SerialEndpointResourceRole::RxDiscontinuities => "rx_discontinuities",
        SerialEndpointResourceRole::TxDma => "tx_dma",
        SerialEndpointResourceRole::TxBuffer => "tx_buffer",
        SerialEndpointResourceRole::TxChannel => "tx_channel",
        SerialEndpointResourceRole::TxWriter => "tx_writer",
        SerialEndpointResourceRole::TxOwner => "tx_owner",
        SerialEndpointResourceRole::TxCompletion => "tx_completion",
    }
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
    let mut tasks = vec![
        component_task(
            declaration.id,
            format!("{}_rx_idle_irq", declaration.id),
            declaration.definition.tasks.peripheral_irq,
            ResolvedTaskTrigger::Interrupt(serial_interrupt(endpoint.hardware.port.peripheral)),
            declaration.interrupt_priority,
            vec![],
            vec![("rx", rx)],
        )?,
        component_task(
            declaration.id,
            format!("{}_rx_dma_irq", declaration.id),
            declaration.definition.tasks.rx_dma_irq,
            ResolvedTaskTrigger::Interrupt(dma_interrupt(rx_dma)),
            declaration.interrupt_priority,
            vec![],
            vec![("rx", rx)],
        )?,
    ];

    if matches!(
        declaration.direction,
        SerialEndpointDirection::Bidirectional
    ) {
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
                .context("validated bidirectional endpoint lost its worker priority")?,
            vec![("owner", owner)],
            vec![("tx_dma", tx_dma)],
        )?);
    }

    Ok(tasks)
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
    validate_contract_resources(&id, "local", contract.local, &local)?;
    validate_contract_resources(&id, "shared", contract.shared, &shared)?;
    if !contract.config.is_empty() || !contract.spawns.is_empty() {
        bail!("component task `{id}` currently requires unsupported config or spawn bindings");
    }
    Ok(ResolvedTask {
        id,
        contract,
        trigger,
        priority,
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
        config: Vec::new(),
        spawns: Vec::new(),
        owner_component: Some(owner),
    })
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

fn validate_unique_generated_ids(
    shared: &[ResolvedSharedResource],
    gpio: &[ResolvedGpioResource],
    endpoints: &[ResolvedSerialEndpoint],
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
    for resource in gpio {
        insert_unique(&mut resource_ids, resource.hardware.id, "board GPIO")?;
    }
    for endpoint in endpoints {
        for resource in &endpoint.resources {
            insert_unique(&mut resource_ids, &resource.id, "serial endpoint resource")?;
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
    let dispatchers = ["EXTI0", "EXTI1", "EXTI2", "EXTI3", "EXTI4"]
        .into_iter()
        .filter(|candidate| !used_interrupts.contains(candidate))
        .take(count)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if dispatchers.len() != count {
        bail!(
            "STM32F4 target needs {count} RTIC dispatchers but EXTI0 through EXTI4 are exhausted"
        );
    }
    Ok(dispatchers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::app_composition::APP_COMPOSITION;

    #[test]
    fn current_composition_expands_serial_endpoint_tasks_and_resources() {
        let resolved = resolve(&APP_COMPOSITION).unwrap();

        assert!(
            resolved
                .tasks
                .iter()
                .any(|task| task.id == "osd_uart_tx_worker")
        );
        assert!(resolved.init_spawns.contains(&"blink_led".to_owned()));
        assert!(
            resolved
                .init_spawns
                .contains(&"osd_uart_tx_worker".to_owned())
        );
        assert_eq!(resolved.serial_endpoints.len(), 1);
        assert_eq!(
            resolved.serial_endpoints[0]
                .resource_id(SerialEndpointResourceRole::TxDma)
                .unwrap(),
            "osd_uart_tx_dma"
        );
    }

    #[test]
    fn current_interrupts_leave_two_dispatcher_priorities() {
        let resolved = resolve(&APP_COMPOSITION).unwrap();
        assert_eq!(resolved.dispatchers, ["EXTI0", "EXTI1"]);
    }
}
