//! Registry of reusable HAL-agnostic task definitions and ordinary Rust bodies.

#[path = "blink_led.rs"]
mod blink_led_task;
#[path = "observe_button_change.rs"]
mod observe_button_change_task;

pub use blink_led_task::blink_led;
pub use observe_button_change_task::observe_button_change;
