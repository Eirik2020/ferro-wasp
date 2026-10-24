//! The border roles' tasks (design §16), target-only: the observer's run
//! every millisecond, publishing on the ledger (design §10, §19), and the
//! coordinator's every ten (design §11).
//!
//! Requirements: BORDER-005, BORDER-011, BORDER-017
//! Safety-critical.

use crate::coordinator::Coordinator;
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

/// How often the coordinator takes requests in.
pub const COORDINATOR_PERIOD_MS: u64 = 10;

/// Run the coordinator every ten milliseconds, on absolute deadlines. It
/// hands what it accepts to the safety master by spawning it; a spawn the
/// safety master cannot take yet is offered again on the next run. Priority
/// is the firmware's: the border band, below the observer.
#[ferroforge::task(
    local = [coordinator: Coordinator<'static>],
    spawn = [safety_master(event: ferrowasp_safety::SafetyEvent)],
    monotonic = Mono,
)]
pub async fn coordinator_task(cx: coordinator_task::Context) {
    let mut next_run = Mono::now();
    loop {
        let now_ms = next_run.duration_since_epoch().to_millis();
        let spawn = &cx.spawn;
        cx.local
            .coordinator
            .run_once(now_ms, |event| spawn.safety_master(event).is_ok());
        next_run += COORDINATOR_PERIOD_MS.millis();
        Mono::delay_until(next_run).await;
    }
}
