# Generated output-disabled physical safety spine

> Generated review artifact. Do not edit directly. This graph declares the reviewed motor and ESC-telemetry hardware, but the safety policy and physical component independently disable output. Generation, compilation, and linking are not target, electrical, bench, or flight evidence.

Board: `foxeer_f405_v2`

## Safety-authority map

| Task | Priority | Authority boundary |
| --- | ---: | --- |
| `actuator_fault_reporter` | 16 | safety-critical |
| `actuator_output` | 15 | safety-critical |
| `control_loop` | 14 | safety-critical |
| `dshot_motor1_dma_complete` | 16 | safety-critical |
| `dshot_motor2_dma_complete` | 16 | safety-critical |
| `dshot_motor3_dma_complete` | 16 | safety-critical |
| `dshot_motor4_dma_complete` | 16 | safety-critical |
| `dshot_service` | 13 | safety-critical |
| `esc_manager` | 4 | safety-critical |
| `imu_control_bridge` | 11 | safety-critical |
| `safety_master` | 16 | safety-critical |

Only `safety_master` owns arming state and the actuator permit. `control_loop` may publish typed requests but cannot grant authority. `actuator_output` owns the sole motor-command consumer and is the only task that may translate a fresh armed request into a bank command. Both selected enable gates are false.

## Task and priority map

| Task | Entry | Priority | Safety class | Component | Init-spawned |
| --- | --- | ---: | --- | --- | --- |
| `actuator_fault_reporter` | software spawn | 16 | safety-critical | `standalone` | no |
| `actuator_output` | software spawn | 15 | safety-critical | `standalone` | no |
| `adc_observation_dma` | interrupt `DMA2_STREAM4` | 1 | non-safety-critical | `standalone` | no |
| `adc_observation_poll` | software spawn | 1 | non-safety-critical | `standalone` | yes |
| `control_loop` | interrupt `TIM4` | 14 | safety-critical | `control` | no |
| `dshot_motor1_dma_complete` | interrupt `DMA2_STREAM1` | 16 | safety-critical | `standalone` | no |
| `dshot_motor2_dma_complete` | interrupt `DMA2_STREAM7` | 16 | safety-critical | `standalone` | no |
| `dshot_motor3_dma_complete` | interrupt `DMA2_STREAM2` | 16 | safety-critical | `standalone` | no |
| `dshot_motor4_dma_complete` | interrupt `DMA2_STREAM6` | 16 | safety-critical | `standalone` | no |
| `dshot_service` | software spawn | 13 | safety-critical | `standalone` | yes |
| `esc_manager` | software spawn | 4 | safety-critical | `standalone` | yes |
| `esc_uart_rx_dma` | interrupt `DMA2_STREAM5` | 5 | non-safety-critical | `standalone` | no |
| `esc_uart_rx_idle` | interrupt `USART1` | 5 | non-safety-critical | `standalone` | no |
| `golden_flash` | software spawn | 1 | non-safety-critical | `standalone` | yes |
| `heartbeat` | software spawn | 1 | non-safety-critical | `standalone` | yes |
| `imu_control_bridge` | software spawn | 11 | safety-critical | `standalone` | yes |
| `io_watchdog` | interrupt `TIM6_DAC` | 9 | non-safety-critical | `standalone` | no |
| `msp_osd` | software spawn | 3 | non-safety-critical | `standalone` | yes |
| `safety_master` | software spawn | 16 | safety-critical | `standalone` | yes |
| `serial1_rx_bridge` | software spawn | 12 | non-safety-critical | `serial1` | no |
| `serial1_rx_dma_irq` | interrupt `DMA1_STREAM5` | 13 | non-safety-critical | `serial1` | no |
| `serial1_rx_idle_irq` | interrupt `USART2` | 13 | non-safety-critical | `serial1` | no |
| `serial1_tx_dma_irq` | interrupt `DMA1_STREAM6` | 13 | non-safety-critical | `serial1` | no |
| `serial1_tx_worker` | software spawn | 4 | non-safety-critical | `serial1` | yes |
| `serial2_rx_bridge` | software spawn | 4 | non-safety-critical | `serial2` | no |
| `serial2_rx_dma_irq` | interrupt `DMA1_STREAM2` | 6 | non-safety-critical | `serial2` | no |
| `serial2_rx_idle_irq` | interrupt `UART4` | 6 | non-safety-critical | `serial2` | no |
| `serial2_tx_dma_irq` | interrupt `DMA1_STREAM4` | 6 | non-safety-critical | `serial2` | no |
| `serial2_tx_worker` | software spawn | 4 | non-safety-critical | `serial2` | yes |
| `spi1_data_ready` | interrupt `EXTI4` | 14 | non-safety-critical | `spi1` | no |
| `spi1_owner_service` | software spawn | 13 | non-safety-critical | `spi1` | no |
| `spi1_parser` | software spawn | 11 | non-safety-critical | `spi1` | no |
| `spi1_poll` | software spawn | 12 | non-safety-critical | `spi1` | no |
| `spi1_rx_dma_irq` | interrupt `DMA2_STREAM0` | 13 | non-safety-critical | `spi1` | no |
| `spi1_timeout` | software spawn | 13 | non-safety-critical | `spi1` | no |
| `usb_cdc` | interrupt `OTG_FS` | 5 | non-safety-critical | `standalone` | no |

