use std::collections::BTreeSet;

use anyhow::{Result, bail};

use crate::{
    manifest::{
        BlinkLedFeature, ButtonArmToggleFeature, ButtonToggleFeature, FeatureConfig, Manifest,
        OsdDisplayPortFeature,
    },
    mcu,
};

const FEATURE_ID: &str = "blink_led";
const BUTTON_FEATURE_ID: &str = "button_toggle";
const BSP_ID: &str = "nucleo-f401re";
const LED_ID: &str = "led2";
const BUTTON_ID: &str = "user_button";
const OSD_FEATURE_ID: &str = "osd_displayport";
const BUTTON_ARM_FEATURE_ID: &str = "button_arm_toggle";

/// Validates all semantic constraints of the deliberately narrow MVP backend.
pub fn validate_manifest(manifest: &Manifest) -> Result<()> {
    require_eq(
        "application.schema_version",
        manifest.application_schema_version,
        6,
    )?;
    require_eq("bsp.schema_version", manifest.bsp_schema_version, 6)?;
    validate_application(manifest)?;
    validate_bsp(manifest)?;

    for (feature_name, feature) in &manifest.features {
        match feature {
            FeatureConfig::BlinkLed(feature) => {
                validate_blink_led(manifest, feature_name, feature)?
            }
            FeatureConfig::ButtonToggle(feature) => {
                validate_button_toggle(manifest, feature_name, feature)?
            }
            FeatureConfig::ButtonArmToggle(feature) => {
                validate_button_arm_toggle(manifest, feature_name, feature)?
            }
            FeatureConfig::OsdDisplayPort(feature) => {
                validate_osd_displayport(manifest, feature_name, feature)?
            }
        }
    }

    Ok(())
}

/// Compatibility spelling for callers that use a shorter verb.
pub fn validate(manifest: &Manifest) -> Result<()> {
    validate_manifest(manifest)
}

fn validate_application(manifest: &Manifest) -> Result<()> {
    if !is_safe_application_slug(&manifest.application.name) {
        bail!(
            "invalid value at `application.name`: expected a lowercase ASCII slug starting with a letter and containing only letters, digits, and single hyphens, got `{}`",
            manifest.application.name
        );
    }

    require_str(
        "application.bsp",
        &manifest.application.bsp,
        &manifest.bsp.id,
    )?;

    let mut ordered = BTreeSet::new();
    for (index, feature_name) in manifest.application.feature_order.iter().enumerate() {
        if !ordered.insert(feature_name.as_str()) {
            bail!(
                "duplicate feature at `application.feature_order[{index}]`: `{feature_name}` already appears earlier"
            );
        }
    }

    let declared: BTreeSet<&str> = manifest.features.keys().map(String::as_str).collect();
    if ordered != declared {
        let missing_from_order: Vec<_> = declared.difference(&ordered).copied().collect();
        let missing_from_features: Vec<_> = ordered.difference(&declared).copied().collect();
        bail!(
            "mismatch at `application.feature_order`: every key in `features` must appear exactly once; missing from order: {missing_from_order:?}; missing from `features`: {missing_from_features:?}"
        );
    }

    let blinker = BTreeSet::from([FEATURE_ID, BUTTON_FEATURE_ID]);
    let osd = BTreeSet::from([OSD_FEATURE_ID, BUTTON_ARM_FEATURE_ID]);
    if declared != blinker && declared != osd {
        bail!(
            "unsupported value at `features`: expected the blinker/button pair or USART1/OSD/arm-button set, got {:?}",
            manifest.features.keys().collect::<Vec<_>>()
        );
    }
    let valid_order = (declared == blinker
        && manifest.application.feature_order == [FEATURE_ID, BUTTON_FEATURE_ID])
        || (declared == osd
            && manifest.application.feature_order == [OSD_FEATURE_ID, BUTTON_ARM_FEATURE_ID]);
    if !valid_order {
        bail!(
            "unsupported value at `application.feature_order`: provider must precede its dependent consumer"
        );
    }

    Ok(())
}

