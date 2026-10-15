//! What every flight application shares above its board: the names its
//! `main.rs` uses, the USB configurator state, and `flight_app_facade!`, which
//! writes the rest of an application's facade from its board's identity.
//!
//! The facade is a macro rather than a module because its feature checks,
//! board constants, statics, and build stamps belong to the application
//! crate: `cfg!(feature = ..)`, `board::` paths, and `env!` resolve where the
//! macro expands.

use crate::prelude::*;
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2 as mspv2;

/// The names a flight application's `main.rs` uses, whatever its board.
pub mod prelude {
    pub use core::cell::RefCell;
    pub use core::fmt::Write as CoreFmtWrite;
    pub use core::sync::atomic::Ordering;
    pub use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32};
    pub use critical_section::Mutex;
    pub use defmt::{info, warn};
    pub use embedded_hal::spi::Operation;
    pub use embedded_hal_async::spi::SpiDevice;
    pub use embedded_io_async::Read as AsyncRead;
    pub use ferrowasp_core::actuator::{remap_motor_outputs, throttle_to_u16};
    pub use ferrowasp_core::safety;
    pub use ferrowasp_core::safety::signals::{
        self, ActuatorArmPermitReader, ActuatorArmPermitWriter, RcRatesReader, RcRatesWriter,
    };
    pub use ferrowasp_drivers::{icm42688p as icm, mpu6500 as imu};
    pub use ferrowasp_flight::actuator as actuator_task;
    #[cfg(feature = "mspv2_configurator")]
    pub use ferrowasp_flight::blackbox_storage as blackbox_task;
    pub use ferrowasp_flight::command_link::{CommandLink, CommandSources, ResponseRouter};
    pub use ferrowasp_flight::drone_toolbox as dt;
    pub use ferrowasp_flight::esc_manager as esc;
    pub use ferrowasp_flight::esc_manager::{
        DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_IDLE_THROTTLE_COMMAND, DSHOT_PREARM_STOP_HOLD_MS,
    };
    pub use ferrowasp_flight::flash_storage as flash_task;
    pub use ferrowasp_flight::osd;
    pub use ferrowasp_flight::rc_receiver::RcReceiver;
    #[cfg(not(feature = "mspv2_configurator"))]
    pub use ferrowasp_flight::usb_debug;
    pub use ferrowasp_flight::{log_sync, sync_ledger};
    pub use ferrowasp_io_core::serial::{
        RcProtocol, ResolvedBindings, SerialBindings, SerialRoute,
    };
    pub use ferrowasp_io_core::spi::{
        AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiDeadlineUs,
        SpiRequestMailbox,
    };
    pub use ferrowasp_io_core::{
        serial::{Discontinuity, RxChunk, SerialFault},
        time::TimestampMicros,
    };
    pub use ferrowasp_mspv1 as mspv1;
    #[cfg(feature = "mspv2_configurator")]
    pub use ferrowasp_mspv2 as mspv2;
    pub use ferrowasp_stm32::app_config::{
        ARMING_GUARD_POLL_MS, BENCH_EQUAL_MOTOR_MAX_THROTTLE, DELAY_TIMER_HZ, ESC_MANAGER_PERIOD_MS,
    };
    pub use ferrowasp_stm32::app_storage as stm32_storage;
    pub use ferrowasp_stm32::memory as stm32_memory;
    pub use ferrowasp_stm32::uart_port as stm32_port;

    pub use crate as flight_tasks;
    #[cfg(not(feature = "mspv2_configurator"))]
    pub use crate::app::active_usb_debug_imu_kind;
    pub use crate::app::{
        ConfiguratorUsbState, FlashRpcCommandProducer, FlashRpcResponseConsumer, MspCommonInfoFn,
    };
    #[cfg(feature = "mspv2_configurator")]
    pub use crate::app::{fixed_bytes, handle_msp_packet, stage_rpc_response};
    pub use crate::snapshots::{
        ACTIVE_IMU_KIND, ADC_CURRENT_MV_SNAPSHOT, ADC_VOLTAGE_MV_SNAPSHOT, ATTITUDE_PITCH_DEG10,
        ATTITUDE_ROLL_DEG10, ATTITUDE_YAW_DEG10, BATTERY_CURRENT_CA_SNAPSHOT,
        BATTERY_VOLTAGE_V10_SNAPSHOT, CONTROL_ISR_SEQ, CONTROL_PITCH_DPS10, CONTROL_PITCH_RAW,
        CONTROL_RATE_SEQ, CONTROL_ROLL_DPS10, CONTROL_ROLL_RAW, CONTROL_YAW_DPS10, CONTROL_YAW_RAW,
        DSHOT_ACTIVE_LANES, FLASH_CAPACITY_BYTES, FLASH_JEDEC_CAPACITY_CODE,
        FLASH_JEDEC_MANUFACTURER, FLASH_JEDEC_MEMORY_TYPE, FLASH_LOG_RATE_DIVISOR,
        FLASH_PAGES_WRITTEN, FLASH_READY, FLASH_RECORDS_DROPPED, FLASH_WRITE_FAULTS,
        IMU_BIAS_CALIBRATED, IMU_DRDY_IRQ_COUNT, IMU_DRDY_LAST_US, IMU_DRDY_REJECTED_COUNT,
        IMU_LATEST_PITCH_RAW, IMU_LATEST_ROLL_RAW, IMU_LATEST_SEQ, IMU_LATEST_YAW_RAW, IMU_STALE,
        IMU_TRANSPORT_READY, RC_ARM_HIGH, RC_THROTTLE, SAFETY_ARMED, USB_DEBUG_DUE,
        USB_HOST_CONNECTED, USB_RC_ARMABLE_SNAPSHOT, USB_RC_VALID_SNAPSHOT, host_connected,
        live_snapshot, rc_channels_us,
    };
    #[cfg(feature = "imu_orientation_rtt")]
    pub use crate::snapshots::{
        IMU_LATEST_ACCEL_X_MG, IMU_LATEST_ACCEL_Y_MG, IMU_LATEST_ACCEL_Z_MG,
        IMU_LATEST_GYRO_X_DPS10, IMU_LATEST_GYRO_Y_DPS10, IMU_LATEST_GYRO_Z_DPS10,
        IMU_LATEST_TEMP_C10, IMU_ORIENTATION_VERSION, imu_orientation_snapshot,
    };
    pub use crate::{SPI1_MAILBOX, Spi1Device, Spi1Executor, Spi1Mailbox};

    pub type SerialPortEndpoint = stm32_port::SerialPortEndpoint;
    pub type EscManagerState = esc::EscManager;
    pub type EscRequestProducer = esc::EscRequestProducer;
    pub type EscRequestConsumer = esc::EscRequestConsumer;
    pub type EscAckProducer = esc::EscAckProducer;
    pub type EscAckConsumer = esc::EscAckConsumer;
    pub type EscTelemetryUpdateProducer = esc::EscTelemetryUpdateProducer;
    pub type EscTelemetryUpdateConsumer = esc::EscTelemetryUpdateConsumer;
    pub type UartOwnedRxChannel = stm32_memory::UartOwnedRxChannel;
    pub type UartOwnedTxChannel = stm32_memory::UartOwnedTxChannel;
    pub type UartOwnedWriter = stm32_memory::UartOwnedWriter<'static>;
    pub type UartOwnedTxOwner = stm32_memory::UartOwnedTxOwner<'static>;
    pub type UartOwnedTxCompletion = stm32_memory::UartOwnedTxCompletion<'static>;
    pub type UsbDebugDevice = crate::prelude::backend::usb_serial::UsbCdcDevice;
    pub type UsbDebugSerial = crate::prelude::backend::usb_serial::BufferedUsbCdcSerial;
    pub type FlashRecordProducer = flash_task::RecordProducer;
    pub type FlashRecordConsumer = flash_task::RecordConsumer;
    pub type FlashCommandProducer = flash_task::CommandProducer;
    pub type FlashCommandConsumer = flash_task::CommandConsumer;
    pub type FlashResponseProducer = flash_task::ResponseProducer;
    pub type FlashResponseConsumer = flash_task::ResponseConsumer;
    pub use crate::{FlashRpcCommandConsumer, FlashRpcResponseProducer};
}