## Interrupt and dispatcher map

| Vector | Owner task | Priority |
| --- | --- | ---: |
| `DMA1_STREAM2` | `serial2_rx_dma_irq` | 6 |
| `DMA1_STREAM4` | `serial2_tx_dma_irq` | 6 |
| `DMA1_STREAM5` | `serial1_rx_dma_irq` | 13 |
| `DMA1_STREAM6` | `serial1_tx_dma_irq` | 13 |
| `DMA2_STREAM0` | `spi1_rx_dma_irq` | 13 |
| `DMA2_STREAM1` | `dshot_motor1_dma_complete` | 16 |
| `DMA2_STREAM2` | `dshot_motor3_dma_complete` | 16 |
| `DMA2_STREAM4` | `adc_observation_dma` | 1 |
| `DMA2_STREAM5` | `esc_uart_rx_dma` | 5 |
| `DMA2_STREAM6` | `dshot_motor4_dma_complete` | 16 |
| `DMA2_STREAM7` | `dshot_motor2_dma_complete` | 16 |
| `EXTI4` | `spi1_data_ready` | 14 |
| `OTG_FS` | `usb_cdc` | 5 |
| `TIM4` | `control_loop` | 14 |
| `TIM6_DAC` | `io_watchdog` | 9 |
| `UART4` | `serial2_rx_idle_irq` | 6 |
| `USART1` | `esc_uart_rx_idle` | 5 |
| `USART2` | `serial1_rx_idle_irq` | 13 |

Software dispatchers: `EXTI0`, `EXTI1`, `EXTI2`, `EXTI3`, `CAN1_TX`, `CAN2_TX`, `CAN1_RX0`, `CAN1_RX1`.

## Spawn map

| Source | Destination | Arguments |
| --- | --- | --- |
| `init` | `adc_observation_poll` | none |
| `init` | `dshot_service` | none |
| `init` | `esc_manager` | none |
| `init` | `golden_flash` | none |
| `init` | `heartbeat` | none |
| `init` | `imu_control_bridge` | none |
| `init` | `msp_osd` | none |
| `init` | `safety_master` | none |
| `init` | `serial1_tx_worker` | none |
| `init` | `serial2_tx_worker` | none |
| `actuator_output` | `actuator_fault_reporter` | `report: ferrowasp_core::safety::ActuatorPreparationReport` |
| `control_loop` | `actuator_output` | `request: ferrowasp_core::safety::ActuatorCmd` |
| `dshot_motor1_dma_complete` | `actuator_fault_reporter` | `report: ferrowasp_core::safety::ActuatorPreparationReport` |
| `dshot_motor2_dma_complete` | `actuator_fault_reporter` | `report: ferrowasp_core::safety::ActuatorPreparationReport` |
| `dshot_motor3_dma_complete` | `actuator_fault_reporter` | `report: ferrowasp_core::safety::ActuatorPreparationReport` |
| `dshot_motor4_dma_complete` | `actuator_fault_reporter` | `report: ferrowasp_core::safety::ActuatorPreparationReport` |
| `dshot_service` | `actuator_fault_reporter` | `report: ferrowasp_core::safety::ActuatorPreparationReport` |
| `esc_manager` | `actuator_fault_reporter` | `report: ferrowasp_core::safety::ActuatorPreparationReport` |
| `safety_master` | `actuator_output` | `request: ferrowasp_core::safety::ActuatorCmd` |
| `serial1_rx_dma_irq` | `serial1_rx_bridge` | none |
| `serial1_rx_idle_irq` | `serial1_rx_bridge` | none |
| `serial2_rx_dma_irq` | `serial2_rx_bridge` | none |
| `serial2_rx_idle_irq` | `serial2_rx_bridge` | none |
| `spi1_data_ready` | `spi1_poll` | `observed_at_us: u64` |
| `spi1_poll` | `spi1_timeout` | `observed_at_us: u64` |
| `spi1_rx_dma_irq` | `spi1_parser` | none |

## Authoritative safety-channel map

| Channel | Message | Usable capacity | Producer owner / handle | Consumer owner / handle |
| --- | --- | ---: | --- | --- |
| `actuator_fault_to_safety` | `ActuatorPreparation` | 3 | `actuator_fault_reporter` / `actuator_fault_producer` | `safety_master` / `actuator_fault_consumer` |
| `actuator_to_safety` | `ActuatorPreparation` | 3 | `actuator_output` / `actuator_completion_producer` | `safety_master` / `actuator_completion_consumer` |
| `control_to_actuator` | `MotorCommand` | 3 | `control_loop` / `motor_cmd_producer` | `actuator_output` / `motor_cmd_consumer` |
| `control_to_safety_health` | `PreArmHealth` | 4 | `control_loop` / `prearm_health_producer` | `safety_master` / `prearm_health_consumer` |
| `imu_to_control` | `ImuSample` | 4 | `imu_control_bridge` / `imu_control_producer` | `control_loop` / `imu_control_consumer` |
| `safety_to_actuator_guard` | `ActuatorGuard` | 3 | `safety_master` / `actuator_guard_producer` | `actuator_output` / `actuator_guard_consumer` |
| `sbus_to_control` | `SbusInput` | 4 | `safety_master` / `sbus_control_producer` | `control_loop` / `sbus_control_consumer` |