fn validate_bsp(manifest: &Manifest) -> Result<()> {
    require_str("bsp.id", &manifest.bsp.id, BSP_ID)?;
    mcu::profile(&manifest.bsp.mcu)?;

    require_str("clock.source", &manifest.clock.source, "hsi")?;
    require_eq(
        "clock.source_frequency_hz",
        manifest.clock.source_frequency_hz,
        16_000_000,
    )?;
    require_eq(
        "clock.system_clock_hz",
        manifest.clock.system_clock_hz,
        84_000_000,
    )?;

    let led_ids = manifest
        .resources
        .leds
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if led_ids != BTreeSet::from([LED_ID]) {
        bail!(
            "unsupported value at `resources.leds`: the NUCLEO MVP BSP requires exactly `{LED_ID}`, got {:?}",
            manifest.resources.leds.keys().collect::<Vec<_>>()
        );
    }
    let button_ids = manifest
        .resources
        .buttons
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if button_ids != BTreeSet::from([BUTTON_ID]) {
        bail!(
            "unsupported value at `resources.buttons`: the NUCLEO button BSP requires exactly `{BUTTON_ID}`, got {:?}",
            manifest.resources.buttons.keys().collect::<Vec<_>>()
        );
    }
    for (resource_name, button) in &manifest.resources.buttons {
        let root = format!("resources.buttons.{resource_name}");
        validate_identifier(&format!("{root} resource ID"), resource_name)?;
        validate_pin_token(&format!("{root}.pin"), &button.pin)?;
        require_str(&format!("{root}.pin"), &button.pin, "PC13")?;
    }

    for (resource_name, led) in &manifest.resources.leds {
        let root = format!("resources.leds.{resource_name}");
        validate_identifier(&format!("{root} resource ID"), resource_name)?;
        validate_pin_token(&format!("{root}.pin"), &led.pin)?;
        require_str(&format!("{root}.pin"), &led.pin, "PA5")?;
        if !matches!(led.default.as_str(), "high" | "low") {
            bail!(
                "unsupported value at `{root}.default`: expected `high` or `low`, got `{}`",
                led.default
            );
        }
    }

    if manifest
        .resources
        .serial
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != BTreeSet::from(["usart1"])
    {
        bail!("unsupported value at `resources.serial`: expected exactly `usart1`");
    }
    let serial = manifest.serial_endpoint("usart1").unwrap();
    require_str(
        "resources.serial.usart1.peripheral",
        &serial.peripheral,
        "USART1",
    )?;
    require_str("resources.serial.usart1.tx_pin", &serial.tx_pin, "PA9")?;
    require_str("resources.serial.usart1.rx_pin", &serial.rx_pin, "PA10")?;

    for (id, stream) in [("usart1_rx", 5), ("usart1_tx", 7)] {
        let route = manifest
            .dma_route(id)
            .ok_or_else(|| anyhow::anyhow!("missing `resources.dma.{id}`"))?;
        require_str(
            &format!("resources.dma.{id}.controller"),
            &route.controller,
            "DMA2",
        )?;
        require_eq(
            &format!("resources.dma.{id}.stream"),
            u64::from(route.stream),
            stream,
        )?;
        require_eq(
            &format!("resources.dma.{id}.channel"),
            u64::from(route.channel),
            4,
        )?;
        require_str(
            &format!("resources.dma.{id}.interrupt"),
            &route.interrupt,
            &format!("DMA2_STREAM{stream}"),
        )?;
    }

    Ok(())
}

