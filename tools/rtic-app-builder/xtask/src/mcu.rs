use anyhow::{Result, bail};

/// Backend-owned build facts for one Betaflight-style MCU target.
///
/// `STM32F401` is a compatibility profile, not a claim that every package or
/// density in the STM32F401 product line has this exact memory layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct McuProfile {
    pub id: &'static str,
    pub family: &'static str,
    pub rust_target: &'static str,
    pub hal_crate: &'static str,
    pub hal_feature: &'static str,
    pub pac_path: &'static str,
    pub probe_rs_chip: &'static str,
    pub flash_origin: u64,
    pub flash_size_bytes: u64,
    pub ram_origin: u64,
    pub ram_size_bytes: u64,
}

pub const STM32F401: McuProfile = McuProfile {
    id: "STM32F401",
    family: "stm32f4",
    rust_target: "thumbv7em-none-eabihf",
    hal_crate: "stm32f4xx-hal",
    hal_feature: "stm32f401",
    pac_path: "stm32f4xx_hal::pac",
    probe_rs_chip: "STM32F401RE",
    flash_origin: 0x0800_0000,
    flash_size_bytes: 524_288,
    ram_origin: 0x2000_0000,
    ram_size_bytes: 98_304,
};

pub fn profile(mcu: &str) -> Result<&'static McuProfile> {
    match mcu {
        "STM32F401" => Ok(&STM32F401),
        _ => bail!("unsupported value at `bsp.mcu`: no backend MCU profile exists for `{mcu}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_the_betaflight_style_family_name() {
        let resolved = profile("STM32F401").unwrap();
        assert_eq!(resolved.hal_feature, "stm32f401");
        assert_eq!(resolved.flash_size_bytes, 524_288);
        assert_eq!(resolved.ram_size_bytes, 98_304);
    }

    #[test]
    fn rejects_unregistered_and_exact_part_names() {
        for unsupported in ["STM32F405", "STM32F401RET6", "stm32f401"] {
            let error = profile(unsupported).unwrap_err().to_string();
            assert!(error.contains("bsp.mcu"), "unexpected error: {error}");
        }
    }
}