## Task resource map

| Task | Access | Logical name | Resolved resource |
| --- | --- | --- | --- |
| `actuator_fault_reporter` | local/exclusive | `reports` | `actuator_fault_producer` |
| `actuator_output` | local/exclusive | `commands` | `motor_cmd_consumer` |
| `actuator_output` | local/exclusive | `guards` | `actuator_guard_consumer` |
| `actuator_output` | local/exclusive | `completions` | `actuator_completion_producer` |
| `actuator_output` | local/exclusive | `telemetry` | `esc_telemetry_update_consumer` |
| `actuator_output` | shared/locked | `bank` | `dshot_motors` |
| `adc_observation_dma` | local/exclusive | `buffer` | `adc1_buffer` |
| `adc_observation_dma` | local/exclusive | `planner` | `adc1_planner` |
| `adc_observation_dma` | local/exclusive | `cell_detector` | `battery_cell_detector` |
| `adc_observation_dma` | shared/locked | `transfer` | `adc1_transfer` |
| `adc_observation_dma` | shared/locked | `telemetry` | `flight_service_telemetry` |
| `adc_observation_poll` | shared/locked | `transfer` | `adc1_transfer` |
| `control_loop` | local/exclusive | `scheduler` | `control_scheduler` |
| `control_loop` | local/exclusive | `phase` | `control_phase` |
| `control_loop` | local/exclusive | `sbus` | `sbus_control_consumer` |
| `control_loop` | local/exclusive | `imu` | `imu_control_consumer` |
| `control_loop` | local/exclusive | `motor_commands` | `motor_cmd_producer` |
| `control_loop` | local/exclusive | `prearm_health` | `prearm_health_producer` |
| `control_loop` | local/exclusive | `flash_records` | `flash_record_producer` |
| `control_loop` | local/exclusive | `control_loop_cnt` | `control_loop_cnt` |
| `control_loop` | local/exclusive | `samples_per_control_loop` | `samples_per_control_loop` |
| `control_loop` | local/exclusive | `flight_controller` | `flight_controller` |
| `control_loop` | local/exclusive | `imu_rate_filter` | `imu_rate_filter` |
| `control_loop` | local/exclusive | `imu_angle_integrator` | `imu_angle_integrator` |
| `control_loop` | local/exclusive | `gyro_axis_map` | `gyro_axis_map` |
| `control_loop` | local/exclusive | `gyro_bias_calibrator` | `gyro_bias_calibrator` |
| `control_loop` | local/exclusive | `imu_last_sequence` | `imu_last_sequence` |
| `control_loop` | local/exclusive | `imu_stale_ticks` | `imu_stale_ticks` |
| `control_loop` | local/exclusive | `applied_tuning_seq` | `applied_tuning_seq` |
| `control_loop` | local/exclusive | `motor_cmd_seq` | `motor_cmd_seq` |
| `control_loop` | local/exclusive | `rc_link_was_valid` | `rc_link_was_valid` |
| `control_loop` | local/exclusive | `latest_rc_input` | `latest_rc_input` |
| `control_loop` | local/exclusive | `rc_last_valid_frames` | `rc_last_valid_frames` |
| `control_loop` | local/exclusive | `rc_stale_ticks` | `rc_stale_ticks` |
| `control_loop` | local/exclusive | `control_was_armed` | `control_was_armed` |
| `control_loop` | shared/locked | `telemetry` | `flight_service_telemetry` |
| `control_loop` | shared/locked | `tuning` | `tuning_profile` |
| `control_loop` | shared/locked | `tuning_seq` | `tuning_request_seq` |
| `control_loop` | shared/locked | `log_divisor` | `flash_log_rate_divisor` |
| `control_loop` | shared/locked | `storage` | `storage_status` |
| `dshot_motor1_dma_complete` | shared/locked | `bank` | `dshot_motors` |
| `dshot_motor2_dma_complete` | shared/locked | `bank` | `dshot_motors` |
| `dshot_motor3_dma_complete` | shared/locked | `bank` | `dshot_motors` |
| `dshot_motor4_dma_complete` | shared/locked | `bank` | `dshot_motors` |
| `dshot_service` | local/exclusive | `requests` | `esc_request_consumer` |
| `dshot_service` | local/exclusive | `acknowledgements` | `esc_ack_producer` |
| `dshot_service` | local/exclusive | `pending_request` | `esc_actuator_request` |
| `dshot_service` | local/exclusive | `request_submitted` | `esc_actuator_request_submitted` |
| `dshot_service` | local/exclusive | `fault_reported` | `dshot_fault_reported` |
| `dshot_service` | shared/locked | `bank` | `dshot_motors` |
| `esc_manager` | local/exclusive | `parser` | `esc_telemetry_uart` |
| `esc_manager` | local/exclusive | `manager` | `esc_manager_state` |
| `esc_manager` | local/exclusive | `requests` | `esc_request_producer` |
| `esc_manager` | local/exclusive | `acknowledgements` | `esc_ack_consumer` |
| `esc_manager` | local/exclusive | `updates` | `esc_telemetry_update_producer` |
| `esc_manager` | local/exclusive | `report_ticks` | `esc_manager_report_ticks` |
| `esc_manager` | shared/locked | `discontinuity` | `esc_telemetry_discontinuity` |
| `esc_uart_rx_dma` | shared/locked | `uart` | `uart1_rx` |
| `esc_uart_rx_dma` | shared/locked | `discontinuity` | `esc_telemetry_discontinuity` |
| `esc_uart_rx_idle` | shared/locked | `uart` | `uart1_rx` |
| `esc_uart_rx_idle` | shared/locked | `discontinuity` | `esc_telemetry_discontinuity` |
| `golden_flash` | local/exclusive | `flash` | `flash_device` |
| `golden_flash` | local/exclusive | `records` | `flash_record_consumer` |
| `golden_flash` | local/exclusive | `commands` | `flash_command_consumer` |
| `golden_flash` | local/exclusive | `responses` | `flash_response_producer` |
| `golden_flash` | local/exclusive | `state` | `flash_manager_state` |
| `golden_flash` | shared/locked | `telemetry` | `flight_service_telemetry` |
| `golden_flash` | shared/locked | `tuning` | `tuning_profile` |
| `golden_flash` | shared/locked | `tuning_seq` | `tuning_request_seq` |
| `golden_flash` | shared/locked | `log_divisor` | `flash_log_rate_divisor` |
| `golden_flash` | shared/locked | `status` | `storage_status` |
| `heartbeat` | shared/locked | `telemetry` | `flight_service_telemetry` |
| `heartbeat` | shared/locked | `storage` | `storage_status` |
| `heartbeat` | shared/locked | `usb_status_due` | `usb_status_due` |
| `imu_control_bridge` | local/exclusive | `control` | `imu_control_producer` |
| `imu_control_bridge` | shared/locked | `sample` | `imu_sample` |
| `io_watchdog` | local/exclusive | `watchdog` | `io_watchdog` |
| `io_watchdog` | shared/locked | `spi1_owner` | `spi1_owner` |
| `msp_osd` | local/exclusive | `reader` | `msp_v1_osd_reader` |
| `msp_osd` | local/exclusive | `discontinuities` | `msp_v1_osd_rx_discontinuities` |
| `msp_osd` | local/exclusive | `writer` | `msp_v1_osd_writer` |
| `msp_osd` | local/exclusive | `osd` | `osd_task_state` |
| `msp_osd` | local/exclusive | `output` | `osd_tx_buffer` |
| `msp_osd` | local/exclusive | `refresh_tick` | `osd_refresh_tick` |
| `msp_osd` | local/exclusive | `tx_healthy` | `osd_tx_healthy` |
| `msp_osd` | shared/locked | `telemetry` | `flight_service_telemetry` |
| `msp_osd` | shared/locked | `tuning` | `tuning_profile` |
| `msp_osd` | shared/locked | `tuning_seq` | `tuning_request_seq` |
| `safety_master` | local/exclusive | `reader` | `rc_sbus_reader` |
| `safety_master` | local/exclusive | `discontinuities` | `rc_sbus_rx_discontinuities` |
| `safety_master` | local/exclusive | `foxeer_safety_state` | `foxeer_safety_state` |
| `safety_master` | local/exclusive | `control` | `sbus_control_producer` |
| `safety_master` | local/exclusive | `health` | `prearm_health_consumer` |
| `safety_master` | local/exclusive | `actuator_guards` | `actuator_guard_producer` |
| `safety_master` | local/exclusive | `actuator_completions` | `actuator_completion_consumer` |
| `safety_master` | local/exclusive | `actuator_faults` | `actuator_fault_consumer` |
| `safety_master` | shared/locked | `channel3` | `sbus_channel3` |
| `safety_master` | shared/locked | `telemetry` | `flight_service_telemetry` |
| `safety_master` | shared/locked | `tuning` | `tuning_profile` |
| `serial1_rx_bridge` | local/exclusive | `bridge` | `serial1_rx_bridge` |
| `serial1_rx_dma_irq` | shared/locked | `rx` | `serial1_rx` |
| `serial1_rx_idle_irq` | shared/locked | `rx` | `serial1_rx` |
| `serial1_tx_dma_irq` | local/exclusive | `completion` | `serial1_tx_completion` |
| `serial1_tx_dma_irq` | shared/locked | `tx_dma` | `serial1_tx_dma` |
| `serial1_tx_worker` | local/exclusive | `owner` | `serial1_tx_owner` |
| `serial1_tx_worker` | shared/locked | `tx_dma` | `serial1_tx_dma` |
| `serial2_rx_bridge` | local/exclusive | `bridge` | `serial2_rx_bridge` |
| `serial2_rx_dma_irq` | shared/locked | `rx` | `serial2_rx` |
| `serial2_rx_idle_irq` | shared/locked | `rx` | `serial2_rx` |
| `serial2_tx_dma_irq` | local/exclusive | `completion` | `serial2_tx_completion` |
| `serial2_tx_dma_irq` | shared/locked | `tx_dma` | `serial2_tx_dma` |
| `serial2_tx_worker` | local/exclusive | `owner` | `serial2_tx_owner` |
| `serial2_tx_worker` | shared/locked | `tx_dma` | `serial2_tx_dma` |
| `spi1_data_ready` | local/exclusive | `data_ready` | `spi1_data_ready` |
| `spi1_data_ready` | shared/locked | `kind` | `spi1_kind` |
| `spi1_owner_service` | shared/locked | `owner` | `spi1_owner` |
| `spi1_parser` | local/exclusive | `parser` | `spi1_parser` |
| `spi1_parser` | shared/locked | `kind` | `spi1_kind` |
| `spi1_parser` | shared/locked | `sample` | `imu_sample` |
| `spi1_poll` | local/exclusive | `device` | `spi1_device` |
| `spi1_poll` | local/exclusive | `unavailable_logged` | `spi1_unavailable_logged` |
| `spi1_poll` | shared/locked | `kind` | `spi1_kind` |
| `spi1_rx_dma_irq` | shared/locked | `owner` | `spi1_owner` |
| `spi1_timeout` | shared/locked | `owner` | `spi1_owner` |
| `usb_cdc` | local/exclusive | `device` | `usb_device` |
| `usb_cdc` | local/exclusive | `serial` | `usb_serial` |
| `usb_cdc` | local/exclusive | `header_sent` | `usb_header_sent` |
| `usb_cdc` | local/exclusive | `parser` | `usb_command_parser` |
| `usb_cdc` | local/exclusive | `commands` | `flash_command_producer` |
| `usb_cdc` | local/exclusive | `responses` | `flash_response_consumer` |
| `usb_cdc` | local/exclusive | `pending_response` | `usb_pending_response` |
| `usb_cdc` | shared/locked | `telemetry` | `flight_service_telemetry` |
| `usb_cdc` | shared/locked | `storage` | `storage_status` |
| `usb_cdc` | shared/locked | `status_due` | `usb_status_due` |

