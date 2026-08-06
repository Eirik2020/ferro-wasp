//! Registry of reusable HAL-agnostic task definitions and ordinary Rust bodies.

#[path = "blink_led.rs"]
mod blink_led_task;

pub use blink_led_task::blink_led;
