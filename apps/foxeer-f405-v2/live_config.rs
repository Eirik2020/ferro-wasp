//! Foxeer-owned selection of the existing bounded live-configuration APIs.
//!
//! This host-compiled App Builder input does not define a schema or persistence
//! algorithm. The shared FerroWasp crates remain authoritative for both.

use ferrowasp_core::config::{ConfigKey, ConfigValueSpec};
use ferrowasp_tasks::{drone_toolbox::TuningProfile, flash_storage::StoredConfig};

/// Initial publication sequence before persisted configuration recovery.
pub const INITIAL_TUNING_REQUEST_SEQ: u32 = 1;
/// Initial blackbox divisor before persisted configuration recovery.
pub const INITIAL_FLASH_LOG_RATE_DIVISOR: u32 = 1;

/// Returns the reviewed Foxeer fallback tuning profile.
pub const fn initial_tuning_profile() -> TuningProfile {
    TuningProfile::default_foxeer_f405_v2()
}

/// Returns the existing Foxeer stored-configuration default.
pub const fn initial_stored_config() -> StoredConfig {
    StoredConfig::foxeer_f405_v2_default()
}

/// Returns shared firmware-owned bounds for a public configuration key.
pub const fn value_spec(key: ConfigKey) -> ConfigValueSpec {
    key.value_spec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selections_match_the_pinned_live_configuration_contract() {
        let stored = initial_stored_config();
        assert_eq!(stored.tuning, initial_tuning_profile());
        assert_eq!(stored.log_rate_divisor, 1);
        assert_eq!(INITIAL_TUNING_REQUEST_SEQ, 1);
        assert_eq!(INITIAL_FLASH_LOG_RATE_DIVISOR, 1);
        assert_eq!(ConfigKey::ALL.len(), 21);
        for key in ConfigKey::ALL {
            assert_eq!(value_spec(key), key.value_spec());
        }
    }
}