## Resolved resource inventory

| Resource | Kind | Ownership / role |
| --- | --- | --- |
| `sbus_channel3` | application shared | RTIC locked; initial `U32(0)` |
| `imu_sample` | IMU endpoint `spi1` | Sample; Shared; Exposed |
| `serial1_rx` | serial endpoint `serial1` | RxService; Shared; Private |
| `serial1_rx_bridge` | serial endpoint `serial1` | RxParser; Local; Private |
| `serial1_rx_buffers` | serial endpoint `serial1` | RxBuffers; InitLocal; Private |
| `serial1_rx_channel` | serial endpoint `serial1` | RxChannel; InitLocal; Private |
| `serial1_rx_discontinuities` | serial endpoint `serial1` | RxDiscontinuities; Local; Exposed |
| `serial1_rx_filled_queue` | serial endpoint `serial1` | RxFilledQueue; InitLocal; Private |
| `serial1_rx_free_queue` | serial endpoint `serial1` | RxFreeQueue; InitLocal; Private |
| `serial1_rx_reader` | serial endpoint `serial1` | RxReader; Local; Exposed |
| `serial1_tx_buffer` | serial endpoint `serial1` | TxBuffer; InitLocal; Private |
| `serial1_tx_channel` | serial endpoint `serial1` | TxChannel; InitLocal; Private |
| `serial1_tx_completion` | serial endpoint `serial1` | TxCompletion; Local; Private |
| `serial1_tx_dma` | serial endpoint `serial1` | TxDma; Shared; Private |
| `serial1_tx_owner` | serial endpoint `serial1` | TxOwner; Local; Private |
| `serial1_tx_writer` | serial endpoint `serial1` | TxWriter; Local; Exposed |
| `serial2_rx` | serial endpoint `serial2` | RxService; Shared; Private |
| `serial2_rx_bridge` | serial endpoint `serial2` | RxParser; Local; Private |
| `serial2_rx_buffers` | serial endpoint `serial2` | RxBuffers; InitLocal; Private |
| `serial2_rx_channel` | serial endpoint `serial2` | RxChannel; InitLocal; Private |
| `serial2_rx_discontinuities` | serial endpoint `serial2` | RxDiscontinuities; Local; Exposed |
| `serial2_rx_filled_queue` | serial endpoint `serial2` | RxFilledQueue; InitLocal; Private |
| `serial2_rx_free_queue` | serial endpoint `serial2` | RxFreeQueue; InitLocal; Private |
| `serial2_rx_reader` | serial endpoint `serial2` | RxReader; Local; Exposed |
| `serial2_tx_buffer` | serial endpoint `serial2` | TxBuffer; InitLocal; Private |
| `serial2_tx_channel` | serial endpoint `serial2` | TxChannel; InitLocal; Private |
| `serial2_tx_completion` | serial endpoint `serial2` | TxCompletion; Local; Private |
| `serial2_tx_dma` | serial endpoint `serial2` | TxDma; Shared; Private |
| `serial2_tx_owner` | serial endpoint `serial2` | TxOwner; Local; Private |
| `serial2_tx_writer` | serial endpoint `serial2` | TxWriter; Local; Exposed |
| `spi1_data_ready` | IMU endpoint `spi1` | DataReady; Local; Private |
| `spi1_device` | IMU endpoint `spi1` | Device; Local; Private |
| `spi1_dma_buffers` | IMU endpoint `spi1` | Buffers; InitLocal; Private |
| `spi1_filled_queue` | IMU endpoint `spi1` | FilledQueue; InitLocal; Private |
| `spi1_free_queue` | IMU endpoint `spi1` | FreeQueue; InitLocal; Private |
| `spi1_kind` | IMU endpoint `spi1` | Kind; Shared; Exposed |
| `spi1_owner` | IMU endpoint `spi1` | Owner; Shared; Private |
| `spi1_parser` | IMU endpoint `spi1` | Parser; Local; Private |
| `spi1_unavailable_logged` | IMU endpoint `spi1` | UnavailableLogged; Local; Private |
| `control_phase` | periodic control `control` | local; `Phase` |
| `control_scheduler` | periodic control `control` | local; `Scheduler` |

