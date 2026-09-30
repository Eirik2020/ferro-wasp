use ferrowasp_stm32f4::board_manifest::{
    BoardIdentity, PinAssignment, ResourceClaim, ResourceKind, TimerGroupDescription, TimerMode,
};

pub const BOARD_IDENTITY: BoardIdentity = BoardIdentity {
    target_id: "tbs_lucid_h7",
    name: "TBS Lucid H7",
    mcu: "STM32H743VIT6",
    package: "LQFP100",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardCapabilities {
    pub spi1_imu: bool,
    pub sd_card_storage: bool,
    pub imu_data_ready_exti: bool,
    pub sbus_receiver: bool,
    pub usart3_msp_displayport: bool,
    pub battery_current_adc: bool,
    pub usb_cdc_debug: bool,
    pub dshot_motor_count: u8,
    pub onboard_debug_leds_usable_with_swd: bool,
    pub timer_dma_motor_output: bool,
}

/// The Foxeer F405 V2 feature set on the Lucid's own peripherals. The
/// Lucid's second IMU (SPI4), barometer, MAX7456, motors 5-8, and extra
/// UARTs are not used.
pub const BOARD_CAPABILITIES: BoardCapabilities = BoardCapabilities {
    spi1_imu: true,
    sd_card_storage: true,
    imu_data_ready_exti: true,
    sbus_receiver: true,
    usart3_msp_displayport: true,
    battery_current_adc: true,
    usb_cdc_debug: true,
    dshot_motor_count: 4,
    onboard_debug_leds_usable_with_swd: false,
    timer_dma_motor_output: true,
};

pub const HSE_FREQUENCY_HZ: u32 = 8_000_000;
pub const SYSTEM_CLOCK_HZ: u32 = 400_000_000;
pub const SWD_PINS: &[&str] = &["PA13", "PA14"];
pub const USB_FS_PINS: &[&str] = &["PA11", "PA12"];
pub const IMU_DATA_READY_PIN: &str = "PB2";
pub const CONTROL_SCHEDULER_TIMER: &str = "TIM4";
pub const IO_TIMEBASE_TIMER: &str = "TIM2";
pub const IO_WATCHDOG_TIMER: &str = "TIM6";

pub use ferrowasp_stm32f4_tasks::Spi1ImuKind;

pub const PIN_MAP: &[PinAssignment] = &[
    PinAssignment {
        signal: "MOTOR3_DSHOT",
        pin: "PA0",
        alternate: Some(2),
    },
    PinAssignment {
        signal: "MOTOR4_DSHOT",
        pin: "PA1",
        alternate: Some(2),
    },
    PinAssignment {
        signal: "SPI1_SCK",
        pin: "PA5",
        alternate: Some(5),
    },
    PinAssignment {
        signal: "SPI1_MISO",
        pin: "PA6",
        alternate: Some(5),
    },
    PinAssignment {
        signal: "USB_FS_DM",
        pin: "PA11",
        alternate: Some(10),
    },
    PinAssignment {
        signal: "USB_FS_DP",
        pin: "PA12",
        alternate: Some(10),
    },
    PinAssignment {
        signal: "MOTOR1_DSHOT",
        pin: "PB0",
        alternate: Some(2),
    },
    PinAssignment {
        signal: "MOTOR2_DSHOT",
        pin: "PB1",
        alternate: Some(2),
    },
    PinAssignment {
        signal: "IMU_DATA_READY_EXTI2",
        pin: IMU_DATA_READY_PIN,
        alternate: None,
    },
    PinAssignment {
        signal: "ADC_VOLTAGE",
        pin: "PC0",
        alternate: None,
    },
    PinAssignment {
        signal: "ADC_CURRENT",
        pin: "PC1",
        alternate: None,
    },
    PinAssignment {
        signal: "USART6_TX_RECEIVER",
        pin: "PC6",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "USART6_RX_RECEIVER",
        pin: "PC7",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "SDMMC1_D0",
        pin: "PC8",
        alternate: Some(12),
    },
    PinAssignment {
        signal: "SDMMC1_D1",
        pin: "PC9",
        alternate: Some(12),
    },
    PinAssignment {
        signal: "SDMMC1_D2",
        pin: "PC10",
        alternate: Some(12),
    },
    PinAssignment {
        signal: "SDMMC1_D3",
        pin: "PC11",
        alternate: Some(12),
    },
    PinAssignment {
        signal: "SDMMC1_CK",
        pin: "PC12",
        alternate: Some(12),
    },
    PinAssignment {
        signal: "SPI1_IMU_CS",
        pin: "PC15",
        alternate: None,
    },
    PinAssignment {
        signal: "SDMMC1_CMD",
        pin: "PD2",
        alternate: Some(12),
    },
    PinAssignment {
        signal: "SPI1_MOSI",
        pin: "PD7",
        alternate: Some(5),
    },
    PinAssignment {
        signal: "USART3_TX_MSP_DISPLAYPORT",
        pin: "PD8",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "USART3_RX_MSP_DISPLAYPORT",
        pin: "PD9",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "UART8_RX_ESC_TELEMETRY",
        pin: "PE0",
        alternate: Some(8),
    },
];

pub const DISPATCHER_IRQS: &[&str] = &[
    "FDCAN1_IT0",
    "FDCAN2_IT0",
    "FDCAN1_IT1",
    "FDCAN2_IT1",
    "FDCAN_CAL",
    "SAI1",
    "SAI2",
    "LTDC",
    "DMA2D",
];

pub const HARDWARE_IRQS: &[&str] = &[
    "USART6",
    "DMA1_STR0",
    "USART3",
    "DMA1_STR1",
    "DMA1_STR3",
    "UART8",
    "DMA1_STR2",
    "DMA1_STR4",
    "DMA2_STR0",
    "DMA2_STR1",
    "DMA2_STR2",
    "DMA2_STR3",
    "EXTI2",
    "ADC1_2",
    "TIM4",
    "TIM6_DAC",
    "OTG_FS",
];

pub const TIMER_GROUPS: &[TimerGroupDescription] = &[
    TimerGroupDescription {
        timer: "TIM3",
        mode: TimerMode::Dshot,
        channels: &["TIM3_CH3", "TIM3_CH4"],
    },
    TimerGroupDescription {
        timer: "TIM5",
        mode: TimerMode::Dshot,
        channels: &["TIM5_CH1", "TIM5_CH2"],
    },
    TimerGroupDescription {
        timer: CONTROL_SCHEDULER_TIMER,
        mode: TimerMode::ControlScheduler,
        channels: &[],
    },
    TimerGroupDescription {
        timer: IO_TIMEBASE_TIMER,
        mode: TimerMode::MicrosecondTimebase,
        channels: &[],
    },
    TimerGroupDescription {
        timer: IO_WATCHDOG_TIMER,
        mode: TimerMode::IoWatchdog,
        channels: &[],
    },
];

// DMA claims name the stream and, as `_CH`, its DMAMUX request line.
pub const CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(ResourceKind::Peripheral, "USART6", "SBUS RC input"),
    ResourceClaim::new(ResourceKind::Pin, "PC6", "USART6 TX receiver"),
    ResourceClaim::new(ResourceKind::Pin, "PC7", "USART6 RX receiver"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM0_CH71",
        "USART6 RX SBUS",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART6", "USART6 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STR0", "USART6 RX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "USART3", "DJI MSP DisplayPort"),
    ResourceClaim::new(ResourceKind::Pin, "PD8", "USART3 TX MSP"),
    ResourceClaim::new(ResourceKind::Pin, "PD9", "USART3 RX MSP"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM1_CH45",
        "USART3 RX MSP",
    ),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM3_CH46",
        "USART3 TX MSP",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART3", "USART3 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STR1", "USART3 RX DMA"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STR3", "USART3 TX DMA"),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "SPI1",
        "IMU identity probe and MPU6500/ICM42688-P data path",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PC15", "SPI1 IMU CS"),
    ResourceClaim::new(ResourceKind::Pin, "PA5", "SPI1 SCK"),
    ResourceClaim::new(ResourceKind::Pin, "PA6", "SPI1 MISO"),
    ResourceClaim::new(ResourceKind::Pin, "PD7", "SPI1 MOSI"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM4_CH37", "SPI1 RX"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM5_CH38", "SPI1 TX"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STR4", "SPI1 RX DMA"),
    ResourceClaim::new(ResourceKind::Pin, IMU_DATA_READY_PIN, "IMU data ready"),
    ResourceClaim::new(ResourceKind::Irq, "EXTI2", "IMU data ready"),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "ADC1",
        "Battery and current observation",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PC0", "ADC voltage"),
    ResourceClaim::new(ResourceKind::Pin, "PC1", "ADC current"),
    ResourceClaim::new(ResourceKind::Irq, "ADC1_2", "ADC1 sample completion"),
    ResourceClaim::new(ResourceKind::Peripheral, "TIM3", "Motor 1 and 2 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PB0", "Motor 1 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM3_CH3", "Motor 1 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PB1", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM3_CH4", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::Peripheral, "TIM5", "Motor 3 and 4 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PA0", "Motor 3 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM5_CH1", "Motor 3 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PA1", "Motor 4 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM5_CH2", "Motor 4 DShot"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM0_CH25",
        "Motor 1 DShot",
    ),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STR0",
        "Motor 1 DShot DMA completion",
    ),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM1_CH26",
        "Motor 2 DShot",
    ),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STR1",
        "Motor 2 DShot DMA completion",
    ),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM2_CH55",
        "Motor 3 DShot",
    ),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STR2",
        "Motor 3 DShot DMA completion",
    ),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM3_CH56",
        "Motor 4 DShot",
    ),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STR3",
        "Motor 4 DShot DMA completion",
    ),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "UART8",
        "BLHeli legacy ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PE0", "UART8 RX ESC telemetry"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM2_CH81",
        "UART8 RX ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Irq, "UART8", "UART8 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STR2", "UART8 RX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "SDMMC1", "microSD storage"),
    ResourceClaim::new(ResourceKind::Pin, "PC8", "SDMMC1 D0"),
    ResourceClaim::new(ResourceKind::Pin, "PC9", "SDMMC1 D1"),
    ResourceClaim::new(ResourceKind::Pin, "PC10", "SDMMC1 D2"),
    ResourceClaim::new(ResourceKind::Pin, "PC11", "SDMMC1 D3"),
    ResourceClaim::new(ResourceKind::Pin, "PC12", "SDMMC1 clock"),
    ResourceClaim::new(ResourceKind::Pin, "PD2", "SDMMC1 command"),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        CONTROL_SCHEDULER_TIMER,
        "Control loop scheduler",
    ),
    ResourceClaim::new(
        ResourceKind::Irq,
        CONTROL_SCHEDULER_TIMER,
        "Control loop scheduler",
    ),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        IO_TIMEBASE_TIMER,
        "I/O microsecond timebase",
    ),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        IO_WATCHDOG_TIMER,
        "I/O deadline watchdog",
    ),
    ResourceClaim::new(ResourceKind::Irq, "TIM6_DAC", "I/O deadline watchdog"),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "OTG2_FS",
        "USB CDC diagnostics and configuration",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA11", "USB FS D-"),
    ResourceClaim::new(ResourceKind::Pin, "PA12", "USB FS D+"),
    ResourceClaim::new(ResourceKind::Irq, "OTG_FS", "USB CDC service"),
    ResourceClaim::new(ResourceKind::Irq, "FDCAN1_IT0", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "FDCAN2_IT0", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "FDCAN1_IT1", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "FDCAN2_IT1", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "FDCAN_CAL", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "SAI1", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "SAI2", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "LTDC", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2D", "RTIC software dispatcher"),
];

