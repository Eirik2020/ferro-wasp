// GENERATED FILE — DO NOT EDIT DIRECTLY
// Generated with src/main.rs from the selected application declarations.

//! Crate-local imports used by the generated RTIC application.

#![allow(unused_imports)]

use defmt_rtt as _;
use panic_halt as _;

pub(crate) use crate::platform_config::load_platform_config;
pub(crate) use core::fmt::Write as FmtWrite;
pub(crate) use embedded_io_async::{Read, Write};
pub(crate) use ferrowasp_core::frames::FrameRotation;
pub(crate) use ferrowasp_core::{
    safety::{
        ActuatorAuthority, ActuatorCmd, ActuatorGuardReport, ActuatorPreparationReport,
        ArmingAbortReason, MOTOR_CMD_MAX_AGE_MS, MotorCmd, PreArmHealth, PreArmHealthReport,
        RcLinkInvalidation,
    },
    safety_channel::{SafetyChannel, SafetyConsumer, SafetyProducer},
};
pub(crate) use ferrowasp_drivers::mpu6500::ImuData;
pub(crate) use ferrowasp_io_core::serial::RcInputSnapshot;
pub(crate) use ferrowasp_io_core::{
    platform_config::{ImuInstallationId, RuntimePlatformConfig, SerialService, SpiService},
    serial::SerialFault,
    spi::SpiDeviceError,
};
pub(crate) use ferrowasp_stm32f4::rtic::hal::spi;
pub(crate) use ferrowasp_stm32f4::scheduler::{acknowledge_control_tick, init_control_scheduler};
pub(crate) use ferrowasp_stm32f4::{
    adc::{AdcDmaDeliveryError, AdcDmaIrqPlanner, take_completed_adc1_sample_for},
    app_storage::{AdcBufferBank, AdcStorageResources},
    usb_serial::{BufferedUsbCdcSerial, UsbCdcDevice, UsbCdcIdentity, UsbDeviceState},
    watchdog::acknowledge_watchdog_tick,
};
pub(crate) use ferrowasp_stm32f4::{
    app_storage::{SpiDmaBufferBank, SpiDmaStorageResources, SpiFilledQueue, SpiFreeQueue},
    memory, spi_dma,
    spi_imu_endpoint::{
        IMU_KIND_ICM42688P, IMU_KIND_MPU6500, IMU_KIND_NONE, Spi1ImuDevice, Spi1ImuEndpointOwner,
        Spi1ImuMailbox, Spi1ImuParser, SpiImuOwnerOutcome, SpiImuRxOutcome, SpiImuTimeoutOutcome,
        new_spi1_imu_mailbox,
    },
};
pub(crate) use ferrowasp_stm32f4::{
    app_storage::{
        UartRxBufferBank, UartRxFilledQueue, UartRxFreeQueue, UartRxStorageResources, UartTxBuffer,
    },
    memory::{UartOwnedDiscontinuities, UartOwnedReader, UartOwnedRxChannel},
    memory::{UartOwnedTxChannel, UartOwnedTxCompletion, UartOwnedTxOwner, UartOwnedWriter},
    rtic::prelude::*,
    serial::{
        UartOwnedRxBridgeOutcome, UartOwnedRxBridgeService, UartRxIrqOutcome, UartRxIrqService,
    },
    serial::{UartTxDmaService, UartTxIrqOutcome, UartTxStartError},
    uart_dma::{
        UART_TX_BUFFER_SIZE, Uart2RxIrq, Uart2TxDmaSide, Uart4EndpointResources, Uart4RxIrq,
        Uart4TxDmaSide, UartOwnedRxBridge, Usart2EndpointResources,
    },
};
pub(crate) use ferrowasp_stm32f4::{
    dshot::{
        DshotDmaStorage, DshotInterruptEvent, DshotMotor, DshotMotorBank, DshotServiceEvent,
        DshotTelemetryRequestError,
    },
    uart_dma::{Uart1RxIrq, UartRxParserSide, UartRxReadStatus, Usart1EscTelemetryResources},
};
pub(crate) use ferrowasp_tasks::esc_manager::{
    DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_IDLE_THROTTLE_COMMAND, DSHOT_PREARM_STOP_HOLD_MS,
    EscAckConsumer, EscAckOutcome, EscAckProducer, EscAckQueue, EscActuatorAck, EscActuatorRequest,
    EscIdleQualification, EscIdleQualificationFailure, EscIdleQualificationStatus, EscManager,
    EscManagerConfig, EscOutput, EscRequestConsumer, EscRequestProducer, EscRequestQueue,
    EscTelemetryUpdateConsumer, EscTelemetryUpdateProducer, EscTelemetryUpdateQueue,
};
pub(crate) use ferrowasp_tasks::{
    flash_storage::{
        CommandConsumer, CommandParser, CommandProducer, CommandQueue, GoldenFlashOperation,
        GoldenFlashState, RecordConsumer, RecordEnqueueOutcome, RecordProducer, RecordQueue,
        ResponseConsumer, ResponseFrame, ResponseProducer, ResponseQueue, StorageCommand,
        StorageLayout, emit_page_hex_lines, format_log_info_response, load_config, scan_log,
    },
    osd::{BatteryCellDetector, OsdStickRates, OsdTask},
    service_telemetry::{FlightServiceTelemetry, StorageStatus},
    usb_debug::{self, ImuKind, StatusSnapshot},
};
pub(crate) type Adc1ObservationTransfer = ferrowasp_stm32f4::adc::Adc1ObservationTransferFor<
    ferrowasp_stm32f4::rtic::hal::dma::Stream4<ferrowasp_stm32f4::rtic::hal::pac::DMA2>,
    0,
>;
pub(crate) type Spi2Flash = ferrowasp_drivers::spi_nor::SpiNor<
    ferrowasp_stm32f4::rtic::hal::spi::Spi<ferrowasp_stm32f4::rtic::hal::pac::SPI2>,
    ferrowasp_stm32f4::rtic::hal::gpio::Pin<
        'B',
        12,
        ferrowasp_stm32f4::rtic::hal::gpio::Output<ferrowasp_stm32f4::rtic::hal::gpio::PushPull>,
    >,
>;
pub(crate) type IoWatchdog =
    ferrowasp_stm32f4::rtic::hal::timer::CounterHz<ferrowasp_stm32f4::rtic::hal::pac::TIM6>;
pub(crate) use ferrowasp_tasks::drone_toolbox as dt;