## Persistent task-state ownership

| State | Sole owner | Reviewed recipe |
| --- | --- | --- |
| `applied_tuning_seq` | `control_loop` | `U32(0)` |
| `control_loop_cnt` | `control_loop` | `U32(0)` |
| `control_was_armed` | `control_loop` | `Bool(false)` |
| `flight_controller` | `control_loop` | `FoxeerFlightControllerV1` |
| `foxeer_safety_state` | `safety_master` | `OutputInhibitedFoxeerSafetyMasterV1` |
| `gyro_axis_map` | `control_loop` | `BodyRateToControllerMapV1` |
| `gyro_bias_calibrator` | `control_loop` | `FoxeerGyroBiasCalibratorV1` |
| `imu_angle_integrator` | `control_loop` | `GyroAngleIntegratorV1` |
| `imu_last_sequence` | `control_loop` | `U32(0)` |
| `imu_rate_filter` | `control_loop` | `FoxeerImuRateLowPassFilterV1` |
| `imu_stale_ticks` | `control_loop` | `U32(0)` |
| `latest_rc_input` | `control_loop` | `EmptyRcInputSnapshotV1` |
| `motor_cmd_seq` | `control_loop` | `U32(0)` |
| `rc_last_valid_frames` | `control_loop` | `U32(0)` |
| `rc_link_was_valid` | `control_loop` | `Bool(false)` |
| `rc_stale_ticks` | `control_loop` | `U32(0)` |
| `samples_per_control_loop` | `control_loop` | `U32(2)` |

