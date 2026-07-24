use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

use crate::feature::{FeatureBundle, FeatureFragments};
use crate::manifest::{
    BlinkLedFeature, ButtonArmToggleFeature, ButtonToggleFeature, FeatureConfig, Manifest,
    OsdDisplayPortFeature,
};
use crate::validate;

const BLINK_LED_ID: &str = "blink_led";
const BLINK_LED_IMPLEMENTATION: &str = "stm32f4/blink-led";
const BUTTON_TOGGLE_ID: &str = "button_toggle";
const BUTTON_TOGGLE_IMPLEMENTATION: &str = "stm32f4/button-toggle-blink";
const OSD_ID: &str = "osd_displayport";
const OSD_IMPLEMENTATION: &str = "stm32f4/msp-displayport-usart1-dma";
const BUTTON_ARM_ID: &str = "button_arm_toggle";
const BUTTON_ARM_IMPLEMENTATION: &str = "stm32f4/button-arm-toggle";
const EXPECTED_PLACEHOLDERS: [&str; 6] = [
    "LED_RESOURCE_NAME",
    "LED_RESOURCE_TYPE",
    "LED_INIT_EXPRESSION",
    "BLINKER_ENABLED_NAME",
    "BLINK_PERIOD_MS",
    "TASK_PRIORITY",
];
const BUTTON_EXPECTED_PLACEHOLDERS: [&str; 11] = [
    "BUTTON_RESOURCE_NAME",
    "BUTTON_RESOURCE_TYPE",
    "BUTTON_INIT_EXPRESSION",
    "EXTI_RESOURCE_NAME",
    "EXTI_RESOURCE_TYPE",
    "EXTI_INIT_EXPRESSION",
    "BLINKER_ENABLED_NAME",
    "LED_RESOURCE_NAME",
    "BUTTON_INTERRUPT",
    "TASK_PRIORITY",
    "DEBOUNCE_MS",
];
const OSD_EXPECTED_PLACEHOLDERS: [&str; 20] = [
    "BUFFER_SIZE",
    "RX_QUEUE_CAPACITY",
    "TX_QUEUE_CAPACITY",
    "REFRESH_PERIOD_MS",
    "REFRESH_TASK_PRIORITY",
    "BAUD",
    "UART_IRQ_PRIORITY",
    "RX_DMA_IRQ_PRIORITY",
    "TX_DMA_IRQ_PRIORITY",
    "TX_WORKER_PRIORITY",
    "OSD_TASK_PRIORITY",
    "UART_INTERRUPT",
    "RX_DMA_INTERRUPT",
    "TX_DMA_INTERRUPT",
    "SERIAL_RESOURCE",
    "RX_DMA_RESOURCE",
    "TX_DMA_RESOURCE",
    "OSD_COMPONENT_RESOURCE",
    "OSD_OUTPUT_RESOURCE",
    "OSD_TELEMETRY_RESOURCE",
];
const BUTTON_ARM_EXPECTED_PLACEHOLDERS: [&str; 10] = [
    "BUTTON_RESOURCE_NAME",
    "BUTTON_RESOURCE_TYPE",
    "BUTTON_INIT_EXPRESSION",
    "EXTI_RESOURCE_NAME",
    "EXTI_RESOURCE_TYPE",
    "EXTI_INIT_EXPRESSION",
    "BUTTON_INTERRUPT",
    "TASK_PRIORITY",
    "DEBOUNCE_MS",
    "OSD_TELEMETRY_RESOURCE",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedClaims {
    pub symbols: BTreeSet<String>,
    pub required_symbols: BTreeSet<String>,
    pub resources: BTreeSet<String>,
    pub pins: BTreeSet<String>,
    pub peripherals: BTreeSet<String>,
    pub interrupts: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFeature {
    pub id: String,
    pub fragments: FeatureFragments,
    pub replacements: BTreeMap<String, String>,
    pub claims: ResolvedClaims,
    /// Hash of the bundle contents plus every resolved, injection-safe token.
    pub fingerprint: String,
}

/// Resolves a declared feature without selecting or substituting hardware.
///
/// The MVP deliberately supports one exact backend configuration. All string
/// values are matched against an allowlist before fixed Rust tokens are emitted.
pub fn resolve_feature(
    manifest: &Manifest,
    feature_name: &str,
    bundle: &FeatureBundle,
) -> Result<ResolvedFeature> {
    validate::validate_manifest(manifest)?;
    let feature = manifest.feature(feature_name).ok_or_else(|| {
        anyhow::anyhow!("feature `{feature_name}` is present in feature_order but not in features")
    })?;

    match feature {
        FeatureConfig::BlinkLed(feature) => {
            resolve_blink_feature(manifest, feature_name, feature, bundle)
        }
        FeatureConfig::ButtonToggle(feature) => {
            resolve_button_feature(manifest, feature_name, feature, bundle)
        }
        FeatureConfig::ButtonArmToggle(feature) => {
            resolve_button_arm_feature(manifest, feature_name, feature, bundle)
        }
        FeatureConfig::OsdDisplayPort(feature) => {
            resolve_osd_feature(manifest, feature_name, feature, bundle)
        }
    }
}

fn resolve_osd_feature(
    manifest: &Manifest,
    feature_name: &str,
    feature: &OsdDisplayPortFeature,
    bundle: &FeatureBundle,
) -> Result<ResolvedFeature> {
    require_str("feature id", feature_name, OSD_ID)?;
    require_str(
        "feature implementation",
        &feature.implementation,
        OSD_IMPLEMENTATION,
    )?;
    require_str(
        "loaded feature implementation",
        &bundle.implementation,
        OSD_IMPLEMENTATION,
    )?;
    require_str("feature metadata id", &bundle.metadata.id, OSD_ID)?;
    require_set(
        "required_placeholders",
        &bundle.metadata.required_placeholders,
        &OSD_EXPECTED_PLACEHOLDERS,
    )?;
    require_set(
        "claimed_symbols",
        &bundle.metadata.claimed_symbols,
        &[
            "usart1_rx_idle",
            "usart1_rx_dma",
            "usart1_tx_dma",
            "usart1_tx_kick",
            "osd_displayport",
            "osd_refresh_tick",
        ],
    )?;
    require_set(
        "claimed_resources",
        &bundle.metadata.claimed_resources,
        &[
            "SERIAL_RESOURCE",
            "RX_DMA_RESOURCE",
            "TX_DMA_RESOURCE",
            "OSD_COMPONENT_RESOURCE",
            "OSD_OUTPUT_RESOURCE",
            "OSD_TELEMETRY_RESOURCE",
        ],
    )?;
    require_set(
        "claimed_interrupts",
        &bundle.metadata.claimed_interrupts,
        &["UART_INTERRUPT", "RX_DMA_INTERRUPT", "TX_DMA_INTERRUPT"],
    )?;
    require_set("required_symbols", &bundle.metadata.required_symbols, &[])?;
    require_set("insertion_after", &bundle.metadata.insertion_after, &[])?;

    let serial = manifest.serial_endpoint(&feature.endpoint).unwrap();
    let rx_dma = manifest.dma_route(&feature.rx_dma).unwrap();
    let tx_dma = manifest.dma_route(&feature.tx_dma).unwrap();
    let replacements = BTreeMap::from([
        ("BUFFER_SIZE".to_owned(), feature.rx_buffer_size.to_string()),
        (
            "RX_QUEUE_CAPACITY".to_owned(),
            feature.rx_queue_capacity.to_string(),
        ),
        (
            "TX_QUEUE_CAPACITY".to_owned(),
            feature.tx_queue_capacity.to_string(),
        ),
        (
            "REFRESH_PERIOD_MS".to_owned(),
            feature.refresh_period_ms.to_string(),
        ),
        (
            "REFRESH_TASK_PRIORITY".to_owned(),
            feature.refresh_task_priority.to_string(),
        ),
        // Transitional static OSD binding. The boot router will derive this
        // from ActivePlatformConfig's msp_displayport assignment.
        ("BAUD".to_owned(), "115200".to_owned()),
        (
            "UART_IRQ_PRIORITY".to_owned(),
            feature.uart_irq_priority.to_string(),
        ),
        (
            "RX_DMA_IRQ_PRIORITY".to_owned(),
            feature.rx_dma_irq_priority.to_string(),
        ),
        (
            "TX_DMA_IRQ_PRIORITY".to_owned(),
            feature.tx_dma_irq_priority.to_string(),
        ),
        (
            "TX_WORKER_PRIORITY".to_owned(),
            feature.tx_worker_priority.to_string(),
        ),
        (
            "OSD_TASK_PRIORITY".to_owned(),
            feature.task_priority.to_string(),
        ),
        ("UART_INTERRUPT".to_owned(), serial.peripheral.clone()),
        ("RX_DMA_INTERRUPT".to_owned(), rx_dma.interrupt.clone()),
        ("TX_DMA_INTERRUPT".to_owned(), tx_dma.interrupt.clone()),
        ("SERIAL_RESOURCE".to_owned(), "osd_serial".to_owned()),
        ("RX_DMA_RESOURCE".to_owned(), "usart1_rx_dma".to_owned()),
        ("TX_DMA_RESOURCE".to_owned(), "usart1_tx_dma".to_owned()),
        (
            "OSD_COMPONENT_RESOURCE".to_owned(),
            "osd_component".to_owned(),
        ),
        ("OSD_OUTPUT_RESOURCE".to_owned(), "osd_output".to_owned()),
        (
            "OSD_TELEMETRY_RESOURCE".to_owned(),
            "osd_telemetry".to_owned(),
        ),
    ]);
    let claims = ResolvedClaims {
        symbols: bundle.metadata.claimed_symbols.iter().cloned().collect(),
        required_symbols: BTreeSet::new(),
        resources: [
            "osd_serial",
            "usart1_rx_dma",
            "usart1_tx_dma",
            "osd_component",
            "osd_output",
            "osd_telemetry",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        pins: [serial.tx_pin.clone(), serial.rx_pin.clone()]
            .into_iter()
            .collect(),
        peripherals: [serial.peripheral.clone(), rx_dma.controller.clone()]
            .into_iter()
            .collect(),
        interrupts: [
            serial.peripheral.clone(),
            rx_dma.interrupt.clone(),
            tx_dma.interrupt.clone(),
            "EXTI0".to_owned(),
            "EXTI1".to_owned(),
            "EXTI2".to_owned(),
        ]
        .into_iter()
        .collect(),
    };
    let fingerprint = resolved_fingerprint(&bundle.fingerprint, &replacements, &claims);
    Ok(ResolvedFeature {
        id: OSD_ID.to_owned(),
        fragments: bundle.fragments.clone(),
        replacements,
        claims,
        fingerprint,
    })
}

fn resolve_button_arm_feature(
    manifest: &Manifest,
    feature_name: &str,
    feature: &ButtonArmToggleFeature,
    bundle: &FeatureBundle,
) -> Result<ResolvedFeature> {
    require_str("feature id", feature_name, BUTTON_ARM_ID)?;
    require_str(
        "feature implementation",
        &feature.implementation,
        BUTTON_ARM_IMPLEMENTATION,
    )?;
    require_str(
        "loaded feature implementation",
        &bundle.implementation,
        BUTTON_ARM_IMPLEMENTATION,
    )?;
    require_str("feature metadata id", &bundle.metadata.id, BUTTON_ARM_ID)?;
    require_set(
        "required_placeholders",
        &bundle.metadata.required_placeholders,
        &BUTTON_ARM_EXPECTED_PLACEHOLDERS,
    )?;
    require_set(
        "claimed_symbols",
        &bundle.metadata.claimed_symbols,
        &["button_arm_toggle", "button_arm_toggle_debounce"],
    )?;
    require_set(
        "required_symbols",
        &bundle.metadata.required_symbols,
        &[OSD_ID],
    )?;
    require_set(
        "claimed_resources",
        &bundle.metadata.claimed_resources,
        &["BUTTON_RESOURCE_NAME", "EXTI_RESOURCE_NAME"],
    )?;
    require_set(
        "claimed_interrupts",
        &bundle.metadata.claimed_interrupts,
        &["BUTTON_INTERRUPT"],
    )?;
    require_set(
        "insertion_after",
        &bundle.metadata.insertion_after,
        &[OSD_ID],
    )?;

    let button = manifest.button(&feature.button).unwrap();
    let button_name = &feature.button;
    let exti_name = format!("{button_name}_exti");
    let replacements = BTreeMap::from([
        ("BUTTON_RESOURCE_NAME".to_owned(), button_name.clone()),
        ("BUTTON_RESOURCE_TYPE".to_owned(), "PC13<Input>".to_owned()),
        (
            "BUTTON_INIT_EXPRESSION".to_owned(),
            format!(
                "let mut {button_name} = Input::new(gpioc.pc13, Pull::Up);\n{button_name}.make_interrupt_source(&mut syscfg);\n{button_name}.trigger_on_edge(&mut cx.device.EXTI, Edge::Falling);\n{button_name}.enable_interrupt(&mut cx.device.EXTI);"
            ),
        ),
        ("EXTI_RESOURCE_NAME".to_owned(), exti_name.clone()),
        ("EXTI_RESOURCE_TYPE".to_owned(), "EXTI".to_owned()),
        (
            "EXTI_INIT_EXPRESSION".to_owned(),
            format!("let {exti_name} = cx.device.EXTI;"),
        ),
        ("BUTTON_INTERRUPT".to_owned(), "EXTI15_10".to_owned()),
        (
            "TASK_PRIORITY".to_owned(),
            feature.task_priority.to_string(),
        ),
        ("DEBOUNCE_MS".to_owned(), feature.debounce_ms.to_string()),
        (
            "OSD_TELEMETRY_RESOURCE".to_owned(),
            "osd_telemetry".to_owned(),
        ),
    ]);
    let claims = ResolvedClaims {
        symbols: bundle.metadata.claimed_symbols.iter().cloned().collect(),
        required_symbols: bundle.metadata.required_symbols.iter().cloned().collect(),
        resources: [button_name.clone(), exti_name].into_iter().collect(),
        pins: [button.pin.clone()].into_iter().collect(),
        peripherals: ["EXTI".to_owned()].into_iter().collect(),
        interrupts: ["EXTI15_10".to_owned()].into_iter().collect(),
    };
    let fingerprint = resolved_fingerprint(&bundle.fingerprint, &replacements, &claims);
    Ok(ResolvedFeature {
        id: BUTTON_ARM_ID.to_owned(),
        fragments: bundle.fragments.clone(),
        replacements,
        claims,
        fingerprint,
    })
}

fn resolve_blink_feature(
    manifest: &Manifest,
    feature_name: &str,
    feature: &BlinkLedFeature,
    bundle: &FeatureBundle,
) -> Result<ResolvedFeature> {
    require_str("feature id", feature_name, BLINK_LED_ID)?;

    if bundle.implementation != feature.implementation {
        bail!(
            "loaded feature implementation `{}` does not match `features.{feature_name}.implementation` `{}`",
            bundle.implementation,
            feature.implementation
        );
    }
    require_str(
        "feature implementation",
        &bundle.implementation,
        BLINK_LED_IMPLEMENTATION,
    )?;
    require_str("feature metadata id", &bundle.metadata.id, BLINK_LED_ID)?;
    require_metadata_contract(bundle)?;

    let led = manifest.led(&feature.led).ok_or_else(|| {
        anyhow::anyhow!(
            "feature `{feature_name}` references missing BSP LED resource `{}`",
            feature.led
        )
    })?;

    // Resource IDs have already passed the manifest validator. Re-parse them at
    // the Rust token boundary as defense in depth.
    validate_identifier(&feature.led, "features.blink_led.led")?;
    let led_name = &feature.led;
    let enabled_name = "blinker_enabled";
    let default_pin_state = match led.default.as_str() {
        "high" => "High",
        "low" => "Low",
        _ => unreachable!("manifest validation accepts only high or low"),
    };

    let mut replacements = BTreeMap::new();
    replacements.insert("LED_RESOURCE_NAME".to_owned(), led_name.clone());
    replacements.insert(
        "LED_RESOURCE_TYPE".to_owned(),
        "PA5<Output<PushPull>>".to_owned(),
    );
    replacements.insert(
        "LED_INIT_EXPRESSION".to_owned(),
        format!(
            "let mut {led_name} = gpioa\n    .pa5\n    .into_push_pull_output_in_state(PinState::{default_pin_state});\n{led_name}.set_internal_resistor(Pull::None);\n{led_name}.set_speed(Speed::Low);"
        ),
    );
    replacements.insert("BLINKER_ENABLED_NAME".to_owned(), enabled_name.to_owned());
    replacements.insert(
        "BLINK_PERIOD_MS".to_owned(),
        feature.toggle_period_ms.to_string(),
    );
    replacements.insert(
        "TASK_PRIORITY".to_owned(),
        feature.task_priority.to_string(),
    );

    let replacement_keys: BTreeSet<_> = replacements.keys().map(String::as_str).collect();
    let expected_keys: BTreeSet<_> = EXPECTED_PLACEHOLDERS.into_iter().collect();
    if replacement_keys != expected_keys {
        bail!("internal backend error: replacement keys do not match the feature contract");
    }

    let claims = ResolvedClaims {
        symbols: bundle.metadata.claimed_symbols.iter().cloned().collect(),
        required_symbols: bundle.metadata.required_symbols.iter().cloned().collect(),
        resources: [led_name.clone(), enabled_name.to_owned()]
            .into_iter()
            .collect(),
        pins: [led.pin.clone()].into_iter().collect(),
        peripherals: BTreeSet::new(),
        interrupts: ["EXTI0".to_owned()].into_iter().collect(),
    };
    let fingerprint = resolved_fingerprint(&bundle.fingerprint, &replacements, &claims);

    Ok(ResolvedFeature {
        id: BLINK_LED_ID.to_owned(),
        fragments: bundle.fragments.clone(),
        replacements,
        claims,
        fingerprint,
    })
}

fn resolve_button_feature(
    manifest: &Manifest,
    feature_name: &str,
    feature: &ButtonToggleFeature,
    bundle: &FeatureBundle,
) -> Result<ResolvedFeature> {
    require_str("feature id", feature_name, BUTTON_TOGGLE_ID)?;
    if bundle.implementation != feature.implementation {
        bail!(
            "loaded feature implementation `{}` does not match `features.{feature_name}.implementation` `{}`",
            bundle.implementation,
            feature.implementation
        );
    }
    require_str(
        "feature implementation",
        &bundle.implementation,
        BUTTON_TOGGLE_IMPLEMENTATION,
    )?;
    require_str("feature metadata id", &bundle.metadata.id, BUTTON_TOGGLE_ID)?;
    require_button_metadata_contract(bundle)?;

    let button = manifest.button(&feature.button).ok_or_else(|| {
        anyhow::anyhow!(
            "feature `{feature_name}` references missing BSP button resource `{}`",
            feature.button
        )
    })?;
    manifest.led(&feature.led).ok_or_else(|| {
        anyhow::anyhow!(
            "feature `{feature_name}` references missing BSP LED resource `{}`",
            feature.led
        )
    })?;
    let blink = manifest
        .blink_led_feature()
        .ok_or_else(|| anyhow::anyhow!("button_toggle requires blink_led configuration"))?;

    for (value, path) in [
        (&feature.button, "features.button_toggle.button"),
        (&feature.led, "features.button_toggle.led"),
    ] {
        validate_identifier(value, path)?;
    }

    let button_name = &feature.button;
    let exti_name = format!("{button_name}_exti");
    let enabled_name = "blinker_enabled";

    let replacements = BTreeMap::from([
        ("BUTTON_RESOURCE_NAME".to_owned(), button_name.clone()),
        ("BUTTON_RESOURCE_TYPE".to_owned(), "PC13<Input>".to_owned()),
        (
            "BUTTON_INIT_EXPRESSION".to_owned(),
            format!(
                "let mut {button_name} = Input::new(gpioc.pc13, Pull::Up);\n{button_name}.make_interrupt_source(&mut syscfg);\n{button_name}.trigger_on_edge(&mut cx.device.EXTI, Edge::Falling);\n{button_name}.enable_interrupt(&mut cx.device.EXTI);"
            ),
        ),
        ("EXTI_RESOURCE_NAME".to_owned(), exti_name.clone()),
        ("EXTI_RESOURCE_TYPE".to_owned(), "EXTI".to_owned()),
        (
            "EXTI_INIT_EXPRESSION".to_owned(),
            format!("let {exti_name} = cx.device.EXTI;"),
        ),
        ("BLINKER_ENABLED_NAME".to_owned(), enabled_name.to_owned()),
        ("LED_RESOURCE_NAME".to_owned(), feature.led.clone()),
        ("BUTTON_INTERRUPT".to_owned(), "EXTI15_10".to_owned()),
        (
            "TASK_PRIORITY".to_owned(),
            feature.task_priority.to_string(),
        ),
        ("DEBOUNCE_MS".to_owned(), feature.debounce_ms.to_string()),
    ]);
    let replacement_keys: BTreeSet<_> = replacements.keys().map(String::as_str).collect();
    let expected_keys: BTreeSet<_> = BUTTON_EXPECTED_PLACEHOLDERS.into_iter().collect();
    if replacement_keys != expected_keys {
        bail!("internal backend error: button replacement keys do not match the feature contract");
    }

    let claims = ResolvedClaims {
        symbols: bundle.metadata.claimed_symbols.iter().cloned().collect(),
        required_symbols: bundle.metadata.required_symbols.iter().cloned().collect(),
        resources: [button_name.clone(), exti_name].into_iter().collect(),
        pins: [button.pin.clone()].into_iter().collect(),
        peripherals: ["EXTI".to_owned()].into_iter().collect(),
        interrupts: ["EXTI15_10".to_owned(), "EXTI1".to_owned()]
            .into_iter()
            .collect(),
    };
    debug_assert_eq!(blink.led, feature.led);
    let fingerprint = resolved_fingerprint(&bundle.fingerprint, &replacements, &claims);
    Ok(ResolvedFeature {
        id: BUTTON_TOGGLE_ID.to_owned(),
        fragments: bundle.fragments.clone(),
        replacements,
        claims,
        fingerprint,
    })
}

pub fn resolve_blink_led(manifest: &Manifest, bundle: &FeatureBundle) -> Result<ResolvedFeature> {
    resolve_feature(manifest, BLINK_LED_ID, bundle)
}

/// Rejects collisions while preserving the caller's declared feature order.
pub fn validate_resolved_claims(features: &[ResolvedFeature]) -> Result<()> {
    let mut symbols = BTreeMap::new();
    let mut resources = BTreeMap::new();
    let mut pins = BTreeMap::new();
    let mut peripherals = BTreeMap::new();
    let mut interrupts = BTreeMap::new();

    for feature in features {
        record_claims(&mut symbols, "symbol", &feature.id, &feature.claims.symbols)?;
        record_claims(
            &mut resources,
            "logical resource",
            &feature.id,
            &feature.claims.resources,
        )?;
        record_claims(&mut pins, "physical pin", &feature.id, &feature.claims.pins)?;
        record_claims(
            &mut peripherals,
            "peripheral",
            &feature.id,
            &feature.claims.peripherals,
        )?;
        record_claims(
            &mut interrupts,
            "interrupt binding",
            &feature.id,
            &feature.claims.interrupts,
        )?;
    }

    let available_symbols: BTreeSet<_> = symbols.keys().cloned().collect();
    for feature in features {
        for required in &feature.claims.required_symbols {
            if !available_symbols.contains(required) {
                bail!(
                    "feature `{}` requires symbol `{required}`, but no selected feature claims it",
                    feature.id
                );
            }
        }
    }

    Ok(())
}

fn record_claims(
    owners: &mut BTreeMap<String, String>,
    kind: &str,
    feature_id: &str,
    claims: &BTreeSet<String>,
) -> Result<()> {
    for claim in claims {
        if let Some(first_owner) = owners.insert(claim.clone(), feature_id.to_owned()) {
            bail!(
                "duplicate {kind} claim `{claim}`: feature `{feature_id}` conflicts with feature `{first_owner}`"
            );
        }
    }
    Ok(())
}

fn require_metadata_contract(bundle: &FeatureBundle) -> Result<()> {
    require_set(
        "required_placeholders",
        &bundle.metadata.required_placeholders,
        &EXPECTED_PLACEHOLDERS,
    )?;
    require_set(
        "claimed_symbols",
        &bundle.metadata.claimed_symbols,
        &["blink_led"],
    )?;
    require_set(
        "claimed_resources",
        &bundle.metadata.claimed_resources,
        &["LED_RESOURCE_NAME", "BLINKER_ENABLED_NAME"],
    )?;
    require_set(
        "claimed_interrupts",
        &bundle.metadata.claimed_interrupts,
        &[],
    )?;
    if !bundle.metadata.required_symbols.is_empty() {
        bail!("blink_led metadata `required_symbols` must be empty for the MVP");
    }
    if !bundle.metadata.insertion_after.is_empty() {
        bail!("blink_led metadata `insertion_after` must be empty for the MVP");
    }
    Ok(())
}

fn require_button_metadata_contract(bundle: &FeatureBundle) -> Result<()> {
    require_set(
        "required_placeholders",
        &bundle.metadata.required_placeholders,
        &BUTTON_EXPECTED_PLACEHOLDERS,
    )?;
    require_set(
        "claimed_symbols",
        &bundle.metadata.claimed_symbols,
        &["button_toggle", "button_toggle_debounce"],
    )?;
    require_set(
        "claimed_resources",
        &bundle.metadata.claimed_resources,
        &["BUTTON_RESOURCE_NAME", "EXTI_RESOURCE_NAME"],
    )?;
    require_set(
        "claimed_interrupts",
        &bundle.metadata.claimed_interrupts,
        &["BUTTON_INTERRUPT"],
    )?;
    require_set(
        "required_symbols",
        &bundle.metadata.required_symbols,
        &["blink_led"],
    )?;
    require_set(
        "insertion_after",
        &bundle.metadata.insertion_after,
        &["blink_led"],
    )?;
    Ok(())
}

fn require_set(field: &str, actual: &[String], expected: &[&str]) -> Result<()> {
    let actual: BTreeSet<_> = actual.iter().map(String::as_str).collect();
    let expected: BTreeSet<_> = expected.iter().copied().collect();
    if actual != expected {
        bail!("unsupported feature metadata `{field}`: expected {expected:?}, got {actual:?}");
    }
    Ok(())
}

fn validate_identifier(value: &str, path: &str) -> Result<()> {
    let identifier = syn::parse_str::<syn::Ident>(value)
        .map_err(|_| anyhow::anyhow!("invalid Rust identifier at `{path}`: `{value}`"))?;
    if identifier != value {
        bail!("invalid plain Rust identifier at `{path}`: `{value}`");
    }
    Ok(())
}

fn require_str(label: &str, actual: &str, expected: &str) -> Result<()> {
    if actual != expected {
        bail!("unsupported {label}: expected `{expected}`, got `{actual}`");
    }
    Ok(())
}

fn resolved_fingerprint(
    bundle_fingerprint: &str,
    replacements: &BTreeMap<String, String>,
    claims: &ResolvedClaims,
) -> String {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, bundle_fingerprint);
    for (name, value) in replacements {
        hash_part(&mut hasher, name);
        hash_part(&mut hasher, value);
    }
    for (kind, values) in [
        ("symbols", &claims.symbols),
        ("required_symbols", &claims.required_symbols),
        ("resources", &claims.resources),
        ("pins", &claims.pins),
        ("peripherals", &claims.peripherals),
        ("interrupts", &claims.interrupts),
    ] {
        hash_part(&mut hasher, kind);
        for value in values {
            hash_part(&mut hasher, value);
        }
    }
    hex::encode(hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::feature::load_feature_bundle;
    use crate::manifest;

    use super::*;

    const APPLICATION: &str = include_str!("../../applications/nucleo-f401re-blinky.toml");
    const OSD_APPLICATION: &str = include_str!("../../applications/nucleo-f401re-osd.toml");
    const BSP: &str = include_str!("../../bsp/nucleo-f401re.toml");

    fn valid_manifest() -> Manifest {
        manifest::parse(APPLICATION, BSP).unwrap()
    }

    fn bundle() -> FeatureBundle {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../feature-library");
        load_feature_bundle(&root, BLINK_LED_IMPLEMENTATION).unwrap()
    }

    fn button_bundle() -> FeatureBundle {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../feature-library");
        load_feature_bundle(&root, BUTTON_TOGGLE_IMPLEMENTATION).unwrap()
    }

    fn osd_bundle() -> FeatureBundle {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../feature-library");
        load_feature_bundle(&root, OSD_IMPLEMENTATION).unwrap()
    }

    fn button_arm_bundle() -> FeatureBundle {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../feature-library");
        load_feature_bundle(&root, BUTTON_ARM_IMPLEMENTATION).unwrap()
    }

    #[test]
    fn resolves_only_fixed_safe_tokens() {
        let manifest = valid_manifest();
        let resolved = resolve_blink_led(&manifest, &bundle()).unwrap();

        assert_eq!(resolved.id, "blink_led");
        assert_eq!(
            resolved.replacements["LED_RESOURCE_TYPE"],
            "PA5<Output<PushPull>>"
        );
        assert_eq!(resolved.replacements["BLINK_PERIOD_MS"], "1000");
        assert_eq!(resolved.replacements["TASK_PRIORITY"], "1");
        assert!(
            resolved.replacements["LED_INIT_EXPRESSION"]
                .contains("into_push_pull_output_in_state(PinState::High)")
        );
        assert_eq!(resolved.claims.pins, BTreeSet::from(["PA5".to_owned()]));
        assert_eq!(resolved.fingerprint.len(), 64);
    }

    #[test]
    fn rejects_unsupported_hardware_without_fallback() {
        let mut manifest = valid_manifest();
        manifest.resources.leds.get_mut("led2").unwrap().pin = "PB6".into();

        let error = resolve_blink_led(&manifest, &bundle()).unwrap_err();
        assert!(error.to_string().contains("resources.leds.led2.pin"));
        assert!(error.to_string().contains("expected `PA5`, got `PB6`"));
    }

    #[test]
    fn translates_declared_default_level() {
        let mut manifest = valid_manifest();
        manifest.resources.leds.get_mut("led2").unwrap().default = "low".into();

        let resolved = resolve_blink_led(&manifest, &bundle()).unwrap();
        assert!(
            resolved.replacements["LED_INIT_EXPRESSION"]
                .contains("into_push_pull_output_in_state(PinState::Low)")
        );
    }

    #[test]
    fn preserves_exact_period_translation() {
        let mut manifest = valid_manifest();
        manifest.blink_led_feature_mut().unwrap().toggle_period_ms = 250;
        let resolved = resolve_blink_led(&manifest, &bundle()).unwrap();
        assert_eq!(resolved.replacements["BLINK_PERIOD_MS"], "250");
    }

    #[test]
    fn resolves_button_exti_and_debounce_policy() {
        let manifest = valid_manifest();
        let feature = manifest.button_toggle_feature().unwrap();
        let resolved =
            resolve_button_feature(&manifest, BUTTON_TOGGLE_ID, feature, &button_bundle()).unwrap();

        assert_eq!(resolved.replacements["BUTTON_RESOURCE_TYPE"], "PC13<Input>");
        assert_eq!(resolved.replacements["BUTTON_INTERRUPT"], "EXTI15_10");
        assert_eq!(resolved.replacements["DEBOUNCE_MS"], "20");
        assert!(resolved.replacements["BUTTON_INIT_EXPRESSION"].contains("Pull::Up"));
        assert!(resolved.replacements["BUTTON_INIT_EXPRESSION"].contains("Edge::Falling"));
        assert_eq!(resolved.claims.pins, BTreeSet::from(["PC13".to_owned()]));
        assert_eq!(
            resolved.claims.interrupts,
            BTreeSet::from(["EXTI15_10".to_owned(), "EXTI1".to_owned()])
        );
    }

    #[test]
    fn resolves_usart1_dma_osd_without_owning_uart_in_the_component() {
        let manifest = manifest::parse(OSD_APPLICATION, BSP).unwrap();
        let feature = match manifest.features.get(OSD_ID).unwrap() {
            FeatureConfig::OsdDisplayPort(feature) => feature,
            _ => panic!("expected OSD feature"),
        };
        let resolved = resolve_osd_feature(&manifest, OSD_ID, feature, &osd_bundle()).unwrap();

        assert_eq!(resolved.replacements["UART_INTERRUPT"], "USART1");
        assert_eq!(resolved.replacements["RX_DMA_INTERRUPT"], "DMA2_STREAM5");
        assert_eq!(resolved.replacements["TX_DMA_INTERRUPT"], "DMA2_STREAM7");
        assert_eq!(resolved.replacements["BUFFER_SIZE"], "70");
        assert_eq!(resolved.replacements["RX_QUEUE_CAPACITY"], "4");
        assert_eq!(resolved.replacements["TX_QUEUE_CAPACITY"], "16");
        assert_eq!(resolved.replacements["REFRESH_PERIOD_MS"], "100");
        assert_eq!(
            resolved.claims.pins,
            BTreeSet::from(["PA10".to_owned(), "PA9".to_owned()])
        );
        assert!(resolved.claims.resources.contains("osd_serial"));
        assert!(resolved.claims.resources.contains("osd_component"));
    }

    #[test]
    fn resolves_debounced_button_as_an_osd_consumer() {
        let manifest = manifest::parse(OSD_APPLICATION, BSP).unwrap();
        let feature = match manifest.features.get(BUTTON_ARM_ID).unwrap() {
            FeatureConfig::ButtonArmToggle(feature) => feature,
            _ => panic!("expected ARM button feature"),
        };
        let resolved =
            resolve_button_arm_feature(&manifest, BUTTON_ARM_ID, feature, &button_arm_bundle())
                .unwrap();

        assert_eq!(resolved.replacements["BUTTON_INTERRUPT"], "EXTI15_10");
        assert_eq!(resolved.replacements["DEBOUNCE_MS"], "20");
        assert_eq!(
            resolved.replacements["OSD_TELEMETRY_RESOURCE"],
            "osd_telemetry"
        );
        assert_eq!(
            resolved.claims.required_symbols,
            BTreeSet::from([OSD_ID.to_owned()])
        );
    }

    #[test]
    fn rejects_colliding_resolved_claims() {
        let resolved = resolve_blink_led(&valid_manifest(), &bundle()).unwrap();
        let error = validate_resolved_claims(&[resolved.clone(), resolved]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate symbol claim `blink_led`")
        );
    }
}
