//! Explicit compile-time registry for repository-owned App Builder inputs.
//!
//! Every external Rust input is named here. The registry deliberately avoids
//! filesystem glob discovery so a missing, duplicated, or newly introduced
//! input cannot enter generation silently.

use std::{collections::BTreeSet, path::Path};

use anyhow::{Result, bail};

use crate::rtic::{composition::AppComposition, resolve::ResolvedApp};

#[path = "../../../apps/foxeer-f405-v2/app_composition.rs"]
pub mod foxeer_app_composition;
#[path = "../../../boards/foxeer-f405-v2/board.rs"]
pub mod foxeer_board;
#[path = "../../../apps/foxeer-f405-v2/verification/golden_reconciliation.rs"]
pub mod foxeer_golden_reconciliation;
#[path = "../../../apps/foxeer-f405-v2/components/golden_services.rs"]
pub mod foxeer_golden_services;
#[path = "../../../apps/foxeer-f405-v2/live_config.rs"]
pub mod foxeer_live_config;
#[path = "../../../apps/foxeer-f405-v2/platform_config.rs"]
pub mod foxeer_platform_config;

#[path = "../../../tasks/actuator_fault_reporter.rs"]
mod portable_actuator_fault_reporter;
#[path = "../../../tasks/imu_control_bridge.rs"]
mod portable_imu_control_bridge;
#[path = "../../../tasks/inhibited_actuator.rs"]
mod portable_inhibited_actuator;
#[path = "../../../tasks/observe_button_change.rs"]
mod portable_observe_button_change;

#[path = "../../../apps/foxeer-f405-v2/tasks/foxeer_control.rs"]
mod foxeer_control_input;
#[path = "../../../apps/foxeer-f405-v2/tasks/golden_flash.rs"]
mod foxeer_golden_flash_input;
#[path = "../../../apps/foxeer-f405-v2/tasks/heartbeat.rs"]
mod foxeer_heartbeat_input;
#[path = "../../../apps/foxeer-f405-v2/tasks/foxeer_safety_master.rs"]
mod foxeer_safety_master_input;
#[path = "../../../apps/foxeer-f405-v2/tasks/usb_cdc.rs"]
mod foxeer_usb_cdc_input;

/// Selected Foxeer input namespace used by resolver and regression tests.
pub mod foxeer_f405_v2 {
    pub use super::foxeer_app_composition as app_composition;
    pub use super::foxeer_board as board;
    pub use super::foxeer_golden_reconciliation as golden_reconciliation;
    pub use super::foxeer_live_config as live_config;
    pub use super::foxeer_platform_config as platform_config;
}

/// Explicit task namespace assembled from portable, backend, and app inputs.
pub mod selected_tasks {
    pub use super::foxeer_control_input::foxeer_control;
    pub use super::foxeer_golden_flash_input::golden_flash;
    pub use super::foxeer_heartbeat_input::heartbeat;
    pub use super::foxeer_safety_master_input::foxeer_safety_master;
    pub use super::foxeer_usb_cdc_input::usb_cdc;
    pub use super::portable_actuator_fault_reporter::actuator_fault_reporter;
    pub use super::portable_imu_control_bridge::imu_control_bridge;
    pub use super::portable_inhibited_actuator::inhibited_actuator;
    pub use super::portable_observe_button_change::observe_button_change;
    pub use crate::backends::stm32f4::tasks::{
        adc_observation_dma, adc_observation_poll, blink_led, button_exti, dshot_dma_complete,
        dshot_service, esc_manager, esc_uart_rx_dma, esc_uart_rx_idle, imu_data_ready, io_watchdog,
        msp_osd, periodic_control_tick, physical_actuator, serial_discard, serial_rx_bridge,
        serial_rx_dma_irq, serial_rx_idle_irq, serial_tx_dma_irq, serial_tx_worker, simple_osd,
        spi_imu_owner_service, spi_imu_parser, spi_imu_poll, spi_imu_rx_dma_irq, spi_imu_timeout,
    };
}