fn validate_osd_displayport(
    manifest: &Manifest,
    feature_name: &str,
    feature: &OsdDisplayPortFeature,
) -> Result<()> {
    let root = format!("features.{feature_name}");
    require_str(feature_name, feature_name, OSD_FEATURE_ID)?;
    require_str(
        &format!("{root}.implementation"),
        &feature.implementation,
        "stm32f4/msp-displayport-usart1-dma",
    )?;
    if manifest.serial_endpoint(&feature.endpoint).is_none() || feature.endpoint != "usart1" {
        bail!("invalid value at `{root}.endpoint`: expected BSP endpoint `usart1`");
    }
    require_str(&format!("{root}.rx_dma"), &feature.rx_dma, "usart1_rx")?;
    require_str(&format!("{root}.tx_dma"), &feature.tx_dma, "usart1_tx")?;
    for (path, value, expected) in [
        ("rx_buffer_size", feature.rx_buffer_size, 70),
        ("rx_buffer_count", feature.rx_buffer_count, 2),
        ("rx_queue_capacity", feature.rx_queue_capacity, 4),
        ("tx_buffer_size", feature.tx_buffer_size, 70),
        ("tx_queue_capacity", feature.tx_queue_capacity, 16),
    ] {
        require_eq(&format!("{root}.{path}"), value, expected)?;
    }
    for priority in [
        feature.refresh_task_priority,
        feature.uart_irq_priority,
        feature.rx_dma_irq_priority,
        feature.tx_dma_irq_priority,
        feature.tx_worker_priority,
    ] {
        if !(1..=16).contains(&priority) {
            bail!("invalid RTIC priority in `{root}`");
        }
    }
    if feature.refresh_period_ms == 0 || 1_000 % feature.refresh_period_ms != 0 {
        bail!(
            "invalid value at `{root}.refresh_period_ms`: {} must divide 1000 ms exactly",
            feature.refresh_period_ms
        );
    }
    require_str(
        &format!("{root}.task_name"),
        &feature.task_name,
        OSD_FEATURE_ID,
    )?;
    if !(1..=16).contains(&feature.task_priority) {
        bail!("invalid value at `{root}.task_priority`: expected 1..=16");
    }
    Ok(())
}

fn validate_button_arm_toggle(
    manifest: &Manifest,
    feature_name: &str,
    feature: &ButtonArmToggleFeature,
) -> Result<()> {
    let root = format!("features.{feature_name}");
    require_str(feature_name, feature_name, BUTTON_ARM_FEATURE_ID)?;
    require_str(
        &format!("{root}.implementation"),
        &feature.implementation,
        "stm32f4/button-arm-toggle",
    )?;
    require_str(
        &format!("{root}.task_name"),
        &feature.task_name,
        BUTTON_ARM_FEATURE_ID,
    )?;
    if !(1..=16).contains(&feature.task_priority) {
        bail!("invalid value at `{root}.task_priority`: expected 1..=16");
    }
    if feature.debounce_ms < 2 || 1_000 % feature.debounce_ms != 0 {
        bail!(
            "invalid value at `{root}.debounce_ms`: {} must be at least 2 ms and divide 1000 ms exactly",
            feature.debounce_ms
        );
    }
    require_str(&format!("{root}.button"), &feature.button, BUTTON_ID)?;
    if manifest.button(&feature.button).is_none() {
        bail!("unknown BSP button resource at `{root}.button`");
    }
    require_str(&format!("{root}.osd"), &feature.osd, OSD_FEATURE_ID)?;
    Ok(())
}

fn validate_button_toggle(
    manifest: &Manifest,
    feature_name: &str,
    feature: &ButtonToggleFeature,
) -> Result<()> {
    let root = format!("features.{feature_name}");
    require_str(feature_name, feature_name, BUTTON_FEATURE_ID)?;
    require_str(
        &format!("{root}.implementation"),
        &feature.implementation,
        "stm32f4/button-toggle-blink",
    )?;
    validate_identifier(&format!("{root}.task_name"), &feature.task_name)?;
    require_str(
        &format!("{root}.task_name"),
        &feature.task_name,
        BUTTON_FEATURE_ID,
    )?;
    if !(1..=16).contains(&feature.task_priority) {
        bail!(
            "invalid value at `{root}.task_priority`: expected an RTIC priority in 1..=16, got {}",
            feature.task_priority
        );
    }
    if feature.task_priority <= manifest.blink_led_feature().unwrap().task_priority {
        bail!(
            "invalid value at `{root}.task_priority`: button/debounce tasks must have higher priority than blink_led"
        );
    }
    if feature.debounce_ms < 2 || 1_000 % feature.debounce_ms != 0 {
        bail!(
            "invalid value at `{root}.debounce_ms`: {} must be at least 2 ms and divide 1000 ms exactly",
            feature.debounce_ms
        );
    }

    validate_resource_reference(&format!("{root}.button"), &feature.button)?;
    if manifest.button(&feature.button).is_none() {
        bail!(
            "unknown BSP button resource at `{root}.button`: `{}` is not declared in `resources.buttons`",
            feature.button
        );
    }
    validate_resource_reference(&format!("{root}.led"), &feature.led)?;
    if manifest.led(&feature.led).is_none() {
        bail!(
            "unknown BSP LED resource at `{root}.led`: `{}` is not declared in `resources.leds`",
            feature.led
        );
    }
    Ok(())
}

