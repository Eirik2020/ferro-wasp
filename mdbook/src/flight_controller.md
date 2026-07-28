# Flight Controller

This section documents the reusable flight-control and actuator paths:

- [IMU](imu.md) covers supported sensors, sampling, orientation, and
  freshness.
- [Arming Sequence](arming.md) covers the guarded transition from disarmed to
  active output.
- [DShot](dshot.md) covers the standard ESC protocol and four-lane STM32F4
  backend.
- [Priority and Scheduling](priority.md) describes the RTIC priority model.

Board availability and limitations are tracked in
[Current Support](current_support.md).
