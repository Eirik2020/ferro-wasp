/// Frequency used by the firmware's microsecond-resolution blocking delay timer.
pub const DELAY_TIMER_HZ: u32 = 1_000_000;

/// Poll cadence while revalidating live arming guards during a bounded hold.
pub const ARMING_GUARD_POLL_MS: u32 = 10;

/// Service cadence for the legacy ESC telemetry manager.
pub const ESC_MANAGER_PERIOD_MS: u32 = 2;

/// Maximum command accepted by capped motor commissioning modes.
pub const BENCH_EQUAL_MOTOR_MAX_THROTTLE: f32 = 250.0;

/// Period of the non-actuator NUCLEO bring-up heartbeat.
pub const BRINGUP_HEARTBEAT_PERIOD_MS: u32 = 1_000;

const _: () = {
    assert!(DELAY_TIMER_HZ > 0);
    assert!(ARMING_GUARD_POLL_MS > 0);
    assert!(ESC_MANAGER_PERIOD_MS > 0);
    assert!(BENCH_EQUAL_MOTOR_MAX_THROTTLE > 0.0);
    assert!(BRINGUP_HEARTBEAT_PERIOD_MS > 0);
};