fn validate_resource_reference(path: &str, value: &str) -> Result<()> {
    validate_identifier(path, value)
}

fn validate_blink_led(
    manifest: &Manifest,
    feature_name: &str,
    feature: &BlinkLedFeature,
) -> Result<()> {
    let root = format!("features.{feature_name}");

    require_str(
        &format!("{root}.implementation"),
        &feature.implementation,
        "stm32f4/blink-led",
    )?;
    validate_identifier(&format!("{root}.task_name"), &feature.task_name)?;
    require_str(&format!("{root}.task_name"), &feature.task_name, FEATURE_ID)?;

    if !(1..=16).contains(&feature.task_priority) {
        bail!(
            "invalid value at `{root}.task_priority`: expected an RTIC priority in 1..=16, got {}",
            feature.task_priority
        );
    }

    if feature.toggle_period_ms == 0 {
        bail!("invalid value at `{root}.toggle_period_ms`: the period must be positive");
    }
    if 1_000 % feature.toggle_period_ms != 0 {
        bail!(
            "invalid value at `{root}.toggle_period_ms`: {} does not divide 1000 ms exactly",
            feature.toggle_period_ms
        );
    }

    validate_identifier(&format!("{root}.led"), &feature.led)?;
    if manifest.led(&feature.led).is_none() {
        bail!(
            "unknown BSP LED resource at `{root}.led`: `{}` is not declared in `resources.leds`",
            feature.led
        );
    }
    Ok(())
}

fn validate_identifier(path: &str, value: &str) -> Result<()> {
    if syn::parse_str::<syn::Ident>(value).is_err() {
        bail!("invalid Rust identifier at `{path}`: `{value}`");
    }
    Ok(())
}

fn validate_pin_token(path: &str, value: &str) -> Result<()> {
    let Some(pin) = value.strip_prefix('P') else {
        bail!(
            "invalid pin token at `{path}`: expected `P<port><number>` such as `PA5`, got `{value}`"
        );
    };
    let mut characters = pin.chars();
    let Some(port) = characters.next() else {
        bail!(
            "invalid pin token at `{path}`: expected `P<port><number>` such as `PA5`, got `{value}`"
        );
    };
    let number = characters.as_str();
    let valid_number = !(number.is_empty() || number.len() > 1 && number.starts_with('0'))
        && number.parse::<u8>().is_ok_and(|number| number <= 15);

    if !port.is_ascii_uppercase() || !valid_number {
        bail!(
            "invalid pin token at `{path}`: expected `P<port><number>` such as `PA5`, got `{value}`"
        );
    }
    Ok(())
}

fn is_safe_application_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    let Some(last) = bytes.last() else {
        return false;
    };

    first.is_ascii_lowercase()
        && (last.is_ascii_lowercase() || last.is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !value.contains("--")
}

fn require_str(path: &str, actual: &str, expected: &str) -> Result<()> {
    if actual != expected {
        bail!("unsupported value at `{path}`: expected `{expected}`, got `{actual}`");
    }
    Ok(())
}

