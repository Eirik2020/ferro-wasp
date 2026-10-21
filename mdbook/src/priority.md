# Priority and Scheduling

Task priorities on the Foxeer F405 V2, the reference board; the TBS Lucid H7
uses the same levels. Higher numbers preempt lower ones. The safety-critical
core runs at 8 and above, the border roles at 6 and 7, and everything that
is not safety-critical at 5 and below, so no display, storage or host link
can delay flight work.

| Priority | Task | Type | What it does |
|---:|---|---|---|
| 16 | `safety_master` | SW | Arms and disarms; the only arming authority. |
| 16 | DShot DMA completions | HW | Finish each motor lane's frame. |
| 15 | `actuator_output` | SW | The only owner of motor output; applies the final safety gate. |
| 14 | `control_loop` | HW (timer) | The rate controller and mixer; publishes motor commands. |
| 14 | `imu_data_ready` | HW (EXTI) | The IMU's data-ready line; starts a sample read. |
| 13 | `dshot_service`, `dshot_fallback`, `actuator_idle_notify` | SW | Start DShot frame sets; relay idle preparation to the safety master. |
| 13 | SPI1 owner, receive DMA and timeout | SW, HW | Run the IMU's SPI transfers. |
| 12 | `spi1_poll`, `spi1_parser` | SW | Read and decode IMU samples. |
| 11 | UART transports | HW, SW | Receive and transmit DMA and idle-line events for every port. |
| 10 | `rc_input` | SW | Decodes the receiver and qualifies the RC link. |
| 9 | `io_watchdog` | HW (timer) | Recovers a stalled SPI transfer. |
| 8 | `esc_manager_task` | SW | Polls ESC telemetry over its return line. |
| 7 | `observer_task` | SW | Publishes the craft's state on the read-only ledger the tasks below read, and streams logged control ticks to the blackbox. |
| 5 | `usb_fs` | HW | The USB command line and status. |
| 3 | `osd_refresh`, `configurator_link` | SW | The MSP OSD; the command line on a UART. |
| 1 | `flash_manager_task`, `heartbeat`, `rc_telemetry`, ADC | SW, HW | Storage and the blackbox, diagnostics, receiver telemetry, battery sensing. |
