//! STM32H743 clock tree for a flight board.

use stm32h7xx_hal::{
    pac,
    prelude::*,
    rcc::{
        Ccdr,
        rec::{AdcClkSel, UsbClkSel},
    },
};

/// 400 MHz: the H743's full rate at voltage scale 1, which every silicon
/// revision supports. 480 MHz needs revision V and VOS0.
pub const SYSTEM_CLOCK_HZ: u32 = 400_000_000;
/// AHB, and through the APB prescalers the APB1/APB2 timer clocks.
pub const HCLK_HZ: u32 = 200_000_000;
/// PLL1 Q feeds the SPI1-3 and SDMMC kernel clocks.
pub const PLL1_Q_HZ: u32 = 100_000_000;

/// Freeze the clock tree from an external crystal: the core at
/// `SYSTEM_CLOCK_HZ`, SPI and SDMMC from PLL1 Q, USB from HSI48, and the ADC
/// from the 64 MHz HSI through `per_ck`.
pub fn freeze_hse(pwr: pac::PWR, rcc: pac::RCC, syscfg: &pac::SYSCFG, hse_hz: u32) -> Ccdr {
    let pwrcfg = pwr.constrain().freeze();
    let mut ccdr = rcc
        .constrain()
        .use_hse(hse_hz.Hz())
        .sys_ck(SYSTEM_CLOCK_HZ.Hz())
        .hclk(HCLK_HZ.Hz())
        .pll1_q_ck(PLL1_Q_HZ.Hz())
        .freeze(pwrcfg, syscfg);
    ccdr.peripheral.kernel_usb_clk_mux(UsbClkSel::Hsi48);
    ccdr.peripheral.kernel_adc_clk_mux(AdcClkSel::Per);
    ccdr
}
