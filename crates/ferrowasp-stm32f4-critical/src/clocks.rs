pub const SYSTEM_CLOCK_HZ: u32 = 168_000_000;
pub const STM32F401_SYSTEM_CLOCK_HZ: u32 = 84_000_000;
pub const CONTROL_SCHEDULER_HZ: u32 = 800;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockPlan {
    pub system_hz: u32,
    pub control_scheduler_hz: u32,
    pub requires_pll48: bool,
}

pub const DEFAULT_CLOCK_PLAN: ClockPlan = ClockPlan {
    system_hz: SYSTEM_CLOCK_HZ,
    control_scheduler_hz: CONTROL_SCHEDULER_HZ,
    requires_pll48: false,
};

#[cfg(all(target_arch = "arm", any(feature = "stm32f401", feature = "stm32f405")))]
pub fn freeze_hsi(
    rcc: stm32f4xx_hal::rcc::Rcc,
    system_hz: u32,
    requires_pll48: bool,
) -> stm32f4xx_hal::rcc::Rcc {
    use stm32f4xx_hal::prelude::*;

    let config = stm32f4xx_hal::rcc::Config::hsi().sysclk(system_hz.Hz());
    let config = if requires_pll48 {
        config.require_pll48clk()
    } else {
        config
    };
    rcc.freeze(config)
}

#[cfg(all(target_arch = "arm", any(feature = "stm32f401", feature = "stm32f405")))]
pub fn freeze_hse(
    rcc: stm32f4xx_hal::rcc::Rcc,
    hse_hz: u32,
    system_hz: u32,
    requires_pll48: bool,
) -> stm32f4xx_hal::rcc::Rcc {
    use stm32f4xx_hal::prelude::*;

    let config = stm32f4xx_hal::rcc::Config::hse(hse_hz.Hz()).sysclk(system_hz.Hz());
    let config = if requires_pll48 {
        config.require_pll48clk()
    } else {
        config
    };
    rcc.freeze(config)
}