## Component and physical-resource map

| Instance | Physical resource | Role |
| --- | --- | --- |
| `serial1` | `Usart2` (`serial1`) | DMA serial endpoint |
| `serial2` | `Uart4` (`serial2`) | DMA serial endpoint |
| `spi1` | `Spi1` (`spi1`) | SPI IMU endpoint, installation `ImuInstallationId(1)` |
| `control` | `Tim4` (`tim4`) | 800 Hz scheduler / 400 Hz control |
| `physical_actuator` | `Tim1Ch1` / `PinId { port: A, pin: 8 }` / `DmaRoute { controller: Dma2, stream: Stream1, channel: Channel6 }` | physical output 1 to logical M1 |
| `physical_actuator` | `Tim8Ch4` / `PinId { port: C, pin: 9 }` / `DmaRoute { controller: Dma2, stream: Stream7, channel: Channel7 }` | physical output 2 to logical M2 |
| `physical_actuator` | `Tim8Ch3` / `PinId { port: C, pin: 8 }` / `DmaRoute { controller: Dma2, stream: Stream2, channel: Channel0 }` | physical output 3 to logical M3 |
| `physical_actuator` | `Tim1Ch3N` / `PinId { port: B, pin: 15 }` / `DmaRoute { controller: Dma2, stream: Stream6, channel: Channel6 }` | physical output 4 to logical M4 (complementary) |
| `physical_actuator` | `Usart1` (`esc_telemetry`) | receive-only ESC telemetry; output enabled = `false` |
| `golden_services` | `Adc1` (`adc1_observation`) / `DmaRoute { controller: Dma2, stream: Stream4, channel: Channel0 }` | ADC voltage/current observation on channels 10/11 |
| `golden_services` | `Spi2` (`spi2_nor`) | CPU-serviced SPI NOR at 10000000 Hz |
| `golden_services` | `OtgFs` (`usb_cdc`) | USB CDC `FerroWasp Foxeer Debug` / `FW-FOX-F405V2` |
| `golden_services` | `Tim6` (`tim6`) | 8000 Hz I/O watchdog |
| `monotonic` | `tim2` | 1000000 Hz RTIC monotonic |
| `init_delay` | `tim5` | 1000000 Hz synchronous initialization delay |

## Golden Foxeer reconciliation

### Pinned handwritten source state

- Repository revision: `e7f4590dee98b4ebd70114932153e066a70e7972`.
- Complete `apps/foxeer-f405-v2` tree: `c9eb1d19412cb9cb34d3a74005d4060c0fd2fcdd`.
- Rust source tree `apps/foxeer-f405-v2/src`: `b5467ee26b70d939234ff2f5a56ef6c71c5a8b10`.
- Review scope: `Cargo.toml`, `src/main.rs`, `src/lib.rs`, and `src/board/**` from that tree. Generated dependency-lockfile churn is not used as behavioral evidence.