#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcCommandProducer = flash_task::RpcCommandProducer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcCommandProducer = ();
#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcResponseConsumer = flash_task::RpcResponseConsumer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcResponseConsumer = ();

/// The board's MSPv2 identity for the common commands, configuration
/// supplied to the USB task; `()` without the MSPv2 configurator.
#[cfg(feature = "mspv2_configurator")]
pub type MspCommonInfoFn = fn() -> mspv2::commands::CommonInfo<'static>;
#[cfg(not(feature = "mspv2_configurator"))]
pub type MspCommonInfoFn = ();

/// The USB configurator's parser and pending reply.
#[cfg(feature = "mspv2_configurator")]
pub struct ConfiguratorUsbState {
    pub parser: mspv2::MspParser,
    pub pending_tx: mspv2::EncodedFrame,
    pub rpc_payload: [u8; mspv2::MAX_PAYLOAD_LEN],
}

#[cfg(feature = "mspv2_configurator")]
impl ConfiguratorUsbState {
    pub const fn new() -> Self {
        Self {
            parser: mspv2::MspParser::new(),
            pending_tx: mspv2::EncodedFrame::new(),
            rpc_payload: [0; mspv2::MAX_PAYLOAD_LEN],
        }
    }
}

#[cfg(not(feature = "mspv2_configurator"))]
pub struct ConfiguratorUsbState;

