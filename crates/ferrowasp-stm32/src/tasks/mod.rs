//! The non-critical hardware's RTIC task definitions (design §16),
//! target-only: battery sensing on ADC1, the status LEDs, and the onboard
//! barometer.

pub mod adc;
// The barometer task carries an I2C bus, which only a selected chip has.
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod baro;
pub mod leds;