### Semantic graph diff

| Required area | Pinned handwritten app | Generated candidate | Disposition |
| --- | --- | --- | --- |
| Physical resources | STM32F405 at 168 MHz; reviewed USART1/2, UART4, SPI1/2, ADC1, OTG_FS, TIM1/2/4/5/6/8, GPIO and DMA routes. | The typed board declaration locks the same routes; the reusable USART2 endpoint additionally reserves unused TX DMA1 Stream6. | Reviewed intentional deviation |
| Initialization | Hand-wired peripheral construction, split signals/queues, SysTick 1 kHz monotonic, TIM2 microsecond timebase and TIM5 delay. | Typed constructors and split handles initialize the same services; TIM2 is the 1 MHz RTIC monotonic and TIM5 remains the 1 MHz init delay. | Reviewed intentional decomposition/timer deviation |
| Tasks and priorities | Protocol-specific tasks include RC input p10, safety p16, control p14, actuator p15, DShot p13/p16 and mandatory services at their recorded priorities. | The exact generated task inventory and priorities are validation-locked; SBUS uses endpoint IRQ p13, bridge p12 and safety-owned parsing p16. | Reviewed intentional decomposition/priority deviation |
| Transports | USART2 SBUS, UART4 MSP V1 DisplayPort, SPI1 IMU, SPI2 NOR, USART1 ESC telemetry and OTG_FS CDC are directly wired. | Typed endpoint/component routes preserve those protocols, ownership and bounded discontinuity handling. | Equivalent semantic routes; reviewed endpoint decomposition |
| Capacities | Shared backends provide four RX buffers/depth four, UART4 TX depth 16, SPI buffers/queues, motor/ESC queues and flash queues. | The same backend capacities are locked, plus seven explicit SPSC safety channels with validated capacities and sole owners. | Equivalent shared capacities; intentional explicit safety channels |
| Safety state | At the pinned default feature set all board verification flags are true, so physical actuator output can arm after runtime guards and ESC qualification. | Safety state starts output-inhibited, the physical component gate is false, and control is forced disarmed. | Intentional pre-cutover inhibition |
| Fault behavior | Protocol faults invalidate inputs; stale/invalid control fails closed; actuator and ESC faults revoke authority and stop output. | The same reusable fail-closed policies are retained, with a bounded central actuator-fault channel and explicit transport-discontinuity paths. | Equivalent outcome; reviewed event-routing deviation |
| Timing | 800 Hz TIM4 scheduler, 400 Hz control, 2 ms DShot/ESC, 10 ms OSD, 100 ms ADC, 2 s heartbeat and 8 kHz watchdog. | Those cadences are validation-locked; safety/actuator add bounded 10 ms policy loops and the IMU bridge polls at 1 ms. | Equivalent required cadences; reviewed bounded polling |
| Feature gates | Compile-time profiles cover flight, props-off bench, fault injection, RTT diagnostics and optional MSPv2 configuration. | One fixed mandatory-service composition has no generated feature matrix and both output gates remain false. | Intentional single inhibited profile |
| Logging and configuration | Sequenced tuning, append-only flight records, copy-on-write config, USB CLI, scratch test and optional MSPv2 RPC. | The same record/config primitives and bounded queues are used; scratch test and MSPv2 RPC are omitted, and forced disarm prevents flight-scoped page assembly. | Reviewed intentional diagnostic omissions |

### Preserved boundaries

- Foxeer F405 V2 MCU/clock facts and the selected USART2, UART4, SPI1, SPI2, ADC1, OTG_FS, TIM2, TIM4, TIM5, and TIM6 routes.
- The verified IMU sensor-to-body orientation, followed exactly once by the legacy body-to-controller pitch mapping.
- The TIM4 priority-14 800 Hz scheduler and 400 Hz control cadence.
- Bounded SBUS recovery, freshness, arming-transition, guard-order, controller, Quad-X mixing, motor-order, and command-capacity semantics.
- Only safety-owned state can grant an actuator permit; outer control code can only publish typed requests.
- TIM1/TIM8 DShot and USART1 telemetry use the reviewed physical routes, while two independent software gates remain disabled.
- ADC observations expire after 500 ms; MSP, USB, flash, heartbeat, and watchdog services remain outside actuator authority.

### Locked capacities and cadences

| Surface | Generated lock | Golden relationship |
| --- | --- | --- |
| Serial RX/TX | 4 DMA RX buffers, RX queue depth 4 and TX queue depth 16 per typed endpoint | Shared STM32F4 backend values; USART2 TX is generated but unused |
| SPI1 IMU | 5 DMA RX buffers and queue storage 4 | Same shared STM32F4 backend |
| Safety channels | SBUS/IMU/health usable 4; motor/guard/completion/fault usable 3 | Generated explicit SPSC split of golden signal and motor-command paths |
| ESC queues | request 4, acknowledgement 4, telemetry update 16 storage slots | Same reusable ESC manager types |
| Flash/USB queues | record 64, command 8, response 32 storage slots; command line 96 and response 64 bytes | Same reusable storage primitives; MSPv2 RPC queues omitted |
| Control/services | 800/400 Hz control; 2 ms DShot/ESC; 10 ms OSD; 100 ms ADC; 2 s heartbeat; 8 kHz watchdog | Golden cadences preserved |