pub const USB_CDC_CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "OTG2_FS",
        "USB CDC diagnostics and configuration",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA11", "USB FS D-"),
    ResourceClaim::new(ResourceKind::Pin, "PA12", "USB FS D+"),
    ResourceClaim::new(ResourceKind::Irq, "OTG_FS", "USB CDC service"),
];

pub const ESC_TELEMETRY_CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "UART8",
        "BLHeli legacy ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PE0", "UART8 RX ESC telemetry"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM2_CH81",
        "UART8 RX ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Irq, "UART8", "UART8 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STR2", "UART8 RX DMA"),
];

pub const SD_STORAGE_CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(ResourceKind::Peripheral, "SDMMC1", "microSD storage"),
    ResourceClaim::new(ResourceKind::Pin, "PC8", "SDMMC1 D0"),
    ResourceClaim::new(ResourceKind::Pin, "PC9", "SDMMC1 D1"),
    ResourceClaim::new(ResourceKind::Pin, "PC10", "SDMMC1 D2"),
    ResourceClaim::new(ResourceKind::Pin, "PC11", "SDMMC1 D3"),
    ResourceClaim::new(ResourceKind::Pin, "PC12", "SDMMC1 clock"),
    ResourceClaim::new(ResourceKind::Pin, "PD2", "SDMMC1 command"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_stm32f4::board_manifest::{
        ResourceKind, count_claims_by_kind, find_duplicate_claim,
    };

    #[test]
    fn identity_names_the_tbs_lucid_h7() {
        assert_eq!(BOARD_IDENTITY.target_id, "tbs_lucid_h7");
        assert_eq!(BOARD_IDENTITY.name, "TBS Lucid H7");
        assert_eq!(BOARD_IDENTITY.mcu, "STM32H743VIT6");
        assert_eq!(HSE_FREQUENCY_HZ, 8_000_000);
        assert_eq!(SYSTEM_CLOCK_HZ, 400_000_000);
    }

    #[test]
    fn active_capabilities_match_the_foxeer_feature_set() {
        let enabled = [
            BOARD_CAPABILITIES.spi1_imu,
            BOARD_CAPABILITIES.sd_card_storage,
            BOARD_CAPABILITIES.imu_data_ready_exti,
            BOARD_CAPABILITIES.sbus_receiver,
            BOARD_CAPABILITIES.usart3_msp_displayport,
            BOARD_CAPABILITIES.battery_current_adc,
            BOARD_CAPABILITIES.usb_cdc_debug,
            BOARD_CAPABILITIES.timer_dma_motor_output,
        ];

        assert_eq!(enabled, [true; 8]);
        assert!(!BOARD_CAPABILITIES.onboard_debug_leds_usable_with_swd);
        assert_eq!(BOARD_CAPABILITIES.dshot_motor_count, 4);
    }

    #[test]
    fn manifest_has_no_duplicate_claims_and_includes_standard_services() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
        for required in USB_CDC_CLAIMS
            .iter()
            .chain(ESC_TELEMETRY_CLAIMS)
            .chain(SD_STORAGE_CLAIMS)
        {
            assert!(CLAIMS.iter().any(|active| {
                active.kind == required.kind && active.resource == required.resource
            }));
        }
    }

    #[test]
    fn motor_map_follows_the_betaflight_target() {
        let motor = |signal| {
            PIN_MAP
                .iter()
                .find(|assignment| assignment.signal == signal)
                .expect("motor pin must be present")
        };
        let motors = [
            motor("MOTOR1_DSHOT"),
            motor("MOTOR2_DSHOT"),
            motor("MOTOR3_DSHOT"),
            motor("MOTOR4_DSHOT"),
        ];
        assert_eq!(
            motors.map(|motor| (motor.pin, motor.alternate)),
            [
                ("PB0", Some(2)),
                ("PB1", Some(2)),
                ("PA0", Some(2)),
                ("PA1", Some(2)),
            ]
        );
    }

    #[test]
    fn swd_pins_remain_unclaimed() {
        for reserved_pin in SWD_PINS {
            assert!(!CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::Pin && claim.resource == *reserved_pin
            }));
        }
    }

    #[test]
    fn active_claim_counts_match_the_runtime_contract() {
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::DmaStream), 10);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::TimerChannel), 4);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::Peripheral), 12);
    }

    #[test]
    fn hardware_irqs_do_not_overlap_software_dispatchers() {
        for irq in HARDWARE_IRQS {
            assert!(!DISPATCHER_IRQS.contains(irq));
        }
    }

    #[test]
    fn each_timer_has_one_mode() {
        for (index, group) in TIMER_GROUPS.iter().enumerate() {
            assert!(
                !TIMER_GROUPS[index + 1..]
                    .iter()
                    .any(|other| other.timer == group.timer)
            );
        }
    }
}
