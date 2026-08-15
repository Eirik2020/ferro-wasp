//! Mandatory bounded Foxeer flight-service suite declaration.

use crate::hardware_definitions::stm32f4::{
    board_declaration::BoardDeclaration,
    dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
    pins::{GpioPort, PinId},
    service_hardware::{AdcPeripheral, SpiMode, UsbPeripheral},
    spi::SpiPeripheral,
    timer::TimerPeripheral,
};

/// Observation and service component selected by the golden composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GoldenServicesDeclaration {
    /// Application-local component identity.
    pub id: &'static str,
    /// Board ADC hardware identity.
    pub adc_hardware_id: &'static str,
    /// Board SPI NOR hardware identity.
    pub flash_hardware_id: &'static str,
    /// Board USB hardware identity.
    pub usb_hardware_id: &'static str,
    /// Board watchdog-timer identity.
    pub watchdog_hardware_id: &'static str,
    /// ADC DMA interrupt priority.
    pub adc_interrupt_priority: u8,
    /// ADC polling task priority.
    pub adc_poll_priority: u8,
    /// DisplayPort task priority.
    pub osd_priority: u8,
    /// CPU-serviced flash owner priority.
    pub flash_priority: u8,
    /// USB interrupt priority.
    pub usb_priority: u8,
    /// Diagnostic heartbeat priority.
    pub heartbeat_priority: u8,
    /// I/O watchdog interrupt priority.
    pub watchdog_priority: u8,
    /// ADC conversion cadence.
    pub adc_period_ms: u32,
    /// DisplayPort service cadence.
    pub osd_period_ms: u32,
    /// Flash owner service cadence.
    pub flash_period_ms: u32,
    /// Heartbeat cadence.
    pub heartbeat_period_ms: u32,
    /// I/O watchdog update frequency.
    pub watchdog_hz: u32,
    /// Foxeer voltage-divider ratio multiplied by ten.
    pub vbat_divider_x10: u16,
    /// Betaflight-compatible current scale.
    pub current_scale: u32,
    /// Current offset in milliamps.
    pub current_offset_ma: i32,
    /// Maximum plausible cell voltage.
    pub battery_max_cell_mv: u16,
    /// Minimum voltage used for cell-count detection.
    pub battery_detect_cell_mv: u16,
    /// Maximum supported cell count.
    pub battery_max_cells: u8,
}

impl GoldenServicesDeclaration {
    /// Creates the reviewed output-observation and persistence suite.
    pub const fn foxeer(
        id: &'static str,
        adc_hardware_id: &'static str,
        flash_hardware_id: &'static str,
        usb_hardware_id: &'static str,
        watchdog_hardware_id: &'static str,
    ) -> Self {
        Self {
            id,
            adc_hardware_id,
            flash_hardware_id,
            usb_hardware_id,
            watchdog_hardware_id,
            adc_interrupt_priority: 1,
            adc_poll_priority: 1,
            osd_priority: 3,
            flash_priority: 1,
            usb_priority: 5,
            heartbeat_priority: 1,
            watchdog_priority: 9,
            adc_period_ms: 100,
            osd_period_ms: 10,
            flash_period_ms: 1,
            heartbeat_period_ms: 2_000,
            watchdog_hz: 8_000,
            vbat_divider_x10: 110,
            current_scale: 70,
            current_offset_ma: 0,
            battery_max_cell_mv: 4_300,
            battery_detect_cell_mv: 3_000,
            battery_max_cells: 8,
        }
    }
}

