//! A board's status LED behind embedded-hal, so a shared task can light it
//! without naming the chip family, the pin, or which level lights it.

use core::convert::Infallible;
use embedded_hal::digital::OutputPin;

/// The pin level that lights the LED.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LedPolarity {
    ActiveHigh,
    ActiveLow,
}

pub struct StatusLed {
    pin: &'static mut dyn OutputPin<Error = Infallible>,
    polarity: LedPolarity,
}

impl StatusLed {
    /// Take the pin and turn the LED off.
    pub fn new(pin: &'static mut dyn OutputPin<Error = Infallible>, polarity: LedPolarity) -> Self {
        let mut led = Self { pin, polarity };
        led.set(false);
        led
    }

    pub fn set(&mut self, lit: bool) {
        let Ok(()) = if lit == (self.polarity == LedPolarity::ActiveHigh) {
            self.pin.set_high()
        } else {
            self.pin.set_low()
        };
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::cell::Cell;
    use embedded_hal::digital::ErrorType;
    use std::{boxed::Box, rc::Rc};

    /// The pin's last level, shared with the test.
    struct Level(Rc<Cell<Option<bool>>>);

    impl ErrorType for Level {
        type Error = Infallible;
    }

    impl OutputPin for Level {
        fn set_low(&mut self) -> Result<(), Infallible> {
            self.0.set(Some(false));
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Infallible> {
            self.0.set(Some(true));
            Ok(())
        }
    }

    fn led(polarity: LedPolarity) -> (StatusLed, Rc<Cell<Option<bool>>>) {
        let level = Rc::new(Cell::new(None));
        let pin: &'static mut Level = Box::leak(Box::new(Level(level.clone())));
        (StatusLed::new(pin, polarity), level)
    }

    #[test]
    fn an_active_low_led_starts_off_with_its_pin_high_and_lights_low() {
        let (mut led, level) = led(LedPolarity::ActiveLow);
        assert_eq!(level.get(), Some(true));
        led.set(true);
        assert_eq!(level.get(), Some(false));
    }

    #[test]
    fn an_active_high_led_starts_off_with_its_pin_low_and_lights_high() {
        let (mut led, level) = led(LedPolarity::ActiveHigh);
        assert_eq!(level.get(), Some(false));
        led.set(true);
        assert_eq!(level.get(), Some(true));
    }
}
