use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::manifest::{FeatureConfig, Manifest};

const CONTRACT_DIRECTORY: &str = "architecture-contracts";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicationSafetyScope {
    Validation,
    BenchInhibited,
    Flight,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InteractionKind {
    Stream,
    Snapshot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SafetyClass {
    SafetyCritical,
    SafetyRelated,
    NonCritical,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportKind {
    SpscQueue,
    MpscQueue,
    LatestValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverflowPolicy {
    RejectNewAndRecordFault,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WakeupPolicy {
    AwaitReceiver,
    None,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransportContract {
    pub id: String,
    pub interaction: InteractionKind,
    pub safety: SafetyClass,
    pub kind: TransportKind,
    pub producers: Vec<String>,
    pub consumers: Vec<String>,
    pub capacity: Option<u32>,
    pub overflow: Option<OverflowPolicy>,
    pub wakeup: WakeupPolicy,
    pub faults: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskExecutionKind {
    HardwareRunToCompletion,
    AsyncDivergentConsumer,
    AsyncPeriodic,
    AsyncDelayedOneShot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PeriodicMode {
    FixedRate,
    FixedDelay,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MissedReleasePolicy {
    SkipToNext,
    RunOnceAndRebase,
    RecordFaultAndContinue,
    RecordFaultAndStop,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContract {
    pub id: String,
    pub execution: TaskExecutionKind,
    #[serde(default)]
    pub receive_transports: Vec<String>,
    pub period_ms: Option<u64>,
    pub delay_ms: Option<u64>,
    pub periodic_mode: Option<PeriodicMode>,
    pub missed_release: Option<MissedReleasePolicy>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendMechanism {
    Monotonic,
    PhysicalEndpoint,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypedRecipeSlot {
    pub name: String,
    pub rust_type: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PhysicalClaimKind {
    CorePeripheral,
    Peripheral,
    Pin,
    DmaRoute,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalClaimContract {
    pub id: String,
    pub kind: PhysicalClaimKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BackendRecipeContract {
    pub id: String,
    pub version: u32,
    pub mechanism: BackendMechanism,
    pub inputs: Vec<TypedRecipeSlot>,
    pub outputs: Vec<TypedRecipeSlot>,
    pub physical_claims: Vec<PhysicalClaimContract>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FaultSeverity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArmingEffect {
    None,
    Inhibit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FaultContract {
    pub id: String,
    pub severity: FaultSeverity,
    pub latching: bool,
    pub arming_effect: ArmingEffect,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildFailurePolicy {
    RejectCompilation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootInitializationFailurePolicy {
    PanicHalt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArchitectureContract {
    pub schema_version: u64,
    pub application_id: String,
    pub safety_scope: ApplicationSafetyScope,
    pub actuator_owner: Option<String>,
    pub build_failure_policy: BuildFailurePolicy,
    pub boot_initialization_failure_policy: BootInitializationFailurePolicy,
    pub transports: Vec<TransportContract>,
    pub tasks: Vec<TaskContract>,
    pub backend_recipes: Vec<BackendRecipeContract>,
    pub faults: Vec<FaultContract>,
}

pub fn validate_for_manifest(repository_root: &Path, manifest: &Manifest) -> Result<()> {
    if !matches!(
        manifest.application.name.as_str(),
        "nucleo-f401re-blinky" | "nucleo-f401re-osd"
    ) {
        return Ok(());
    }
    let path = repository_root
        .join(CONTRACT_DIRECTORY)
        .join(format!("{}.toml", manifest.application.name));
    let source = fs::read_to_string(&path)
        .with_context(|| format!("read architecture contract {}", path.display()))?;
    let contract: ArchitectureContract = toml::from_str(&source)
        .with_context(|| format!("parse architecture contract {}", path.display()))?;
    contract.validate(manifest)
}

impl ArchitectureContract {
    pub fn validate(&self, manifest: &Manifest) -> Result<()> {
        if self.schema_version != 1 {
            bail!(
                "unsupported architecture contract schema_version {}; expected 1",
                self.schema_version
            );
        }
        if self.application_id != manifest.application.name {
            bail!(
                "architecture contract application_id `{}` does not match manifest `{}`",
                self.application_id,
                manifest.application.name
            );
        }
        if self.safety_scope == ApplicationSafetyScope::Flight {
            if self.actuator_owner.is_none() {
                bail!("flight architecture contract requires exactly one actuator owner");
            }
        } else if self.actuator_owner.is_some() {
            bail!("NUCLEO validation/bench contract must not declare an actuator owner");
        }

        require_sorted_unique(
            self.transports.iter().map(|value| value.id.as_str()),
            "transport",
        )?;
        require_sorted_unique(self.tasks.iter().map(|value| value.id.as_str()), "task")?;
        require_sorted_unique(
            self.backend_recipes.iter().map(|value| value.id.as_str()),
            "backend recipe",
        )?;
        require_sorted_unique(self.faults.iter().map(|value| value.id.as_str()), "fault")?;

        let transport_ids: BTreeSet<_> = self
            .transports
            .iter()
            .map(|value| value.id.as_str())
            .collect();
        let task_ids: BTreeSet<_> = self.tasks.iter().map(|value| value.id.as_str()).collect();
        let fault_ids: BTreeSet<_> = self.faults.iter().map(|value| value.id.as_str()).collect();
        for transport in &self.transports {
            transport.validate()?;
            for endpoint in transport.producers.iter().chain(&transport.consumers) {
                if !task_ids.contains(endpoint.as_str()) {
                    bail!(
                        "transport `{}` references unknown task endpoint `{endpoint}`",
                        transport.id
                    );
                }
            }
            for fault in &transport.faults {
                if !fault_ids.contains(fault.as_str()) {
                    bail!(
                        "transport `{}` references unknown fault `{fault}`",
                        transport.id
                    );
                }
            }
        }
        for task in &self.tasks {
            task.validate(&transport_ids)?;
            for transport_id in &task.receive_transports {
                let transport = self
                    .transports
                    .iter()
                    .find(|transport| transport.id == *transport_id)
                    .expect("task validation checked transport existence");
                if !transport.consumers.contains(&task.id) {
                    bail!(
                        "divergent task `{}` receives `{transport_id}` but is not its declared consumer",
                        task.id
                    );
                }
            }
        }
        for recipe in &self.backend_recipes {
            if recipe.version == 0 || recipe.inputs.is_empty() || recipe.outputs.is_empty() {
                bail!(
                    "backend recipe `{}` requires a nonzero version and typed inputs/outputs",
                    recipe.id
                );
            }
            require_sorted_unique(
                recipe.inputs.iter().map(|slot| slot.name.as_str()),
                "recipe input",
            )?;
            require_sorted_unique(
                recipe.outputs.iter().map(|slot| slot.name.as_str()),
                "recipe output",
            )?;
            require_sorted_unique(
                recipe.physical_claims.iter().map(|claim| claim.id.as_str()),
                "recipe physical claim",
            )?;
            for slot in recipe.inputs.iter().chain(&recipe.outputs) {
                syn::parse_str::<syn::Type>(&slot.rust_type).with_context(|| {
                    format!(
                        "backend recipe `{}` slot `{}` has invalid Rust type `{}`",
                        recipe.id, slot.name, slot.rust_type
                    )
                })?;
            }
        }

        self.validate_nucleo_shape()?;
        self.validate_nucleo_manifest_values(manifest)
    }

    fn validate_nucleo_shape(&self) -> Result<()> {
        let tasks: BTreeSet<_> = self.tasks.iter().map(|value| value.id.as_str()).collect();
        match self.application_id.as_str() {
            "nucleo-f401re-blinky" => {
                require_exact_members(
                    &tasks,
                    &["blink-led", "button-debounce", "button-exti"],
                    "NUCLEO blinky tasks",
                )?;
            }
            "nucleo-f401re-osd" => {
                require_exact_members(
                    &tasks,
                    &[
                        "button-arm-debounce",
                        "button-arm-exti",
                        "osd-consumer",
                        "osd-refresh",
                        "usart1-rx-dma",
                        "usart1-rx-idle",
                        "usart1-tx-dma",
                        "usart1-tx-worker",
                    ],
                    "NUCLEO OSD tasks",
                )?;
                let transports: BTreeSet<_> = self
                    .transports
                    .iter()
                    .map(|value| value.id.as_str())
                    .collect();
                require_exact_members(
                    &transports,
                    &[
                        "osd-telemetry",
                        "osd-to-usart1-tx",
                        "uart1-rx-and-refresh-to-osd",
                        "usart1-tx-completion",
                    ],
                    "NUCLEO OSD transports",
                )?;
            }
            _ => bail!("unsupported NUCLEO architecture contract"),
        }
        Ok(())
    }

    fn validate_nucleo_manifest_values(&self, manifest: &Manifest) -> Result<()> {
        match self.application_id.as_str() {
            "nucleo-f401re-blinky" => {
                let blink = manifest
                    .blink_led_feature()
                    .context("NUCLEO blinky contract requires blink_led")?;
                let button = manifest
                    .button_toggle_feature()
                    .context("NUCLEO blinky contract requires button_toggle")?;
                self.require_task_period("blink-led", blink.toggle_period_ms)?;
                self.require_task_delay("button-debounce", button.debounce_ms)?;
            }
            "nucleo-f401re-osd" => {
                let osd = match manifest.feature("osd_displayport") {
                    Some(FeatureConfig::OsdDisplayPort(feature)) => feature,
                    _ => bail!("NUCLEO OSD contract requires osd_displayport"),
                };
                let button = match manifest.feature("button_arm_toggle") {
                    Some(FeatureConfig::ButtonArmToggle(feature)) => feature,
                    _ => bail!("NUCLEO OSD contract requires button_arm_toggle"),
                };
                self.require_task_period("osd-refresh", osd.refresh_period_ms)?;
                self.require_task_delay("button-arm-debounce", button.debounce_ms)?;
                self.require_transport_capacity(
                    "uart1-rx-and-refresh-to-osd",
                    osd.rx_queue_capacity,
                )?;
                self.require_transport_capacity("osd-to-usart1-tx", osd.tx_queue_capacity)?;
                self.require_transport_capacity("usart1-tx-completion", 1)?;
            }
            _ => bail!("unsupported NUCLEO architecture contract"),
        }
        Ok(())
    }

    fn require_task_period(&self, id: &str, expected: u64) -> Result<()> {
        let task = self
            .tasks
            .iter()
            .find(|task| task.id == id)
            .with_context(|| format!("architecture contract is missing task `{id}`"))?;
        if task.period_ms != Some(expected) {
            bail!(
                "task `{id}` period {:?} does not match manifest value {expected}",
                task.period_ms
            );
        }
        Ok(())
    }

    fn require_task_delay(&self, id: &str, expected: u64) -> Result<()> {
        let task = self
            .tasks
            .iter()
            .find(|task| task.id == id)
            .with_context(|| format!("architecture contract is missing task `{id}`"))?;
        if task.delay_ms != Some(expected) {
            bail!(
                "task `{id}` delay {:?} does not match manifest value {expected}",
                task.delay_ms
            );
        }
        Ok(())
    }

    fn require_transport_capacity(&self, id: &str, expected: u64) -> Result<()> {
        let transport = self
            .transports
            .iter()
            .find(|transport| transport.id == id)
            .with_context(|| format!("architecture contract is missing transport `{id}`"))?;
        if u64::from(transport.capacity.unwrap_or_default()) != expected {
            bail!(
                "transport `{id}` capacity {:?} does not match manifest value {expected}",
                transport.capacity
            );
        }
        Ok(())
    }
}

impl TransportContract {
    fn validate(&self) -> Result<()> {
        match self.kind {
            TransportKind::SpscQueue => {
                if self.producers.len() != 1 || self.consumers.len() != 1 {
                    bail!(
                        "SPSC transport `{}` requires one producer and one consumer",
                        self.id
                    );
                }
                self.require_bounded_queue_fields()?;
            }
            TransportKind::MpscQueue => {
                if self.producers.is_empty() || self.consumers.len() != 1 {
                    bail!(
                        "MPSC transport `{}` requires one-or-more producers and one consumer",
                        self.id
                    );
                }
                self.require_bounded_queue_fields()?;
            }
            TransportKind::LatestValue => {
                if self.producers.len() != 1 || self.consumers.is_empty() {
                    bail!(
                        "latest-value transport `{}` requires one writer and one-or-more readers",
                        self.id
                    );
                }
                if self.capacity.is_some()
                    || self.overflow.is_some()
                    || self.wakeup != WakeupPolicy::None
                    || !self.faults.is_empty()
                {
                    bail!(
                        "latest-value transport `{}` must not declare queue or wake-up semantics",
                        self.id
                    );
                }
            }
        }
        require_sorted_unique(
            self.producers.iter().map(String::as_str),
            "transport producer",
        )?;
        require_sorted_unique(
            self.consumers.iter().map(String::as_str),
            "transport consumer",
        )?;
        Ok(())
    }

    fn require_bounded_queue_fields(&self) -> Result<()> {
        if self.capacity == Some(0) || self.capacity.is_none() {
            bail!("queue transport `{}` requires a positive capacity", self.id);
        }
        if self.overflow.is_none()
            || self.wakeup != WakeupPolicy::AwaitReceiver
            || self.faults.is_empty()
        {
            bail!(
                "queue transport `{}` requires overflow, await-receiver wake-up, and typed faults",
                self.id
            );
        }
        require_sorted_unique(self.faults.iter().map(String::as_str), "transport fault")?;
        Ok(())
    }
}

impl TaskContract {
    fn validate(&self, transport_ids: &BTreeSet<&str>) -> Result<()> {
        match self.execution {
            TaskExecutionKind::HardwareRunToCompletion => {
                self.require_no_async_fields()?;
            }
            TaskExecutionKind::AsyncDivergentConsumer => {
                if self.receive_transports.is_empty() {
                    bail!(
                        "divergent consumer task `{}` requires one-or-more receive transports",
                        self.id
                    );
                }
                require_sorted_unique(
                    self.receive_transports.iter().map(String::as_str),
                    "task receive transport",
                )?;
                for transport in &self.receive_transports {
                    if !transport_ids.contains(transport.as_str()) {
                        bail!(
                            "task `{}` references unknown receive transport `{transport}`",
                            self.id
                        );
                    }
                }
                if self.period_ms.is_some()
                    || self.delay_ms.is_some()
                    || self.periodic_mode.is_some()
                    || self.missed_release.is_some()
                {
                    bail!(
                        "divergent consumer task `{}` has unrelated timing fields",
                        self.id
                    );
                }
            }
            TaskExecutionKind::AsyncPeriodic => {
                if self.period_ms == Some(0)
                    || self.period_ms.is_none()
                    || self.periodic_mode.is_none()
                    || self.missed_release.is_none()
                    || self.delay_ms.is_some()
                    || !self.receive_transports.is_empty()
                {
                    bail!("periodic task `{}` has an incomplete schedule", self.id);
                }
            }
            TaskExecutionKind::AsyncDelayedOneShot => {
                if self.delay_ms == Some(0)
                    || self.delay_ms.is_none()
                    || self.period_ms.is_some()
                    || self.periodic_mode.is_some()
                    || self.missed_release.is_some()
                    || !self.receive_transports.is_empty()
                {
                    bail!(
                        "delayed one-shot task `{}` has invalid timing fields",
                        self.id
                    );
                }
            }
        }
        Ok(())
    }

    fn require_no_async_fields(&self) -> Result<()> {
        if !self.receive_transports.is_empty()
            || self.period_ms.is_some()
            || self.delay_ms.is_some()
            || self.periodic_mode.is_some()
            || self.missed_release.is_some()
        {
            bail!(
                "hardware task `{}` must not declare async execution fields",
                self.id
            );
        }
        Ok(())
    }
}

fn require_sorted_unique<'a>(values: impl IntoIterator<Item = &'a str>, label: &str) -> Result<()> {
    let values: Vec<_> = values.into_iter().collect();
    let mut sorted = values.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if values != sorted {
        bail!("{label} IDs must be unique and in canonical lexical order");
    }
    Ok(())
}

fn require_exact_members(actual: &BTreeSet<&str>, required: &[&str], label: &str) -> Result<()> {
    let required: BTreeSet<_> = required.iter().copied().collect();
    if actual != &required {
        bail!(
            "{label} do not match the required set (actual: {}; required: {})",
            actual.iter().copied().collect::<Vec<_>>().join(", "),
            required.iter().copied().collect::<Vec<_>>().join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest;

    const BSP: &str = include_str!("../../bsp/nucleo-f401re.toml");
    const BLINKY: &str = include_str!("../../applications/nucleo-f401re-blinky.toml");
    const OSD: &str = include_str!("../../applications/nucleo-f401re-osd.toml");

    #[test]
    fn checked_nucleo_contracts_match_their_manifests() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for (application, source) in [("nucleo-f401re-blinky", BLINKY), ("nucleo-f401re-osd", OSD)]
        {
            let manifest = manifest::parse(source, BSP).unwrap();
            assert_eq!(manifest.application.name, application);
            validate_for_manifest(root, &manifest).unwrap();
        }
    }

    #[test]
    fn destructive_queue_fanout_is_rejected() {
        let transport = TransportContract {
            id: "bad-fanout".to_owned(),
            interaction: InteractionKind::Stream,
            safety: SafetyClass::NonCritical,
            kind: TransportKind::SpscQueue,
            producers: vec!["producer".to_owned()],
            consumers: vec!["consumer-a".to_owned(), "consumer-b".to_owned()],
            capacity: Some(4),
            overflow: Some(OverflowPolicy::RejectNewAndRecordFault),
            wakeup: WakeupPolicy::AwaitReceiver,
            faults: vec!["bad-fanout".to_owned()],
        };
        assert!(
            transport
                .validate()
                .unwrap_err()
                .to_string()
                .contains("one consumer")
        );
    }

    #[test]
    fn periodic_tasks_require_explicit_release_semantics() {
        let task = TaskContract {
            id: "periodic".to_owned(),
            execution: TaskExecutionKind::AsyncPeriodic,
            receive_transports: Vec::new(),
            period_ms: Some(100),
            delay_ms: None,
            periodic_mode: None,
            missed_release: None,
        };
        assert!(
            task.validate(&BTreeSet::new())
                .unwrap_err()
                .to_string()
                .contains("incomplete schedule")
        );
    }

    #[test]
    fn contract_timing_must_match_the_application_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut manifest = manifest::parse(BLINKY, BSP).unwrap();
        manifest.blink_led_feature_mut().unwrap().toggle_period_ms = 999;

        let error = validate_for_manifest(root, &manifest).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not match manifest value 999")
        );
    }
}
