// GENERATED FILE — DO NOT EDIT DIRECTLY
// Lowered from the app-owned live_config.rs selection.

//! Bounded live-configuration defaults for the generated firmware.

use ferrowasp_tasks::{drone_toolbox::TuningProfile, flash_storage::StoredConfig};

pub(crate) const INITIAL_TUNING_REQUEST_SEQ: u32 = 1;
pub(crate) const INITIAL_FLASH_LOG_RATE_DIVISOR: u32 = 1;

pub(crate) const fn initial_tuning_profile() -> TuningProfile {
    TuningProfile::default_foxeer_f405_v2()
}

pub(crate) const fn initial_stored_config() -> StoredConfig {
    StoredConfig::foxeer_f405_v2_default()
}
