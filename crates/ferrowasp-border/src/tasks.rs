//! The border roles' tasks (design §16), target-only: the
//! observer's run every millisecond, publishing on the ledger (design §10, §19).
//!
//! Requirements: BORDER-005
//! Safety-critical.

use crate::observer::{Ledger, Observer};
use ferrowasp_io_contracts::time::TimestampMicros;
use fugit::ExtU64 as _;

/// Run the observer every millisecond, on absolute deadlines so its cadence
/// does not drift. Priority is the firmware's: the border band, below the
/// core and above every non-critical task.
#[ferroforge::task(
    local = [observer: Observer<'static>],
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