/// Source form selected for one complete catalog application.
#[derive(Clone, Copy)]
pub enum ApplicationInput {
    /// Repository-owned Rust composition compiled through this catalog.
    RustComposition {
        /// Complete typed composition.
        composition: &'static AppComposition,
        /// App-specific fail-closed reconciliation performed after resolution.
        reconcile: fn(&ResolvedApp) -> Result<()>,
        /// Deterministic app-specific report appendix.
        report: fn() -> String,
        /// App-owned initial live-configuration values.
        live_config: LiveConfigInput,
    },
    /// Checked strict manifests confined to this builder workspace.
    ValidationManifest {
        /// Builder-relative application manifest path.
        application_path: &'static str,
        /// Builder-relative BSP manifest path.
        bsp_path: &'static str,
    },
}

/// Bounded app-owned live-configuration defaults lowered into firmware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveConfigInput {
    /// Initial tuning publication sequence.
    pub initial_tuning_request_seq: u32,
    /// Initial blackbox logging divisor.
    pub initial_flash_log_rate_divisor: u32,
}

/// One selectable complete application and its build-facing identity.
#[derive(Clone, Copy)]
pub struct ApplicationEntry {
    /// Stable CLI and output-directory identifier.
    pub id: &'static str,
    /// Reviewed input form used to resolve the application.
    pub input: ApplicationInput,
    /// Rust embedded compilation target.
    pub rust_target: &'static str,
    /// Exact probe-rs chip selector used only by explicit hardware commands.
    pub probe_chip: &'static str,
    /// Generated Cargo binary name.
    pub binary_name: &'static str,
}

/// Complete allowlisted application catalog.
pub const APPLICATIONS: &[ApplicationEntry] = &[
    ApplicationEntry {
        id: "foxeer-f405-v2",
        input: ApplicationInput::RustComposition {
            composition: &foxeer_app_composition::APP_COMPOSITION,
            reconcile: foxeer_golden_reconciliation::validate,
            report: foxeer_golden_reconciliation::render,
            live_config: LiveConfigInput {
                initial_tuning_request_seq: foxeer_live_config::INITIAL_TUNING_REQUEST_SEQ,
                initial_flash_log_rate_divisor: foxeer_live_config::INITIAL_FLASH_LOG_RATE_DIVISOR,
            },
        },
        rust_target: "thumbv7em-none-eabihf",
        probe_chip: "STM32F405RG",
        binary_name: "app-builder-v3-generated",
    },
    ApplicationEntry {
        id: "nucleo-f401re-blinky",
        input: ApplicationInput::ValidationManifest {
            application_path: "applications/nucleo-f401re-blinky.toml",
            bsp_path: "bsp/nucleo-f401re.toml",
        },
        rust_target: "thumbv7em-none-eabihf",
        probe_chip: "STM32F401RE",
        binary_name: "rtic-generated-app",
    },
    ApplicationEntry {
        id: "nucleo-f401re-osd",
        input: ApplicationInput::ValidationManifest {
            application_path: "applications/nucleo-f401re-osd.toml",
            bsp_path: "bsp/nucleo-f401re.toml",
        },
        rust_target: "thumbv7em-none-eabihf",
        probe_chip: "STM32F401RE",
        binary_name: "rtic-generated-app",
    },
];

/// Resolves an exact allowlisted application ID.
pub fn application(id: &str) -> Result<&'static ApplicationEntry> {
    APPLICATIONS
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown application `{id}`; expected one of: {}",
                application_names().join(", ")
            )
        })
}

/// Returns selectable application IDs in deterministic catalog order.
pub fn application_names() -> Vec<&'static str> {
    APPLICATIONS.iter().map(|entry| entry.id).collect()
}

/// Validates application identities and confined manifest registrations.
pub fn validate_applications(builder_root: &Path) -> Result<()> {
    validate_application_entries(builder_root, APPLICATIONS)
}

