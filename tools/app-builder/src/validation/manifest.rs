use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

/// Application-owned feature selection and behavior.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationManifest {
    pub schema_version: u64,
    pub application: Application,
    pub features: BTreeMap<String, FeatureConfig>,
}

/// BSP-owned physical hardware facts and available resources.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BspManifest {
    pub schema_version: u64,
    pub bsp: Bsp,
    pub clock: Clock,
    pub resources: Resources,
}

/// Fully resolved input set used by validation, rendering, and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Manifest {
    pub application_schema_version: u64,
    pub bsp_schema_version: u64,
    pub application: Application,
    pub bsp: Bsp,
    pub clock: Clock,
    pub resources: Resources,
    pub features: BTreeMap<String, FeatureConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Application {
    pub name: String,
    pub bsp: String,
    pub feature_order: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bsp {
    pub id: String,
    pub mcu: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub source: String,
    pub source_frequency_hz: u64,
    pub system_clock_hz: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub leds: BTreeMap<String, Led>,
    pub buttons: BTreeMap<String, Button>,
    #[serde(default)]
    pub serial: BTreeMap<String, SerialEndpoint>,
    #[serde(default)]
    pub dma: BTreeMap<String, DmaRoute>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FeatureConfig {
    BlinkLed(BlinkLedFeature),
    ButtonToggle(ButtonToggleFeature),
    ButtonArmToggle(ButtonArmToggleFeature),
    OsdDisplayPort(OsdDisplayPortFeature),
}

impl FeatureConfig {
    pub fn implementation(&self) -> &str {
        match self {
            Self::BlinkLed(feature) => &feature.implementation,
            Self::ButtonToggle(feature) => &feature.implementation,
            Self::ButtonArmToggle(feature) => &feature.implementation,
            Self::OsdDisplayPort(feature) => &feature.implementation,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlinkLedFeature {
    pub implementation: String,
    pub task_name: String,
    pub task_priority: u64,
    pub toggle_period_ms: u64,
    pub led: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonToggleFeature {
    pub implementation: String,
    pub task_name: String,
    pub task_priority: u64,
    pub debounce_ms: u64,
    pub button: String,
    pub led: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonArmToggleFeature {
    pub implementation: String,
    pub task_name: String,
    pub task_priority: u64,
    pub debounce_ms: u64,
    pub button: String,
    pub osd: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OsdDisplayPortFeature {
    pub implementation: String,
    pub endpoint: String,
    pub rx_dma: String,
    pub tx_dma: String,
    pub rx_buffer_size: u64,
    pub rx_buffer_count: u64,
    pub rx_queue_capacity: u64,
    pub tx_buffer_size: u64,
    pub tx_queue_capacity: u64,
    pub refresh_period_ms: u64,
    pub refresh_task_priority: u64,
    pub uart_irq_priority: u64,
    pub rx_dma_irq_priority: u64,
    pub tx_dma_irq_priority: u64,
    pub tx_worker_priority: u64,
    pub task_name: String,
    pub task_priority: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Led {
    pub pin: String,
    pub default: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    pub pin: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SerialEndpoint {
    pub peripheral: String,
    pub tx_pin: String,
    pub rx_pin: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DmaRoute {
    pub controller: String,
    pub stream: u8,
    pub channel: u8,
    pub interrupt: String,
}

impl Manifest {
    /// Returns a declared feature by the same key used in `feature_order`.
    pub fn feature(&self, name: &str) -> Option<&FeatureConfig> {
        self.features.get(name)
    }

    pub fn blink_led_feature(&self) -> Option<&BlinkLedFeature> {
        match self.features.get("blink_led") {
            Some(FeatureConfig::BlinkLed(feature)) => Some(feature),
            _ => None,
        }
    }

    pub fn blink_led_feature_mut(&mut self) -> Option<&mut BlinkLedFeature> {
        match self.features.get_mut("blink_led") {
            Some(FeatureConfig::BlinkLed(feature)) => Some(feature),
            _ => None,
        }
    }

    pub fn button_toggle_feature_mut(&mut self) -> Option<&mut ButtonToggleFeature> {
        match self.features.get_mut("button_toggle") {
            Some(FeatureConfig::ButtonToggle(feature)) => Some(feature),
            _ => None,
        }
    }

    pub fn button_toggle_feature(&self) -> Option<&ButtonToggleFeature> {
        match self.features.get("button_toggle") {
            Some(FeatureConfig::ButtonToggle(feature)) => Some(feature),
            _ => None,
        }
    }

    pub fn led(&self, name: &str) -> Option<&Led> {
        self.resources.leds.get(name)
    }

    pub fn button(&self, name: &str) -> Option<&Button> {
        self.resources.buttons.get(name)
    }

    pub fn serial_endpoint(&self, name: &str) -> Option<&SerialEndpoint> {
        self.resources.serial.get(name)
    }

    pub fn dma_route(&self, name: &str) -> Option<&DmaRoute> {
        self.resources.dma.get(name)
    }
}

/// Reads and strictly resolves one application manifest and one BSP manifest.
pub fn load(application_path: &Path, bsp_path: &Path) -> Result<Manifest> {
    let application_source = fs::read_to_string(application_path).with_context(|| {
        format!(
            "failed to read application manifest `{}`",
            application_path.display()
        )
    })?;
    let bsp_source = fs::read_to_string(bsp_path)
        .with_context(|| format!("failed to read BSP manifest `{}`", bsp_path.display()))?;

    parse_with_sources(
        &application_source,
        &application_path.display().to_string(),
        &bsp_source,
        &bsp_path.display().to_string(),
    )
}

/// Reads only the application document, for commands that must discover its
/// referenced BSP before resolving the complete manifest pair.
pub fn load_application(path: &Path) -> Result<ApplicationManifest> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read application manifest `{}`", path.display()))?;
    parse_document(&source, &path.display().to_string(), "application")
}

/// Strictly resolves manifest sources supplied by a caller or test.
pub fn parse(application_source: &str, bsp_source: &str) -> Result<Manifest> {
    parse_with_sources(
        application_source,
        "<application-memory>",
        bsp_source,
        "<bsp-memory>",
    )
}

fn parse_with_sources(
    application_source: &str,
    application_source_name: &str,
    bsp_source: &str,
    bsp_source_name: &str,
) -> Result<Manifest> {
    let application: ApplicationManifest =
        parse_document(application_source, application_source_name, "application")?;
    let bsp: BspManifest = parse_document(bsp_source, bsp_source_name, "BSP")?;

    Ok(Manifest {
        application_schema_version: application.schema_version,
        bsp_schema_version: bsp.schema_version,
        application: application.application,
        bsp: bsp.bsp,
        clock: bsp.clock,
        resources: bsp.resources,
        features: application.features,
    })
}

fn parse_document<T>(source: &str, source_name: &str, kind: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let deserializer = toml::Deserializer::new(source);
    serde_path_to_error::deserialize(deserializer).map_err(|error| {
        let path = complete_error_path(&error);
        anyhow!(
            "{kind} manifest `{source_name}` is invalid at `{path}`: {}",
            error.inner()
        )
    })
}

fn complete_error_path(error: &serde_path_to_error::Error<toml::de::Error>) -> String {
    let path = error.path().to_string();
    let field = missing_or_unknown_field(error.inner().message());

    match (path.as_str(), field) {
        ("" | ".", Some(field)) => field.to_owned(),
        ("" | ".", None) => "<document>".to_owned(),
        (path, Some(field)) if path.rsplit('.').next() != Some(field) => {
            format!("{path}.{field}")
        }
        (path, _) => path.to_owned(),
    }
}

fn missing_or_unknown_field(message: &str) -> Option<&str> {
    ["missing field `", "unknown field `"]
        .into_iter()
        .find_map(|prefix| {
            let rest = message.strip_prefix(prefix)?;
            rest.split_once('`').map(|(field, _)| field)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLICATION: &str = include_str!("../../applications/nucleo-f401re-blinky.toml");
    const BSP: &str = include_str!("../../bsp/nucleo-f401re.toml");

    #[test]
    fn valid_nucleo_manifests_resolve() {
        let manifest = parse(APPLICATION, BSP).expect("checked-in manifests should deserialize");

        assert_eq!(manifest.application_schema_version, 6);
        assert_eq!(manifest.bsp_schema_version, 6);
        assert_eq!(manifest.application.name, "nucleo-f401re-blinky");
        assert_eq!(manifest.application.bsp, "nucleo-f401re");
        assert_eq!(manifest.bsp.mcu, "STM32F401");
        assert_eq!(
            manifest.application.feature_order,
            ["blink_led", "button_toggle"]
        );
        assert_eq!(manifest.blink_led_feature().unwrap().led, "led2");
        assert_eq!(
            manifest.button_toggle_feature().unwrap().button,
            "user_button"
        );
        assert_eq!(manifest.resources.leds["led2"].pin, "PA5");
        assert_eq!(manifest.resources.leds["led2"].default, "high");
    }

    #[test]
    fn every_nested_bsp_field_is_required_with_a_complete_path() {
        let invalid = BSP.replace("interrupt = \"DMA2_STREAM5\"\n", "");
        let error = parse(APPLICATION, &invalid).expect_err("interrupt must be mandatory");

        assert!(
            error
                .to_string()
                .contains("resources.dma.usart1_rx.interrupt"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn unknown_nested_bsp_fields_are_rejected_with_a_complete_path() {
        let invalid = BSP.replace(
            "default = \"high\"",
            "default = \"high\"\nautomatic_reload = true",
        );
        let error = parse(APPLICATION, &invalid).expect_err("unknown fields must be rejected");

        assert!(
            error
                .to_string()
                .contains("resources.leds.led2.automatic_reload"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn application_type_errors_include_the_leaf_path() {
        let invalid = APPLICATION.replace("led = \"led2\"", "led = 5");
        let error = parse(&invalid, BSP).expect_err("resource reference must be a string");

        assert!(
            error.to_string().contains("features.blink_led"),
            "unexpected error: {error:#}"
        );
    }
}
