//! The observer's task: one `ferrowasp_border::observer` run every
//! millisecond, publishing on the ledger (ADR-0056, ADR-0061).

use crate::prelude::*;

/// Run the observer every millisecond, on absolute deadlines so its cadence
/// does not drift. Priority is the firmware's: the border band, below the
/// core and above every non-critical task.
#[ferroforge::task(
    local = [observer: ferrowasp_border::observer::Observer<'static>],
    shared = [ledger: Ledger],
    monotonic = Mono,
)]
pub async fn observer_task(mut cx: observer_task::Context) {
    let mut next_run = Mono::now();
    loop {
        let now = TimestampMicros(next_run.duration_since_epoch().to_micros());
        cx.shared
            .ledger
            .lock(|ledger| cx.local.observer.run_once(now, ledger));
        next_run += 1u64.millis();
        Mono::delay_until(next_run).await;
    }
}