fn validate_application_entries(builder_root: &Path, entries: &[ApplicationEntry]) -> Result<()> {
    let mut ids = BTreeSet::new();
    for entry in entries {
        if !ids.insert(entry.id) {
            bail!("application ID `{}` is registered more than once", entry.id);
        }
        if let ApplicationInput::ValidationManifest {
            application_path,
            bsp_path,
        } = entry.input
        {
            for (kind, relative) in [("application", application_path), ("BSP", bsp_path)] {
                let path = Path::new(relative);
                if path.is_absolute()
                    || path
                        .components()
                        .any(|component| matches!(component, std::path::Component::ParentDir))
                {
                    bail!(
                        "{kind} input for `{}` is not confined: `{relative}`",
                        entry.id
                    );
                }
                if !builder_root.join(path).is_file() {
                    bail!("{kind} input for `{}` is missing at `{relative}`", entry.id);
                }
            }
        }
    }
    Ok(())
}

/// Ownership class of one compiled Rust input.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InputKind {
    /// Physical board facts outside the builder workspace.
    Board,
    /// App composition, configuration, component, or verification input.
    App,
    /// HAL-independent task input under top-level `tasks/`.
    PortableTask,
    /// App-specific task input under the selected app.
    AppTask,
    /// Host-side STM32F4 task declaration owned by the builder backend.
    BackendTask,
}

/// One explicitly registered repository Rust input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RustInput {
    /// Stable catalog identity.
    pub id: &'static str,
    /// Physical ownership class.
    pub kind: InputKind,
    /// Repository-relative source path.
    pub path: &'static str,
}

const fn input(id: &'static str, kind: InputKind, path: &'static str) -> RustInput {
    RustInput { id, kind, path }
}

/// Complete explicit input inventory used by the initial consolidated builder.
pub const RUST_INPUTS: &[RustInput] = &[
    input(
        "foxeer_f405_v2_board",
        InputKind::Board,
        "boards/foxeer-f405-v2/board.rs",
    ),
    input(
        "foxeer_f405_v2_composition",
        InputKind::App,
        "apps/foxeer-f405-v2/app_composition.rs",
    ),
    input(
        "foxeer_f405_v2_golden_services",
        InputKind::App,
        "apps/foxeer-f405-v2/components/golden_services.rs",
    ),
    input(
        "foxeer_f405_v2_live_config",
        InputKind::App,
        "apps/foxeer-f405-v2/live_config.rs",
    ),
    input(
        "foxeer_f405_v2_platform_config",
        InputKind::App,
        "apps/foxeer-f405-v2/platform_config.rs",
    ),
    input(
        "foxeer_f405_v2_reconciliation",
        InputKind::App,
        "apps/foxeer-f405-v2/verification/golden_reconciliation.rs",
    ),
    input(
        "actuator_fault_reporter",
        InputKind::PortableTask,
        "tasks/actuator_fault_reporter.rs",
    ),
    input(
        "imu_control_bridge",
        InputKind::PortableTask,
        "tasks/imu_control_bridge.rs",
    ),
    input(
        "inhibited_actuator",
        InputKind::PortableTask,
        "tasks/inhibited_actuator.rs",
    ),
    input(
        "observe_button_change",
        InputKind::PortableTask,
        "tasks/observe_button_change.rs",
    ),
    input(
        "foxeer_control",
        InputKind::AppTask,
        "apps/foxeer-f405-v2/tasks/foxeer_control.rs",
    ),
    input(
        "foxeer_safety_master",
        InputKind::AppTask,
        "apps/foxeer-f405-v2/tasks/foxeer_safety_master.rs",
    ),
    input(
        "golden_flash",
        InputKind::AppTask,
        "apps/foxeer-f405-v2/tasks/golden_flash.rs",
    ),
    input(
        "heartbeat",
        InputKind::AppTask,
        "apps/foxeer-f405-v2/tasks/heartbeat.rs",
    ),
    input(
        "usb_cdc",
        InputKind::AppTask,
        "apps/foxeer-f405-v2/tasks/usb_cdc.rs",
    ),
    input(
        "adc_observation",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/adc_observation.rs",
    ),
    input(
        "blink_led",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/blink_led.rs",
    ),
    input(
        "button_exti",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/button_exti.rs",
    ),
    input(
        "dshot_dma",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/dshot_dma.rs",
    ),
    input(
        "dshot_service",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/dshot_service.rs",
    ),
    input(
        "esc_manager",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/esc_manager.rs",
    ),
    input(
        "esc_uart_irq",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/esc_uart_irq.rs",
    ),
    input(
        "imu_data_ready",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/imu_data_ready.rs",
    ),
    input(
        "io_watchdog",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/io_watchdog.rs",
    ),
    input(
        "msp_osd",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/msp_osd.rs",
    ),
    input(
        "periodic_control_tick",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/periodic_control_tick.rs",
    ),
    input(
        "physical_actuator",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/physical_actuator.rs",
    ),
    input(
        "serial_discard",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/serial_discard.rs",
    ),
    input(
        "serial_rx_bridge",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/serial_rx_bridge.rs",
    ),
    input(
        "serial_rx_dma_irq",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/serial_rx_dma_irq.rs",
    ),
    input(
        "serial_rx_idle_irq",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/serial_rx_idle_irq.rs",
    ),
    input(
        "serial_tx_dma_irq",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/serial_tx_dma_irq.rs",
    ),
    input(
        "serial_tx_worker",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/serial_tx_worker.rs",
    ),
    input(
        "simple_osd",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/simple_osd.rs",
    ),
    input(
        "spi_imu_owner_service",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/spi_imu_owner_service.rs",
    ),
    input(
        "spi_imu_parser",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/spi_imu_parser.rs",
    ),
    input(
        "spi_imu_poll",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/spi_imu_poll.rs",
    ),
    input(
        "spi_imu_rx_dma_irq",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/spi_imu_rx_dma_irq.rs",
    ),
    input(
        "spi_imu_timeout",
        InputKind::BackendTask,
        "tools/app-builder/src/backends/stm32f4/tasks/spi_imu_timeout.rs",
    ),
];