/// Shared ADC1 DMA transfer resource.
pub const ADC_TRANSFER: &str = "adc1_transfer";
/// Task-local spare ADC DMA buffer.
pub const ADC_BUFFER: &str = "adc1_buffer";
/// Task-local ADC DMA interrupt planner.
pub const ADC_PLANNER: &str = "adc1_planner";
/// Task-local battery cell-count detector.
pub const BATTERY_CELL_DETECTOR: &str = "battery_cell_detector";
/// Shared bounded service telemetry snapshot.
pub const SERVICE_TELEMETRY: &str = "flight_service_telemetry";
/// Shared runtime tuning profile.
pub const TUNING_PROFILE: &str = "tuning_profile";
/// Shared runtime tuning request sequence.
pub const TUNING_REQUEST_SEQ: &str = "tuning_request_seq";
/// Shared blackbox rate divisor.
pub const LOG_RATE_DIVISOR: &str = "flash_log_rate_divisor";
/// Shared NOR/config storage status.
pub const STORAGE_STATUS: &str = "storage_status";
/// Shared request for one USB diagnostic status frame.
pub const USB_STATUS_DUE: &str = "usb_status_due";
/// Task-local MSP DisplayPort state machine.
pub const OSD_TASK: &str = "osd_task_state";
/// Task-local bounded MSP transmit buffer.
pub const OSD_BUFFER: &str = "osd_tx_buffer";
/// Task-local DisplayPort refresh divider.
pub const OSD_REFRESH_TICK: &str = "osd_refresh_tick";
/// Task-local UART4 transmit health latch.
pub const OSD_TX_HEALTHY: &str = "osd_tx_healthy";
/// Task-local SPI2 NOR device owner.
pub const FLASH_DEVICE: &str = "flash_device";
/// Task-local blackbox record queue producer.
pub const FLASH_RECORD_PRODUCER: &str = "flash_record_producer";
/// Task-local blackbox record queue consumer.
pub const FLASH_RECORD_CONSUMER: &str = "flash_record_consumer";
/// Task-local USB-to-flash command queue producer.
pub const FLASH_COMMAND_PRODUCER: &str = "flash_command_producer";
/// Task-local USB-to-flash command queue consumer.
pub const FLASH_COMMAND_CONSUMER: &str = "flash_command_consumer";
/// Task-local flash-to-USB response queue producer.
pub const FLASH_RESPONSE_PRODUCER: &str = "flash_response_producer";
/// Task-local flash-to-USB response queue consumer.
pub const FLASH_RESPONSE_CONSUMER: &str = "flash_response_consumer";
/// Task-local bounded flash manager state.
pub const FLASH_MANAGER_STATE: &str = "flash_manager_state";
/// Task-local USB CDC device.
pub const USB_DEVICE: &str = "usb_device";
/// Task-local buffered USB CDC serial class.
pub const USB_SERIAL: &str = "usb_serial";
/// Task-local one-shot USB diagnostic header state.
pub const USB_HEADER_SENT: &str = "usb_header_sent";
/// Task-local whitelisted diagnostic command parser.
pub const USB_COMMAND_PARSER: &str = "usb_command_parser";
/// Task-local partially transmitted response.
pub const USB_PENDING_RESPONSE: &str = "usb_pending_response";
/// Task-local TIM6 I/O watchdog counter.
pub const IO_WATCHDOG: &str = "io_watchdog";

/// Concrete type of one component-owned task-local resource.
pub fn local_rust_type(id: &str) -> Option<&'static str> {
    match id {
        ADC_BUFFER => Some("Option<&'static mut [u16; 3]>"),
        ADC_PLANNER => Some("AdcDmaIrqPlanner"),
        BATTERY_CELL_DETECTOR => Some("BatteryCellDetector"),
        OSD_TASK => Some("OsdTask"),
        OSD_BUFFER => Some("[u8; ferrowasp_mspv1::OSD_TX_BUFFER_LEN]"),
        OSD_REFRESH_TICK => Some("u8"),
        OSD_TX_HEALTHY => Some("bool"),
        FLASH_DEVICE => Some("Spi2Flash"),
        FLASH_RECORD_PRODUCER => Some("RecordProducer"),
        FLASH_RECORD_CONSUMER => Some("RecordConsumer"),
        FLASH_COMMAND_PRODUCER => Some("CommandProducer"),
        FLASH_COMMAND_CONSUMER => Some("CommandConsumer"),
        FLASH_RESPONSE_PRODUCER => Some("ResponseProducer"),
        FLASH_RESPONSE_CONSUMER => Some("ResponseConsumer"),
        FLASH_MANAGER_STATE => Some("GoldenFlashState"),
        USB_DEVICE => Some("UsbCdcDevice"),
        USB_SERIAL => Some("BufferedUsbCdcSerial"),
        USB_HEADER_SENT => Some("bool"),
        USB_COMMAND_PARSER => Some("CommandParser"),
        USB_PENDING_RESPONSE => Some("Option<ResponseFrame>"),
        IO_WATCHDOG => Some("IoWatchdog"),
        _ => None,
    }
}

