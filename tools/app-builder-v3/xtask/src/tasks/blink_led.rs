use core::convert::Infallible;

use fugit::MillisDurationU32;
use stm32f4xx_hal::hal::digital::StatefulOutputPin;

use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// LED output owned by this software task instance.
            led: dyn StatefulOutputPin<Error = Infallible>,
        }
        shared {
            /// Shared gate controlled by the button interrupt task.
            enabled: bool,
        }
        config {
            /// Delay between LED state updates.
            interval: MillisDurationU32,
        }
        spawns {}
    }

    /// Blinks its local LED while the shared enabled flag is set.
    pub async fn blink_led(mut cx: blink_led::Context<'_>) {
        loop {
            let enabled = cx.shared.enabled.lock(|enabled| *enabled);
            if enabled {
                let _ = cx.local.led.toggle();
            } else {
                let _ = cx.local.led.set_low();
            }

            Mono::delay(cx.config.interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use core::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    use super::*;
    use stm32f4xx_hal::hal::digital::{ErrorType, OutputPin};

    struct FakeLed {
        high: bool,
    }

    impl ErrorType for FakeLed {
        type Error = Infallible;
    }

    impl OutputPin for FakeLed {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.high = false;
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.high = true;
            Ok(())
        }
    }

    impl StatefulOutputPin for FakeLed {
        fn is_set_high(&mut self) -> Result<bool, Self::Error> {
            Ok(self.high)
        }

        fn is_set_low(&mut self) -> Result<bool, Self::Error> {
            Ok(!self.high)
        }
    }

    fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        future.as_mut().poll(&mut context)
    }

    #[test]
    fn enabled_task_toggles_led_before_waiting() {
        let mut led = FakeLed { high: false };
        let mut enabled = true;
        {
            let context = blink_led::Context::new(
                blink_led::Local::new(&mut led),
                blink_led::Shared::new(&mut enabled),
                blink_led::Config::new(MillisDurationU32::millis(500)),
            );

            assert!(poll_once(blink_led(context)).is_pending());
        }

        assert!(led.high);
    }

    #[test]
    fn disabled_task_drives_led_low_before_waiting() {
        let mut led = FakeLed { high: true };
        let mut enabled = false;
        {
            let context = blink_led::Context::new(
                blink_led::Local::new(&mut led),
                blink_led::Shared::new(&mut enabled),
                blink_led::Config::new(MillisDurationU32::millis(500)),
            );

            assert!(poll_once(blink_led(context)).is_pending());
        }

        assert!(!led.high);
    }
}
