//! Example composition using reusable GPIO and full-duplex UART4 tasks.
//!
//! This declaration is authoring data only. It deliberately does not generate
//! or contain an `#[rtic::app]` module.

use ferrowasp_io_core::serial::SerialProfile;
use fugit::MillisDurationU32;
use stm32f4xx_hal::pac::Interrupt;

use super::board;

use crate::{
    hardware_definitions::stm32f4::{
        hw_endpoint::serial_endpoint::{SERIAL_DMA_ENDPOINT, SerialEndpointDeclaration},
        tasks as stm32f4_tasks,
    },
    rtic::{
        component::ComponentDeclaration,
        composition::{AppComposition, SharedResourceDeclaration, TaskDeclaration},
        timing::MonotonicDeclaration,
    },
    tasks,
};

/// Application-owned state toggled by the button interrupt task.
pub const BUTTON_ENABLED: SharedResourceDeclaration =
    SharedResourceDeclaration::bool("button_enabled", false);

/// Concrete EXTI instance of the reusable button task.
///
/// The selected board must provide `user_button` as a concrete resource that
/// implements `stm32f4xx_hal::gpio::ExtiPin`. Composition validation resolves
/// the identifier against the selected board before generation begins.
pub const BUTTON_EXTI_TASK: TaskDeclaration = TaskDeclaration::interrupt(
    "button_exti",
    &stm32f4_tasks::button_exti::CONTRACT,
    Interrupt::EXTI15_10,
)
.priority(2)
.with_local(&[stm32f4_tasks::button_exti::LOCAL.button.bind("user_button")])
.with_shared(&[stm32f4_tasks::button_exti::SHARED
    .enabled
    .bind("button_enabled")])
.with_config(&[stm32f4_tasks::button_exti::CONFIG.toggle_on_press.set(true)])
.with_spawns(&[stm32f4_tasks::button_exti::SPAWNS
    .button_changed
    .bind("observe_button_change")]);

/// Persistent software task that owns and blinks LED2.
pub const BLINK_LED_TASK: TaskDeclaration =
    TaskDeclaration::software("blink_led", &tasks::blink_led::CONTRACT)
        .priority(1)
        .with_local(&[tasks::blink_led::LOCAL.led.bind("led2")])
        .with_shared(&[tasks::blink_led::SHARED.enabled.bind("button_enabled")])
        .with_config(&[tasks::blink_led::CONFIG
            .interval
            .set(MillisDurationU32::millis(500))]);

/// Software task receiving the state produced by the button interrupt.
pub const OBSERVE_BUTTON_CHANGE_TASK: TaskDeclaration = TaskDeclaration::software(
    "observe_button_change",
    &tasks::observe_button_change::CONTRACT,
)
.priority(1);

/// Full-duplex MSP endpoint consuming the UART4 declaration from `board.rs`.
pub const OSD_UART: SerialEndpointDeclaration = SERIAL_DMA_ENDPOINT
    .declare("osd_uart", "uart4")
    .profile(SerialProfile::msp())
    .bidirectional()
    .interrupt_priority(6)
    .worker_priority(4)
    .rx_buffer_count(4)
    .rx_queue_depth(4)
    .tx_queue_depth(16);

/// Complete example consumed by composition validation and, later, xtask.
pub const APP_COMPOSITION: AppComposition = AppComposition {
    board: &board::BOARD,
    monotonic: MonotonicDeclaration::systick(1_000),
    components: &[ComponentDeclaration::serial_endpoint(OSD_UART)],
    shared_resources: &[BUTTON_ENABLED],
    tasks: &[BUTTON_EXTI_TASK, BLINK_LED_TASK, OBSERVE_BUTTON_CHANGE_TASK],
    init_spawns: &[BLINK_LED_TASK.init_spawn()],
};
