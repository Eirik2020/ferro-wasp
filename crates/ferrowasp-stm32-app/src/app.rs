//! What every flight application shares above its board: the names its
//! `main.rs` uses, the USB configurator state, and `flight_app_facade!`, which
//! writes the rest of an application's facade from its board's identity.
//!
//! The facade is a macro rather than a module because its feature checks,
//! board constants, statics, and build stamps belong to the application
//! crate: `cfg!(feature = ..)`, `board::` paths, and `env!` resolve where the
//! macro expands.

/// The names a flight application's `main.rs` uses, whatever its board.
pub mod prelude {
    pub use crate::dt;
    pub use crate::flash_task;
    pub use core::cell::RefCell;
    pub use core::fmt::Write as CoreFmtWrite;
    pub use core::sync::atomic::Ordering;
    pub use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU16, AtomicU32};
    pub use critical_section::Mutex;
    pub use defmt::{info, warn};
    pub use embedded_hal::spi::Operation;
    pub use embedded_hal_async::spi::SpiDevice;
    pub use embedded_io_async::Read as AsyncRead;
    pub use ferrowasp_blackbox::sync_ledger;
    pub use ferrowasp_border::coordinator::{ConfigBoot, ConfigTopic};
    #[cfg(feature = "mspv2_configurator")]
    pub use ferrowasp_cli::blackbox_storage as blackbox_task;
    pub use ferrowasp_cli::command_link::{CommandLink, CommandSources, ResponseRouter};
    pub use ferrowasp_cli::log_sync;
    #[cfg(not(feature = "mspv2_configurator"))]
    pub use ferrowasp_cli::usb_debug;
    pub use ferrowasp_config::boot::Fallback;
    pub use ferrowasp_control::calibration::accel_level::LevelSample;
    pub use ferrowasp_feedback::esc_manager as esc;
    pub use ferrowasp_feedback::esc_manager::{
        DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_IDLE_THROTTLE_COMMAND, DSHOT_PREARM_STOP_HOLD_MS,
    };
    pub use ferrowasp_flight_control::actuator::{remap_motor_outputs, throttle_to_u16};
    pub use ferrowasp_icm42688p as icm;
    pub use ferrowasp_imu::spi::{
        AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiDeadlineUs,
        SpiRequestMailbox,
    };
    pub use ferrowasp_input::rc_receiver::RcReceiver;
    pub use ferrowasp_io_contracts::time::TimestampMicros;
    pub use ferrowasp_mpu6500 as imu;
    pub use ferrowasp_mspv1 as mspv1;
    #[cfg(feature = "mspv2_configurator")]
    pub use ferrowasp_mspv2 as mspv2;
    pub use ferrowasp_osd as osd;
    pub use ferrowasp_safety as safety;
    pub use ferrowasp_safety::actuator as actuator_task;
    pub use ferrowasp_safety::signals::{
        self, ActuatorArmPermitReader, ActuatorArmPermitWriter, RcRatesReader, RcRatesWriter,
    };
    pub use ferrowasp_serial::{Discontinuity, RxChunk, SerialFault};
    pub use ferrowasp_serial::{RcProtocol, ResolvedBindings, SerialBindings, SerialRoute};
    pub use ferrowasp_stm32::app_config::{
        ARMING_GUARD_POLL_MS, BENCH_EQUAL_MOTOR_MAX_THROTTLE, DELAY_TIMER_HZ, ESC_MANAGER_PERIOD_MS,
    };
    pub use ferrowasp_stm32::app_storage as stm32_storage;
    pub use ferrowasp_stm32::memory as stm32_memory;
    pub use ferrowasp_stm32::uart_port as stm32_port;

    pub use crate as flight_tasks;
    pub use crate::{SPI1_MAILBOX, Spi1Device, Spi1Executor, Spi1Mailbox};
    pub use ferrowasp_cli::tasks::usb::{
        ConfiguratorUsbState, FlashRpcCommandProducer, FlashRpcResponseConsumer, MspCommonInfoFn,
    };
    #[cfg(feature = "mspv2_configurator")]
    pub use ferrowasp_cli::tasks::usb::{fixed_bytes, handle_msp_packet, stage_rpc_response};

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
    pub type UsbCdcLink = crate::prelude::backend::usb_serial::UsbCdcLink;
    pub use ferrowasp_border::observer::Ledger;
    pub use ferrowasp_boundary_contracts::messages as observed;
    pub use ferrowasp_boundary_contracts::requests;
    pub use ferrowasp_boundary_contracts::topics::Identity;
    pub use ferrowasp_io_contracts::link::Stamped;
    pub use ferrowasp_stm32::system_reset::RebootTarget;
    /// The producing end of one task's link into the observer.
    pub type ObserverLink<T> = ferrowasp_io_contracts::link::LinkProducer<'static, T>;
    pub type ObserverState = ferrowasp_border::observer::Observer<'static>;
    pub type CoordinatorState = ferrowasp_border::coordinator::Coordinator<'static>;
    pub use ferrowasp_io_contracts::imu::ImuSample;
    /// The control loop's end of the IMU sample link.
    pub type ImuSampleConsumer = ferrowasp_io_contracts::link::LinkConsumer<'static, ImuSample>;
    /// The control loop's and RC input's end of the coordinator's
    /// configuration link.
    pub type FlightConfigConsumer =
        ferrowasp_io_contracts::link::LinkConsumer<'static, crate::dt::FlightConfig>;
    /// The blackbox stream's consuming end, the flash manager's.
    pub type BlackboxStream =
        ferrowasp_io_contracts::link::LinkConsumer<'static, observed::ControlTick>;
    /// The blackbox stream's queue, which a board sizes to ride out its
    /// storage's longest write stall.
    pub type BlackboxStreamQueue<const N: usize> =
        heapless::spsc::Queue<Stamped<observed::ControlTick>, N>;
    pub type FlashRecordProducer = flash_task::RecordProducer;
    pub type FlashRecordConsumer = flash_task::RecordConsumer;
    pub type FlashCommandProducer = flash_task::CommandProducer;
    pub type FlashCommandConsumer = flash_task::CommandConsumer;
    pub type FlashResponseProducer = flash_task::ResponseProducer;
    pub type FlashResponseConsumer = flash_task::ResponseConsumer;
    pub use crate::{FlashRpcCommandConsumer, FlashRpcResponseProducer};
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
        /// What this image was built from, for the `build` command; the
        /// app's build script supplies the values.
        pub const BUILD_IDENTITY: $crate::BuildIdentity = $crate::BuildIdentity {
            revision: env!("FWSP_GIT_REV"),
            dirty: env!("FWSP_GIT_DIRTY").as_bytes()[0] == b'1',
            features: env!("FWSP_FEATURES"),
        };
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
        /// Restart as the safety master decided: into the firmware, or into
        /// the chip's ROM bootloader.
        pub fn reboot_on_request(target: requests::RebootTarget) -> ! {
            stm32_bootloader::reboot(match target {
                requests::RebootTarget::Firmware => {
                    RebootTarget::Firmware
                }
                requests::RebootTarget::RomBootloader => {
                    RebootTarget::RomBootloader
                }
            })
        }
        /// Wake the USB task, which serves its queues on its interrupt.
        pub fn wake_usb() {
            cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
        }
        pub static RC_ARM_HIGH: AtomicBool = AtomicBool::new(false);
        pub static RC_THROTTLE: AtomicU32 = AtomicU32::new(0);
        pub static SAFETY_ARMED: AtomicBool = AtomicBool::new(false);
        pub static MOTOR_OUTPUT_MAP: AtomicU16 = AtomicU16::new(0);
        pub static DSHOT_LAST_RELEASE: AtomicU32 = AtomicU32::new(0);
        /// When the last host lease ends; the safety master writes it.
        pub static HOST_LEASE_UNTIL: AtomicU32 = AtomicU32::new(0);
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
        pub fn msp_common_info(armed: bool, imu_ready: bool) -> mspv2::commands::CommonInfo<'static> {
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
                sensors: u16::from(imu_ready),
                armed,
            }
        }

        /// The board's answer to the configurator's `Hello`.
        #[cfg(feature = "mspv2_configurator")]
        pub fn rpc_device_info(armed: bool) -> mspv2::rpc::DeviceInfo {
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
                armed,
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

        /// The configuration this boot flies: the newest valid slot, or the
        /// board's default, and whether that fell back from the last save.
        /// Read once, in init.
        pub fn boot_configuration(
            flash: &mut FlashDevice,
            probe: &flight_tasks::FlashProbe,
        ) -> ConfigBoot {
            let boot = flight_tasks::boot_config(flash, probe, DEFAULT_STORED_CONFIG);
            match boot.fallback {
                None => {}
                Some(Fallback::Damaged) => {
                    warn!("Config: a saved slot is torn; arming refused until acknowledged")
                }
                Some(Fallback::Invalid) => {
                    warn!("Config: a saved slot was refused; arming refused until acknowledged")
                }
                Some(Fallback::Unreadable) => warn!(
                    "Config: storage unreadable, board default; arming refused until acknowledged"
                ),
            }
            ConfigBoot {
                boot,
                default: DEFAULT_STORED_CONFIG,
                default_bindings: board::serial::DEFAULT_SERIAL_BINDINGS,
            }
        }

        /// The serial bindings to start the ports with, and the RC protocol of
        /// the port bound to RC input, from the boot's configuration, so a
        /// changed binding or protocol applies after a reboot.
        pub fn boot_serial_bindings(boot: &ConfigBoot) -> ResolvedBindings {
            stm32_port::resolve_boot_serial_bindings(
                boot.boot.config.serial_bindings,
                board::serial::DEFAULT_SERIAL_BINDINGS,
                board::serial::SERIAL_ROUTES,
                boot.boot.config.rc_protocol,
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
            host_connected: bool,
            imu: Option<safety::prearm::ImuPreArmStatus>,
        ) -> Result<(), safety::ArmingAbortReason> {
            safety::validate_arming_guard(permit, rc_link_armable, arm_high, throttle)?;
            safety::validate_prearm_health(safety::prearm::prearm_health(
                imu,
                host_connected,
                |kind| Spi1ImuKind::from_discriminant(kind).is_some_and(imu_kind_flight_verified),
                cfg!(feature = "bench_prearm_imu_stale"),
            )?)
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
        /// from the detected kind's discriminant; `None` when no IMU was
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