#[cfg(not(feature = "mspv2_configurator"))]
impl ConfiguratorUsbState {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ConfiguratorUsbState {
    fn default() -> Self {
        Self::new()
    }
}

/// The detected IMU, as the USB status line names it.
#[cfg(not(feature = "mspv2_configurator"))]
pub fn active_usb_debug_imu_kind() -> ferrowasp_flight::usb_debug::ImuKind {
    use ferrowasp_flight::usb_debug::ImuKind;
    use ferrowasp_stm32::spi1_imu::Spi1ImuKind;
    match Spi1ImuKind::from_discriminant(crate::snapshots::ACTIVE_IMU_KIND.load(Ordering::Relaxed))
    {
        Some(Spi1ImuKind::Mpu6500) => ImuKind::Mpu6500,
        Some(Spi1ImuKind::Icm42688P) => ImuKind::Icm42688P,
        Some(Spi1ImuKind::Mpu6000) => ImuKind::Mpu6000,
        None => ImuKind::None,
    }
}

/// `value`, cut or zero-padded to `N` bytes.
#[cfg(feature = "mspv2_configurator")]
pub fn fixed_bytes<const N: usize>(value: &[u8]) -> [u8; N] {
    let mut output = [0u8; N];
    let len = value.len().min(N);
    output[..len].copy_from_slice(&value[..len]);
    output
}

#[cfg(feature = "mspv2_configurator")]
pub fn stage_rpc_response(
    state: &mut ConfiguratorUsbState,
    response: &mspv2::rpc::RpcResponse,
) -> bool {
    let ConfiguratorUsbState {
        pending_tx,
        rpc_payload,
        ..
    } = state;
    let Ok(payload) = mspv2::rpc::encode_response(response, rpc_payload) else {
        return false;
    };
    pending_tx
        .set(
            mspv2::MspDirection::FromFlightController,
            0,
            mspv2::rpc::MSP2_FWSP_RPC,
            payload,
        )
        .is_ok()
}

/// Answer one MSPv2 packet from the USB host: the common commands from the
/// board's identity, `Hello` from its device information, and every other
/// RPC request handed to the flash manager.
#[cfg(feature = "mspv2_configurator")]
pub fn handle_msp_packet(
    packet: mspv2::MspPacket,
    state: &mut ConfiguratorUsbState,
    rpc_commands: &mut flash_task::RpcCommandProducer,
    common_info: fn() -> mspv2::commands::CommonInfo<'static>,
    device_info: fn() -> mspv2::rpc::DeviceInfo,
) {
    if packet.direction != mspv2::MspDirection::ToFlightController || state.pending_tx.is_pending()
    {
        return;
    }

    let mut common_payload = [0u8; 96];
    if let Some(len) =
        mspv2::commands::encode_common_payload(&packet, &common_info(), &mut common_payload)
    {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::FromFlightController,
            packet.flags,
            packet.function,
            &common_payload[..len],
        );
        return;
    }