/// Validates that all registered paths exist and that IDs and paths are unique.
pub fn validate(repository_root: &Path) -> Result<()> {
    validate_entries(repository_root, RUST_INPUTS)
}

fn validate_entries(repository_root: &Path, entries: &[RustInput]) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for entry in entries {
        if !ids.insert(entry.id) {
            bail!("Rust input ID `{}` is registered more than once", entry.id);
        }
        if !paths.insert(entry.path) {
            bail!(
                "Rust input path `{}` is registered more than once",
                entry.path
            );
        }
        if !repository_root.join(entry.path).is_file() {
            bail!(
                "registered Rust input `{}` is missing at `{}`",
                entry.id,
                entry.path
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtic::task::TaskContract;

    fn repository_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_owned()
    }

    #[test]
    fn explicit_catalog_is_complete_unique_and_physically_partitioned() {
        let root = repository_root();
        validate(&root).unwrap();

        let count = |kind| {
            RUST_INPUTS
                .iter()
                .filter(|input| input.kind == kind)
                .count()
        };
        assert_eq!(count(InputKind::Board), 1);
        assert_eq!(count(InputKind::App), 5);
        assert_eq!(count(InputKind::PortableTask), 4);
        assert_eq!(count(InputKind::AppTask), 5);
        assert_eq!(count(InputKind::BackendTask), 24);
    }

    #[test]
    fn missing_and_duplicate_registrations_fail_clearly() {
        let root = repository_root();
        let duplicate_id = [RUST_INPUTS[0], RUST_INPUTS[0]];
        assert!(
            validate_entries(&root, &duplicate_id)
                .unwrap_err()
                .to_string()
                .contains("ID")
        );

        let duplicate_path = [
            RUST_INPUTS[0],
            RustInput {
                id: "distinct_board_id",
                ..RUST_INPUTS[0]
            },
        ];
        assert!(
            validate_entries(&root, &duplicate_path)
                .unwrap_err()
                .to_string()
                .contains("path")
        );

        let missing = [RustInput {
            id: "missing",
            kind: InputKind::App,
            path: "apps/foxeer-f405-v2/not-present.rs",
        }];
        assert!(
            validate_entries(&root, &missing)
                .unwrap_err()
                .to_string()
                .contains("missing")
        );
    }

    #[test]
    fn application_catalog_is_explicit_complete_and_confined() {
        let builder = Path::new(env!("CARGO_MANIFEST_DIR"));
        validate_applications(builder).unwrap();
        assert_eq!(
            application_names(),
            vec![
                "foxeer-f405-v2",
                "nucleo-f401re-blinky",
                "nucleo-f401re-osd"
            ]
        );
        assert!(matches!(
            application("foxeer-f405-v2").unwrap().input,
            ApplicationInput::RustComposition { .. }
        ));
        for id in ["nucleo-f401re-blinky", "nucleo-f401re-osd"] {
            assert!(matches!(
                application(id).unwrap().input,
                ApplicationInput::ValidationManifest { .. }
            ));
        }
    }

    #[test]
    fn duplicate_and_unconfined_application_entries_fail() {
        let builder = Path::new(env!("CARGO_MANIFEST_DIR"));
        let duplicate = [APPLICATIONS[0], APPLICATIONS[0]];
        assert!(
            validate_application_entries(builder, &duplicate)
                .unwrap_err()
                .to_string()
                .contains("registered more than once")
        );

        let unconfined = [ApplicationEntry {
            id: "unconfined",
            input: ApplicationInput::ValidationManifest {
                application_path: "../outside.toml",
                bsp_path: "bsp/nucleo-f401re.toml",
            },
            rust_target: "thumbv7em-none-eabihf",
            probe_chip: "STM32F401RE",
            binary_name: "rtic-generated-app",
        }];
        assert!(
            validate_application_entries(builder, &unconfined)
                .unwrap_err()
                .to_string()
                .contains("not confined")
        );

        let missing = [ApplicationEntry {
            id: "missing",
            input: ApplicationInput::ValidationManifest {
                application_path: "applications/not-present.toml",
                bsp_path: "bsp/nucleo-f401re.toml",
            },
            rust_target: "thumbv7em-none-eabihf",
            probe_chip: "STM32F401RE",
            binary_name: "rtic-generated-app",
        }];
        assert!(
            validate_application_entries(builder, &missing)
                .unwrap_err()
                .to_string()
                .contains("missing")
        );
    }

    #[test]
    fn external_task_sources_resolve_to_the_registered_files() {
        let root = repository_root();
        let contracts: [(&TaskContract, &str); 9] = [
            (
                &selected_tasks::actuator_fault_reporter::CONTRACT,
                "tasks/actuator_fault_reporter.rs",
            ),
            (
                &selected_tasks::imu_control_bridge::CONTRACT,
                "tasks/imu_control_bridge.rs",
            ),
            (
                &selected_tasks::inhibited_actuator::CONTRACT,
                "tasks/inhibited_actuator.rs",
            ),
            (
                &selected_tasks::observe_button_change::CONTRACT,
                "tasks/observe_button_change.rs",
            ),
            (
                &selected_tasks::foxeer_control::CONTRACT,
                "apps/foxeer-f405-v2/tasks/foxeer_control.rs",
            ),
            (
                &selected_tasks::foxeer_safety_master::CONTRACT,
                "apps/foxeer-f405-v2/tasks/foxeer_safety_master.rs",
            ),
            (
                &selected_tasks::golden_flash::CONTRACT,
                "apps/foxeer-f405-v2/tasks/golden_flash.rs",
            ),
            (
                &selected_tasks::heartbeat::CONTRACT,
                "apps/foxeer-f405-v2/tasks/heartbeat.rs",
            ),
            (
                &selected_tasks::usb_cdc::CONTRACT,
                "apps/foxeer-f405-v2/tasks/usb_cdc.rs",
            ),
        ];

        for (contract, registered) in contracts {
            let compiled = Path::new(env!("CARGO_MANIFEST_DIR")).join(contract.source.file);
            assert_eq!(
                std::fs::canonicalize(compiled).unwrap(),
                std::fs::canonicalize(root.join(registered)).unwrap(),
                "{} did not retain its relocated file!() source",
                contract.id
            );
        }
    }
}
