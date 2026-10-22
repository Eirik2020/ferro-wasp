//! Reusable asynchronous arming-hold validation.
//!
//! Requirements: ARM-001, ARM-004
//! Safety-critical.

use core::future::Future;

/// Revalidates an arming guard throughout a bounded hold interval.
pub async fn wait_hold<E, Check, Delay, DelayFuture>(
    hold_ms: u32,
    poll_ms: u32,
    mut check: Check,
    mut delay: Delay,
) -> Result<(), E>
where
    Check: FnMut() -> Result<(), E>,
    Delay: FnMut(u32) -> DelayFuture,
    DelayFuture: Future<Output = ()>,
{
    let mut remaining_ms = hold_ms;
    let poll_ms = poll_ms.max(1);

    while remaining_ms != 0 {
        check()?;
        let delay_ms = remaining_ms.min(poll_ms);
        delay(delay_ms).await;
        remaining_ms -= delay_ms;
    }

    check()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use core::future::ready;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};

    fn run<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
        }
    }

    #[test]
    fn the_guard_is_checked_through_the_whole_hold_and_once_at_its_end() {
        let checks = Cell::new(0);
        let waited = Cell::new(0);
        let result: Result<(), ()> = run(wait_hold(
            250,
            100,
            || {
                checks.set(checks.get() + 1);
                Ok(())
            },
            |ms| {
                waited.set(waited.get() + ms);
                ready(())
            },
        ));
        assert_eq!(result, Ok(()));
        assert_eq!(waited.get(), 250, "the hold lasts exactly its length");
        assert_eq!(
            checks.get(),
            4,
            "before each of three polls and once at the end"
        );
    }

    #[test]
    fn a_guard_that_fails_during_the_hold_ends_it_at_once() {
        let checks = Cell::new(0);
        let result = run(wait_hold(
            1_000,
            100,
            || {
                checks.set(checks.get() + 1);
                if checks.get() == 3 {
                    Err("throttle raised")
                } else {
                    Ok(())
                }
            },
            |_| ready(()),
        ));
        assert_eq!(result, Err("throttle raised"));
        assert_eq!(checks.get(), 3);
    }

    #[test]
    fn a_zero_poll_interval_still_makes_progress() {
        let waited = Cell::new(0);
        let result: Result<(), ()> = run(wait_hold(
            3,
            0,
            || Ok(()),
            |ms| {
                waited.set(waited.get() + ms);
                ready(())
            },
        ));
        assert_eq!(result, Ok(()));
        assert_eq!(waited.get(), 3);
    }
}