    if packet.function != mspv2::rpc::MSP2_FWSP_RPC {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::Error,
            packet.flags,
            packet.function,
            &[],
        );
        return;
    }

    let Ok(request) = mspv2::rpc::decode_request(packet.payload()) else {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::Error,
            packet.flags,
            packet.function,
            &[],
        );
        return;
    };
    if request.protocol_version != mspv2::rpc::FWSP_RPC_VERSION {
        let response = mspv2::rpc::error(
            request.request_id,
            mspv2::rpc::DeviceError::UnsupportedProtocolVersion,
        );
        let _ = stage_rpc_response(state, &response);
        return;
    }
    if request.operation == mspv2::rpc::Request::Hello {
        let response = mspv2::rpc::ok(
            request.request_id,
            mspv2::rpc::Response::Hello(device_info()),
        );
        let _ = stage_rpc_response(state, &response);
        return;
    }
    let request_id = request.request_id;
    if rpc_commands.enqueue(request).is_err() {
        let response = mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy);
        let _ = stage_rpc_response(state, &response);
    }
}

/// Write a flight application's facade from its board's identity: the
/// feature-combination checks, the shared prelude, the constants and statics
/// its `main.rs` reads from the board's profiles, the MSPv2 identity, and the
/// helpers that resolve boot bindings, arming health, and motor numbering.
/// The application's `lib.rs` adds only its chip backend and board types.
// `internal` re-exports the application crate the macro expands in, not this
// one, so `crate` is meant.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! flight_app_facade {
    (
        board_name: $board_name:literal,
        msp_identifier: $msp_identifier:expr,
        target_name: $target_name:literal,
        mcu: $mcu:expr $(,)?
    ) => {
        #[cfg(any(
            feature = "bench_motor1_only",
            feature = "bench_motor2_only",
            feature = "bench_motor3_only",
            feature = "bench_motor4_only"
        ))]
        compile_error!(
            concat!($board_name, " DShot uses logical-motor selection; physical PWM selectors are not supported.")
        );
        #[cfg(all(
            feature = "bench_actuator_validation",
            not(any(
                feature = "bench_equal_motors",
                feature = "bench_motor1_only",
                feature = "bench_motor2_only",
                feature = "bench_motor3_only",
                feature = "bench_motor4_only",
                feature = "bench_logical_motor1_only",
                feature = "bench_logical_motor2_only",
                feature = "bench_logical_motor3_only",
                feature = "bench_logical_motor4_only"
            ))
        ))]
        compile_error!(
            "Feature `bench_actuator_validation` requires a capped equal-motor, physical-motor, or logical-motor bench feature."
        );
        #[cfg(all(
            not(feature = "bench_actuator_validation"),
            any(
                feature = "bench_equal_motors",
                feature = "bench_motor1_only",
                feature = "bench_motor2_only",
                feature = "bench_motor3_only",
                feature = "bench_motor4_only",
                feature = "bench_logical_motor1_only",
                feature = "bench_logical_motor2_only",
                feature = "bench_logical_motor3_only",
                feature = "bench_logical_motor4_only"
            )
        ))]
        compile_error!(
            concat!($board_name, " motor bench features require the explicit props-off `bench_actuator_validation` gate.")
        );
        #[cfg(any(
            all(
                feature = "bench_motor1_only",
                any(
                    feature = "bench_motor2_only",
                    feature = "bench_motor3_only",
                    feature = "bench_motor4_only",
                    feature = "bench_logical_motor1_only",
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )
            ),
            all(
                feature = "bench_motor2_only",
                any(
                    feature = "bench_motor3_only",
                    feature = "bench_motor4_only",
                    feature = "bench_logical_motor1_only",
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )
            ),
            all(
                feature = "bench_motor3_only",
                any(
                    feature = "bench_motor4_only",
                    feature = "bench_logical_motor1_only",
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )
            ),
            all(
                feature = "bench_motor4_only",
                any(
                    feature = "bench_logical_motor1_only",
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )
            ),
            all(
                feature = "bench_logical_motor1_only",
                any(
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )
            ),
            all(
                feature = "bench_logical_motor2_only",
                any(
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )
            ),
            all(
                feature = "bench_logical_motor3_only",
                feature = "bench_logical_motor4_only"
            )
        ))]
        compile_error!(concat!("Select at most one physical or logical ", $board_name, " motor bench feature."));

        pub use $crate::app::prelude::*;

        pub use board::Spi1ImuKind;
        pub use board::profiles::{
            ADC_OBSERVATION_PROFILE, ARMING_INHIBIT_REASON, DEFAULT_STORED_CONFIG,
            DEFAULT_TUNING, FLIGHT_ARMING_ENABLED, IMU_CONTROL_AXIS_PROFILE,
            imu_kind_flight_verified,
        };
        pub const BENCH_ACTUATOR_VALIDATION_ENABLED: bool =
            cfg!(feature = "bench_actuator_validation");
        pub const SMOKE_ACTUATOR_INHIBIT_ENABLED: bool = cfg!(feature = "smoke_actuator_inhibit");
        pub const ACTUATOR_OUTPUT_ENABLED: bool = !SMOKE_ACTUATOR_INHIBIT_ENABLED
            && (FLIGHT_ARMING_ENABLED || BENCH_ACTUATOR_VALIDATION_ENABLED);
        pub const ACTUATOR_INHIBIT_REASON: &str = if SMOKE_ACTUATOR_INHIBIT_ENABLED {
            concat!($board_name, " smoke-test actuator lockout is active")
        } else {
            ARMING_INHIBIT_REASON
        };

        pub const USB_DEBUG_HEADER: &[u8] =
            concat!("FerroWasp ", $board_name, " storage CLI v1; type help\r\n").as_bytes();
        pub static RC_RATES: Mutex<RefCell<safety::RcRates>> =
            Mutex::new(RefCell::new(safety::RcRates {
                roll: 0,
                pitch: 0,
                yaw: 0,
            }));
        pub static RC_LINK: Mutex<RefCell<safety::RcLinkState>> =
            Mutex::new(RefCell::new(safety::RcLinkState::new()));
        pub static ACTUATOR_ARM_DONE: AtomicBool = AtomicBool::new(false);
        pub static ACTUATOR_ARM_PERMIT: AtomicBool = AtomicBool::new(false);
        pub const ADC_VBAT_DIVIDER_RATIO: f32 = ADC_OBSERVATION_PROFILE.vbat_divider_ratio;
        pub const ADC_CURRENT_BETAFLIGHT_SCALE: u32 =
            ADC_OBSERVATION_PROFILE.current_betaflight_scale;
        pub const ADC_CURRENT_OFFSET_MA: i32 = ADC_OBSERVATION_PROFILE.current_offset_ma;
        pub const BATTERY_MAX_CELL_MV: u16 = ADC_OBSERVATION_PROFILE.battery_max_cell_mv;
        pub const BATTERY_DETECT_CELL_MV: u16 = ADC_OBSERVATION_PROFILE.battery_detect_cell_mv;
        pub const BATTERY_MAX_CELLS: u8 = ADC_OBSERVATION_PROFILE.battery_max_cells;
        pub const BOARD_DSHOT_IDLE_COMMAND: f32 = DSHOT_IDLE_THROTTLE_COMMAND as f32;
        const _: () = {
            assert!(DSHOT_IDLE_THROTTLE_COMMAND > 0);
            assert!(DSHOT_IDLE_QUALIFICATION_CONFIG.is_valid());
        };

        /// Gyro counts per degree per second.
        pub const IMU_GYRO_COUNTS_PER_DPS: f32 =
            IMU_CONTROL_AXIS_PROFILE.gyro_counts_per_dps10 as f32 / 10.0;
        #[cfg(feature = "imu_orientation_rtt")]
        pub const PHYSICAL_IMU_TO_DRONE_ROTATION: dt::FrameRotation =
            IMU_CONTROL_AXIS_PROFILE.imu_to_drone_rotation();
        pub const CONTROL_IMU_TO_RATE_CONTROLLER_MAP: dt::FrameRotation =
            IMU_CONTROL_AXIS_PROFILE.imu_to_rate_controller_map();
        pub const GYRO_BIAS_CALIBRATION_SAMPLES: u32 =
            IMU_CONTROL_AXIS_PROFILE.bias_calibration_samples;
        pub const GYRO_BIAS_CALIBRATION_MAX_RAW: i32 =
            IMU_CONTROL_AXIS_PROFILE.bias_calibration_max_raw;

        /// The board's MSPv2 identity for the common commands.
        #[cfg(feature = "mspv2_configurator")]
        pub fn msp_common_info() -> mspv2::commands::CommonInfo<'static> {
            mspv2::commands::CommonInfo {
                firmware_version: [0, 1, 0],
                board_identifier: $msp_identifier,
                board_name: $board_name.as_bytes(),
                target_name: $target_name.as_bytes(),
                build_date: fixed_bytes(env!("FWSP_BUILD_DATE").as_bytes()),
                build_time: fixed_bytes(env!("FWSP_BUILD_TIME").as_bytes()),
                git_revision: fixed_bytes(env!("FWSP_GIT_REV").as_bytes()),
                uid: device_uid(),
                cycle_time_us: 2_500,
                sensors: u16::from(IMU_TRANSPORT_READY.load(Ordering::Relaxed)),
                armed: SAFETY_ARMED.load(Ordering::Acquire),
            }
        }

        /// The board's answer to the configurator's `Hello`.
        #[cfg(feature = "mspv2_configurator")]
        pub fn rpc_device_info() -> mspv2::rpc::DeviceInfo {
            use mspv2::rpc::capabilities;

            let capabilities = capabilities::CONFIG_READ
                | capabilities::CONFIG_WRITE
                | capabilities::CONFIG_RESET
                | capabilities::BLACKBOX_LIST
                | capabilities::BLACKBOX_DOWNLOAD;
            let mut board_id = [0u8; 16];
            let id = $target_name.as_bytes();
            board_id[..id.len()].copy_from_slice(id);
            mspv2::rpc::DeviceInfo {
                protocol_version: mspv2::rpc::FWSP_RPC_VERSION,
                firmware_version: [0, 1, 0],
                git_revision: fixed_bytes(env!("FWSP_GIT_REV").as_bytes()),
                board_id,
                board_id_len: id.len() as u8,
                mcu: $mcu,
                device_serial: device_uid(),
                armed: SAFETY_ARMED.load(Ordering::Acquire),
                capabilities,
                max_blackbox_chunk: mspv2::rpc::MAX_BLACKBOX_CHUNK as u16,
                config_schema_version: mspv2::rpc::CONFIG_SCHEMA_VERSION,
            }
        }

        /// The MSPv2 identity the USB task is configured with.
        #[cfg(feature = "mspv2_configurator")]
        pub const MSP_COMMON_INFO: MspCommonInfoFn = msp_common_info;
        #[cfg(not(feature = "mspv2_configurator"))]
        pub const MSP_COMMON_INFO: MspCommonInfoFn = ();
        #[cfg(feature = "mspv2_configurator")]
        pub const RPC_DEVICE_INFO: flight_tasks::RpcDeviceInfoFn = rpc_device_info;
        #[cfg(not(feature = "mspv2_configurator"))]
        pub const RPC_DEVICE_INFO: flight_tasks::RpcDeviceInfoFn = ();

        /// The serial bindings to start the ports with, and the RC protocol of
        /// the port bound to RC input: the saved values when storage holds a
        /// configuration, else the board's defaults. Read once in init, so a
        /// changed binding or protocol applies after a reboot.
        pub fn boot_serial_bindings(flash: &mut FlashDevice) -> ResolvedBindings {
            let saved = if FLASH_READY.load(Ordering::Acquire) {
                flash_task::StorageLayout::new(FLASH_CAPACITY_BYTES.load(Ordering::Relaxed))
                    .and_then(|layout| {
                        flight_tasks::load_flash_config(flash, layout, DEFAULT_STORED_CONFIG).ok()
                    })
                    .map(|(config, _, _)| config)
            } else {
                None
            };
            let config = saved.unwrap_or(DEFAULT_STORED_CONFIG);
            stm32_port::resolve_boot_serial_bindings(
                config.serial_bindings,
                board::serial::DEFAULT_SERIAL_BINDINGS,
                board::serial::SERIAL_ROUTES,
                config.rc_protocol,
            )
        }

        /// The receiver decoder for the protocol RC input was started with.
        pub fn rc_receiver_for(protocol: RcProtocol) -> RcReceiver {
            match protocol {
                RcProtocol::Sbus => RcReceiver::sbus(),
                RcProtocol::Crsf => RcReceiver::crsf(),
            }
        }

        pub fn validate_live_arming_guard(
            permit: bool,
            rc_link_armable: bool,
            arm_high: bool,
            throttle: u32,
        ) -> Result<(), safety::ArmingAbortReason> {
            safety::validate_arming_guard(permit, rc_link_armable, arm_high, throttle)?;
            safety::validate_prearm_health(safety::PreArmHealth {
                host_connected: host_connected(),
                imu_ready: IMU_TRANSPORT_READY.load(Ordering::Acquire)
                    && Spi1ImuKind::from_discriminant(ACTIVE_IMU_KIND.load(Ordering::Acquire))
                        .is_some_and(imu_kind_flight_verified)
                    && IMU_LATEST_SEQ.load(Ordering::Acquire) != 0,
                imu_bias_calibrated: IMU_BIAS_CALIBRATED.load(Ordering::Acquire),
                imu_fresh: !cfg!(feature = "bench_prearm_imu_stale")
                    && !IMU_STALE.load(Ordering::Acquire),
            })
        }

        pub const fn dshot_motor_for_output(output: esc::EscOutput) -> board::init::DshotMotor {
            match output {
                esc::EscOutput::Output1 => board::init::DshotMotor::Motor1,
                esc::EscOutput::Output2 => board::init::DshotMotor::Motor2,
                esc::EscOutput::Output3 => board::init::DshotMotor::Motor3,
                esc::EscOutput::Output4 => board::init::DshotMotor::Motor4,
            }
        }

        /// The register the detected SPI1 IMU starts its DMA sample burst at,
        /// from `ACTIVE_IMU_KIND`'s discriminant; `None` when no IMU was
        /// detected.
        pub const fn spi1_imu_burst_register(discriminant: u8) -> Option<u8> {
            match Spi1ImuKind::from_discriminant(discriminant) {
                Some(kind) => Some(kind.dma_burst_register()),
                None => None,
            }
        }

        pub const fn logical_motor_for_physical_index(physical_index: usize) -> u8 {
            let physical_output = physical_index + 1;
            let mut logical_index = 0;
            while logical_index < board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT.len() {
                if board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT[logical_index]
                    == physical_output
                {
                    return logical_index as u8 + 1;
                }
                logical_index += 1;
            }
            0
        }

        pub const fn logical_motor_for_esc_output(output: esc::EscOutput) -> u8 {
            logical_motor_for_physical_index(output.index())
        }

        pub mod internal {
            pub use crate::*;
        }
    };
}