fn require_eq<T>(path: &str, actual: T, expected: T) -> Result<()>
where
    T: Copy + std::fmt::Display + PartialEq,
{
    if actual != expected {
        bail!("unsupported value at `{path}`: expected `{expected}`, got `{actual}`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::manifest::parse;

    use super::*;

    const APPLICATION: &str = include_str!("../../applications/nucleo-f401re-blinky.toml");
    const BSP: &str = include_str!("../../bsp/nucleo-f401re.toml");

    fn valid_manifest() -> Manifest {
        parse(APPLICATION, BSP).expect("test fixtures must deserialize")
    }

    fn error_for(mutator: impl FnOnce(&mut Manifest)) -> String {
        let mut manifest = valid_manifest();
        mutator(&mut manifest);
        validate_manifest(&manifest)
            .expect_err("mutated manifest should fail validation")
            .to_string()
    }

    #[test]
    fn valid_nucleo_configuration_passes() {
        validate_manifest(&valid_manifest()).expect("valid fixtures should pass");
    }

    #[test]
    fn application_must_name_the_supplied_bsp() {
        let error = error_for(|manifest| manifest.application.bsp = "another-board".into());
        assert!(
            error.contains("application.bsp"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn unsafe_application_slug_is_rejected() {
        let error = error_for(|manifest| manifest.application.name = "../escape".into());
        assert!(
            error.contains("application.name"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn feature_order_rejects_duplicates() {
        let error = error_for(|manifest| {
            manifest.application.feature_order.push(FEATURE_ID.into());
        });
        assert!(
            error.contains("application.feature_order[2]"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn feature_order_must_equal_feature_keys() {
        let error = error_for(|manifest| manifest.application.feature_order.clear());
        assert!(
            error.contains("application.feature_order"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn feature_resource_references_are_checked() {
        let error = error_for(|manifest| {
            manifest.blink_led_feature_mut().unwrap().led = "missing_led".into();
        });
        assert!(
            error.contains("features.blink_led.led") && error.contains("resources.leds"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rust_identifiers_are_parsed_by_syn() {
        let error = error_for(|manifest| {
            manifest.blink_led_feature_mut().unwrap().led = "led; unsafe {}".into();
        });
        assert!(
            error.contains("features.blink_led.led"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn toggle_period_must_be_positive_and_exact() {
        let zero = error_for(|manifest| {
            manifest.blink_led_feature_mut().unwrap().toggle_period_ms = 0;
        });
        assert!(zero.contains("features.blink_led.toggle_period_ms"));

        let rounded = error_for(|manifest| {
            manifest.blink_led_feature_mut().unwrap().toggle_period_ms = 333;
        });
        assert!(rounded.contains("does not divide 1000 ms exactly"));
    }

    #[test]
    fn task_priority_is_bounded() {
        for invalid in [0, 17] {
            let error = error_for(|manifest| {
                manifest.blink_led_feature_mut().unwrap().task_priority = invalid;
            });
            assert!(
                error.contains("features.blink_led.task_priority"),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn unsupported_mcu_profile_names_its_path() {
        let error = error_for(|manifest| manifest.bsp.mcu = "STM32F401RET6".into());
        assert!(error.contains("bsp.mcu"), "unexpected error: {error}");
    }

    #[test]
    fn compact_bsp_pin_token_is_validated_before_backend_support() {
        let error = error_for(|manifest| {
            manifest.resources.leds.get_mut(LED_ID).unwrap().pin = "A5".into();
        });
        assert!(
            error.contains("resources.leds.led2.pin") && error.contains("P<port><number>"),
            "unexpected error: {error}"
        );

        let unsupported = error_for(|manifest| {
            manifest.resources.leds.get_mut(LED_ID).unwrap().pin = "PB6".into();
        });
        assert!(
            unsupported.contains("resources.leds.led2.pin")
                && unsupported.contains("expected `PA5`"),
            "unexpected error: {unsupported}"
        );
    }

    #[test]
    fn led_default_is_a_bsp_owned_physical_level() {
        let error = error_for(|manifest| {
            manifest.resources.leds.get_mut(LED_ID).unwrap().default = "on".into();
        });
        assert!(
            error.contains("resources.leds.led2.default")
                && error.contains("expected `high` or `low`"),
            "unexpected error: {error}"
        );
    }
}
