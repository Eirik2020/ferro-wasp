//! The board's status LEDs: a heartbeat that shows the scheduler is running,
//! and the arm state.

use crate::status_led::StatusLed;
use ferrowasp_safety::signals;
use fugit::ExtU64 as _;

/// One LED update every 100 ms; the heartbeat spends five of them lit and
/// five dark, a 1 Hz blink.
const STATUS_LED_TICK_MS: u64 = 100;
const HEARTBEAT_PERIOD_TICKS: u8 = 10;

/// Blink the heartbeat LED and light the armed LED while the safety master
/// reports the aircraft armed. Reads the arm state and writes nothing but
/// the two LEDs, so it cannot affect arming or the motors.
#[ferroforge::task(
    local = [
        heartbeat_led: StatusLed,
        armed_led: StatusLed,
        led_safety_arm_reader: signals::SafetyArmReader,
    ],
    monotonic = Mono,
)]
pub async fn status_leds(cx: status_leds::Context) {
    let mut tick: u8 = 0;

    loop {
        cx.local
            .armed_led
            .set(cx.local.led_safety_arm_reader.read());
        cx.local
            .heartbeat_led
            .set(tick < HEARTBEAT_PERIOD_TICKS / 2);
        tick = (tick + 1) % HEARTBEAT_PERIOD_TICKS;
        Mono::delay(STATUS_LED_TICK_MS.millis()).await;
    }
}