### Intentional differences

| Area | Handwritten golden app | Generated candidate | Rationale |
| --- | --- | --- | --- |
| Actuator authority | Safety-gated DShot on TIM1/TIM8, four motor GPIOs and DMA completion paths; the pinned default feature set enables output after runtime guards and ESC qualification. | The exact four-lane DShot/USART1 path is generated through the shared backend, but both safety policy and the physical component independently inhibit output. | The architecture is compile-checked while software, electrical, props-off, and target evidence gates remain closed. |
| Arming visibility in control | Control observes the live safety arming state and can submit gated actuator requests. | The same control step is called with `armed = false`; the request path is modeled but cannot become active. | This checkpoint proves the control/safety spine without enabling output. |
| RC and safety task shape | Priority-10 `rc_input` parses SBUS; priority-16 `safety_master` evaluates authoritative state from signals. | Priority-16 `safety_master` exclusively owns the SBUS reader, parser recovery, RC link state, arming state, and permit. | The current generated spine keeps parser faults and policy state in one exclusive authority boundary. |
| Safety scheduling | Safety is event-driven through typed `SafetyEvent` spawns from relevant producers. | The safety master combines direct SBUS consumption with a bounded 10 ms timeout/policy loop and publishes exclusive fresh authority observations to the actuator. | Polling is explicit and bounded at this checkpoint; event fan-in can be restored without moving safety authority. |
| Health transport | Safety reads live health prerequisites through golden signal/state readers. | Control publishes observation-only `PreArmHealthReport` values over a bounded SPSC channel with a 20 ms freshness limit. | The observation channel cannot grant authority and makes producer/consumer ownership explicit. |
| Transport composition | USART2 SBUS, UART4 MSP and SPI1 IMU services are hand-wired in the app. | Typed endpoint graphs are physical at composition time; boot platform configuration assigns portable services to connector handles. | This is the builder's validated service-routing boundary; physical routes remain board facts. |
| Serial task decomposition | Protocol-specific interrupt and worker tasks own the queues directly. | Reusable DMA endpoint IRQ, bridge and TX-worker tasks feed portable bounded channels. | The endpoint mechanism is shared across services while preserving bounded DMA and discontinuity handling. |
| IMU decomposition | SPI1 data-ready, polling, owner, DMA, timeout and parser logic is wired directly in the app. | The same physical SPI1/EXTI4/DMA2_STREAM0 flow is expanded from one typed IMU endpoint component. | Generated resource ownership is explicit; the verified sensor-to-body transform is still applied exactly once. |
| OSD scope | Priority-3 DisplayPort telemetry includes receive handling, menus and tuning behavior. | Priority-3 DisplayPort telemetry handles bounded UART4 receive/replies, stale-safe telemetry, overlay updates, and the disarmed tuning menu through reusable endpoint handles. | The task decomposition differs, but UART ownership, behavior, priority, and the no-safety-authority boundary are preserved. |
| Diagnostics and persistence | Includes blackbox/config flash, detailed task identities and application diagnostics. | Owns SPI2 NOR through one priority-1 task, recovers append-only logs/config, verifies copy-on-write saves, and exposes bounded USB CDC text diagnostics and whitelisted commands. | The generated candidate omits the optional destructive scratch test and MSPv2 configurator profile; all mutations remain disarmed-only. |
| Runtime tuning and control logging | Control applies sequenced runtime tuning profiles and publishes blackbox records. | Control applies sequenced stored/menu tuning and publishes rate records into the bounded SPI2 queue, with explicit drop accounting. | The generated build remains permanently disarmed, so flight-scoped page assembly cannot start until the independent output gates are separately approved. |
| Remaining services | Includes USB CDC, ADC sensing, heartbeat, IO watchdog, ESC manager and USART1 ESC telemetry. | Includes the same mandatory service identities and priorities, explicit ADC freshness suppression, OTG_FS CDC, TIM6 watchdog, heartbeat, ESC manager and exact USART1 telemetry. | The generated services are observation, diagnostics, persistence, or recovery paths and receive no actuator authority. |
| RTIC dispatchers | Dispatcher selection follows the handwritten task graph. | Dispatchers are deterministically allocated after resolved interrupt ownership. | Dispatcher identity is an implementation detail, but collision-free allocation remains validated. |
| Feature profiles | Compile-time features select configurator, telemetry, diagnostics and guarded physical-output behavior. | One mandatory-service composition is rendered with text CDC configuration and physical output inhibited; optional MSPv2/diagnostic feature combinations are not generated. | No feature combination may silently expand actuator authority; later profiles require explicit compositions and validation. |

### Reconciliation result

Every requested semantic area is either validation-locked as equivalent or listed above as an intentional deviation. No unexplained difference remains within this pinned graph review. The handwritten Foxeer application remains authoritative; this report does not authorize an automatic cutover or enable either generated actuator gate.


### Evidence boundary

This report and the associated host/Thumb checks establish deterministic declaration, ownership, rendering, type-checking, and link/check evidence only. They do not establish electrical routing, DMA behavior on silicon, sensor validity, worst-case timing, actuator behavior, props-off bench behavior, or flight readiness.

