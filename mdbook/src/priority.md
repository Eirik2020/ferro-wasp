# Priority and Scheduling

| Priority | Task name | Type | Description |
|---:|---|---|---|
| 16 | Safety Master | SW | Highest-level safety authority. Handles kill/disarm state, failsafe decisions, watchdog supervision, and emergency shutdown. |
| 15 | Actuator Output | SW | Sole owner of motor/ESC outputs. Applies the final safety gate and writes DShot, PWM, OneShot, or other actuator commands. |
| 14 | Control Loop | SW | Runs rate/attitude control, mixing, and command limiting, and produces desired actuator outputs. |
| 13 | Actuator Idle Notifier | SW | Relays completed idle preparation after the actuator executor has returned, allowing final safety checks and forced-low fallback. |
| 13 | SPI HW | HW | Handles SPI interrupt/DMA completion for high-rate sensors, especially IMU transfers. Keeps ISR work short and bounded. |
| 12 | IMU Data | SW | Converts raw IMU samples into calibrated gyro/accelerometer data and updates the latest control-loop input state. |
| 11 | UART HW | HW | Handles UART RX/TX interrupt, DMA, and idle-line events. Captures byte buffers and wakes protocol parsers. |
| 10 | RC Input | SW | Parses CRSF, SBUS, ELRS, IBUS, or similar receiver protocols and updates normalized pilot setpoints. |
| 9 | I2C HW | HW | Handles I2C interrupt/DMA completion for lower-rate sensors or peripherals such as barometers, magnetometers, or expanders. |
| 9 | ADC HW | HW | Handles ADC conversion/DMA completion for battery voltage, current, temperature, RSSI, or other analog inputs. |
| 4 | OSD | SW | Updates display/status data for analog or digital FPV systems. Non-flight-critical. |
| 3 | Error Handler | SW | Processes non-critical faults, degraded-mode reports, and diagnostic events after safety-critical handling is complete. |
| 2 | Debugger | SW | Used for debugging without clogging up critical sections |
| 1 | Telemetry | SW | Sends status, sensor data, link statistics, logs, and debug information. Lowest-priority background communication. |