/// Concrete type of one component-owned RTIC shared resource.
pub fn shared_rust_type(id: &str) -> Option<&'static str> {
    match id {
        ADC_TRANSFER => Some("Adc1ObservationTransfer"),
        SERVICE_TELEMETRY => Some("FlightServiceTelemetry"),
        TUNING_PROFILE => Some("dt::TuningProfile"),
        TUNING_REQUEST_SEQ | LOG_RATE_DIVISOR => Some("u32"),
        STORAGE_STATUS => Some("StorageStatus"),
        USB_STATUS_DUE => Some("bool"),
        _ => None,
    }
}

pub(crate) fn validate(
    services: GoldenServicesDeclaration,
    board: &BoardDeclaration,
) -> Result<(), String> {
    let adc = board
        .adc_observation(services.adc_hardware_id)
        .ok_or_else(|| {
            format!(
                "golden services `{}` consume missing ADC hardware `{}`",
                services.id, services.adc_hardware_id
            )
        })?;
    if adc.peripheral != AdcPeripheral::Adc1
        || adc.voltage_pin != PinId::new(GpioPort::C, 0)
        || adc.voltage_channel != 10
        || adc.current_pin != PinId::new(GpioPort::C, 1)
        || adc.current_channel != 11
        || adc.dma
            != DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream4,
                DmaChannel::Channel0,
            )
    {
        return Err("golden services require ADC1 PC0/PC1 on DMA2 Stream4 Channel0".to_owned());
    }

    let flash = board.spi_nor(services.flash_hardware_id).ok_or_else(|| {
        format!(
            "golden services `{}` consume missing SPI NOR hardware `{}`",
            services.id, services.flash_hardware_id
        )
    })?;
    if flash.peripheral != SpiPeripheral::Spi2
        || flash.chip_select != PinId::new(GpioPort::B, 12)
        || flash.sck != PinId::new(GpioPort::B, 13)
        || flash.miso != PinId::new(GpioPort::C, 2)
        || flash.mosi != PinId::new(GpioPort::C, 3)
        || flash.mode != SpiMode::Mode0
        || flash.frequency_hz != 10_000_000
    {
        return Err("golden services require the reviewed CPU-serviced SPI2 NOR route".to_owned());
    }

    let usb = board.usb_cdc(services.usb_hardware_id).ok_or_else(|| {
        format!(
            "golden services `{}` consume missing USB hardware `{}`",
            services.id, services.usb_hardware_id
        )
    })?;
    if usb.peripheral != UsbPeripheral::OtgFs
        || usb.dm != PinId::new(GpioPort::A, 11)
        || usb.dp != PinId::new(GpioPort::A, 12)
    {
        return Err("golden services require OTG_FS on PA11/PA12".to_owned());
    }

    let watchdog = board.timer(services.watchdog_hardware_id).ok_or_else(|| {
        format!(
            "golden services `{}` consume missing watchdog timer `{}`",
            services.id, services.watchdog_hardware_id
        )
    })?;
    if watchdog.peripheral != TimerPeripheral::Tim6
        || services.watchdog_hz != 8_000
        || services.adc_interrupt_priority != 1
        || services.adc_poll_priority != 1
        || services.osd_priority != 3
        || services.flash_priority != 1
        || services.usb_priority != 5
        || services.heartbeat_priority != 1
        || services.watchdog_priority != 9
        || services.vbat_divider_x10 != 110
        || services.current_scale != 70
        || services.current_offset_ma != 0
        || services.battery_max_cell_mv != 4_300
        || services.battery_detect_cell_mv != 3_000
        || services.battery_max_cells != 8
    {
        return Err("golden service priorities, timing, or ADC profile drifted".to_owned());
    }
    Ok(())
}
