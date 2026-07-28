//! Reusable asynchronous arming-hold validation.

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
