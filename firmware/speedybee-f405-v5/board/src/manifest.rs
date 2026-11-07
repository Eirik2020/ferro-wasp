use ferrowasp_stm32::board_manifest::{
    BoardIdentity, PinAssignment, ResourceClaim, ResourceKind, TimerGroupDescription, TimerMode,
};

pub const BOARD_IDENTITY: BoardIdentity = BoardIdentity {
    target_id: "speedybee_f405_v5",
    name: "SpeedyBee F405 V5",
    mcu: "STM32F405RGT6",
    package: "LQFP64",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardCapabilities {
    pub spi1_imu: bool,
    pub spi3_flash: bool,
    pub imu_data_ready_exti: bool,
    pub sbus_receiver_on_o4_connector: bool,
    pub uart1_configurator_line: bool,
    pub uart5_esc_telemetry: bool,
    pub battery_current_adc: bool,
    pub usb_cdc_debug: bool,
    pub spl06_baro_i2c: bool,
    pub inverted_buzzer: bool,
    pub gyro_clkin_output: bool,
    pub dshot_motor_count: u8,
}

// The MAX7456 OSD chip sits on SPI2 and the servo, LED-strip, camera-control,
// PPM and PINIO pads exist, but FerroWasp has no driver for any of them yet;
// those pads stay unclaimed until one lands, so a later change owns them
// outright rather than racing this bring-up's claims.
pub const BOARD_CAPABILITIES: BoardCapabilities = BoardCapabilities {
    spi1_imu: true,
    spi3_flash: true,
    imu_data_ready_exti: true,
    sbus_receiver_on_o4_connector: true,
    uart1_configurator_line: true,
    uart5_esc_telemetry: true,
    battery_current_adc: true,
    usb_cdc_debug: true,
    spl06_baro_i2c: true,
    inverted_buzzer: true,
    gyro_clkin_output: true,
    dshot_motor_count: 4,
};

pub const HSE_FREQUENCY_HZ: u32 = 8_000_000;
pub const SYSTEM_CLOCK_HZ: u32 = 168_000_000;
pub const SWD_PINS: &[&str] = &["PA13", "PA14"];
pub const USB_FS_PINS: &[&str] = &["PA11", "PA12"];
pub const IMU_DATA_READY_PIN: &str = "PC4";
/// The ICM42688-P's CLKIN, PB15, which the target drives from TIM12 channel 2
/// (the F405's only timer channel on the pin, alternate function 9). The
/// exact rate Betaflight drives is settled at bench bring-up; the pin and its
/// timer are the board's wiring either way.
pub const GYRO_1_CLKIN_PIN: &str = "PB15";
/// The SPL06-007 barometer's seven-bit address on I2C1: ArduPilot's
/// `speedybeef4v5` hwdefs probe `BARO SPL06 I2C:0:0x76`.
pub const BARO_I2C_ADDRESS: u8 = 0x76;
pub const CONTROL_SCHEDULER_TIMER: &str = "TIM4";
/// TIM5, not the Foxeer's TIM2: this board's motors 3 and 4 sit on TIM2
/// channels 3 and 4, and TIM5 is the F405's other 32-bit counter.
pub const IO_TIMEBASE_TIMER: &str = "TIM5";
pub const IO_WATCHDOG_TIMER: &str = "TIM6";

pub use ferrowasp_stm32::spi1_imu::Spi1ImuKind;

pub const PIN_MAP: &[PinAssignment] = &[
    PinAssignment {
        signal: "SPI1_IMU_CS",
        pin: "PA4",
        alternate: None,
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
        signal: "SPI1_MOSI",
        pin: "PA7",
        alternate: Some(5),
    },
    PinAssignment {
        signal: "USART1_TX_CONFIGURATOR",
        pin: "PA9",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "USART1_RX_CONFIGURATOR",
        pin: "PA10",
        alternate: Some(7),
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
        signal: "MOTOR2_DSHOT",
        pin: "PB0",
        alternate: Some(2),
    },
    PinAssignment {
        signal: "MOTOR1_DSHOT",
        pin: "PB1",
        alternate: Some(2),
    },
    PinAssignment {
        signal: "SPI3_FLASH_SCK",
        pin: "PB3",
        alternate: Some(6),
    },
    PinAssignment {
        signal: "SPI3_FLASH_MISO",
        pin: "PB4",
        alternate: Some(6),
    },
    PinAssignment {
        signal: "SPI3_FLASH_MOSI",
        pin: "PB5",
        alternate: Some(6),
    },
    PinAssignment {
        signal: "I2C1_SCL_BARO",
        pin: "PB6",
        alternate: Some(4),
    },
    PinAssignment {
        signal: "I2C1_SDA_BARO",
        pin: "PB7",
        alternate: Some(4),
    },
    PinAssignment {
        signal: "BEEPER_INVERTED",
        pin: "PB9",
        alternate: None,
    },
    PinAssignment {
        signal: "MOTOR3_DSHOT",
        pin: "PB10",
        alternate: Some(1),
    },
    PinAssignment {
        signal: "MOTOR4_DSHOT",
        pin: "PB11",
        alternate: Some(1),
    },
    PinAssignment {
        signal: "GYRO_1_CLKIN",
        pin: GYRO_1_CLKIN_PIN,
        alternate: Some(9),
    },
    PinAssignment {
        signal: "ADC_VOLTAGE",
        pin: "PC1",
        alternate: None,
    },
    PinAssignment {
        signal: "IMU_DATA_READY_EXTI4",
        pin: IMU_DATA_READY_PIN,
        alternate: None,
    },
    PinAssignment {
        signal: "ADC_CURRENT",
        pin: "PC5",
        alternate: None,
    },
    PinAssignment {
        signal: "USART6_RX_RC_O4",
        pin: "PC7",
        alternate: Some(8),
    },
    PinAssignment {
        signal: "SPI3_FLASH_CS",
        pin: "PC12",
        alternate: None,
    },
    PinAssignment {
        signal: "UART5_RX_ESC_TELEMETRY",
        pin: "PD2",
        alternate: Some(8),
    },
];

pub const DISPATCHER_IRQS: &[&str] = &[
    "CAN1_TX",
    "CAN2_TX",
    "CAN1_RX0",
    "CAN1_RX1",
    "CAN1_SCE",
    "CAN2_RX0",
    "CAN2_RX1",
    "OTG_HS_EP1_OUT",
    "OTG_HS_EP1_IN",
    "SDIO",
    "CAN2_SCE",
];

pub const HARDWARE_IRQS: &[&str] = &[
    "USART1",
    "DMA2_STREAM5",
    "DMA2_STREAM7",
    "USART6",
    "DMA2_STREAM2",
    "UART5",
    "DMA1_STREAM0",
    "DMA2_STREAM0",
    "DMA2_STREAM3",
    "EXTI4",
    "DMA2_STREAM4",
    "DMA1_STREAM2",
    "DMA1_STREAM7",
    "DMA1_STREAM1",
    "DMA1_STREAM6",
    "TIM4",
    "TIM6_DAC",
    "OTG_FS",
];

// The motor bank spans two timers, and they start as one: TIM3 leads TIM2
// through the F405's internal trigger route TIM2 ITR2 (FerroHAL's
// `timer_trigger!(TIM2: TIM3, 2)`), the arrangement FerroHAL's run R proved on
// hardware with all four lanes, so a DShot frame leaves the four motor pads
// on the same edge. The scheduler, timebase and watchdog timers follow the
// Foxeer F405 V2's choices where this board's TIM2 is not free for the
// microsecond timebase; TIM5 is the F405's other 32-bit counter.
pub const TIMER_GROUPS: &[TimerGroupDescription] = &[
    TimerGroupDescription {
        timer: "TIM3",
        mode: TimerMode::Dshot,
        channels: &["TIM3_CH4", "TIM3_CH3"],
    },
    TimerGroupDescription {
        timer: "TIM2",
        mode: TimerMode::Dshot,
        channels: &["TIM2_CH3", "TIM2_CH4"],
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
    TimerGroupDescription {
        timer: "TIM12",
        mode: TimerMode::StaticPwm,
        channels: &["TIM12_CH2"],
    },
];

pub const CLAIMS: &[ResourceClaim] = &[
    // The configurator line on USART1, both ways.
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "USART1",
        "UART1 configurator line",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA9", "USART1 TX configurator"),
    ResourceClaim::new(ResourceKind::Pin, "PA10", "USART1 RX configurator"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM5_CH4",
        "USART1 configurator RX",
    ),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM7_CH4",
        "USART1 configurator TX",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART1", "USART1 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM5", "USART1 RX DMA"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM7", "USART1 TX DMA"),
    // SBUS RC on the DJI O4 connector's receive line, USART6.
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "USART6",
        "UART6 SBUS receiver on the DJI O4 connector",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PC7", "USART6 RX receiver"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM2_CH5",
        "USART6 receiver RX",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART6", "USART6 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM2", "USART6 RX DMA"),
    // ESC telemetry on UART5, receive only.
    ResourceClaim::new(ResourceKind::Peripheral, "UART5", "BLHeli ESC telemetry"),
    ResourceClaim::new(ResourceKind::Pin, "PD2", "UART5 RX ESC telemetry"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM0_CH4",
        "UART5 RX ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Irq, "UART5", "UART5 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM0", "UART5 RX DMA"),
    // The IMU on SPI1 and its data-ready line.
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "SPI1",
        "IMU identity probe and ICM42688-P data path",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA4", "SPI1 IMU CS"),
    ResourceClaim::new(ResourceKind::Pin, "PA5", "SPI1 SCK"),
    ResourceClaim::new(ResourceKind::Pin, "PA6", "SPI1 MISO"),
    ResourceClaim::new(ResourceKind::Pin, "PA7", "SPI1 MOSI"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM0_CH3", "SPI1 RX"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM3_CH3", "SPI1 TX"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM0", "SPI1 RX DMA"),
    ResourceClaim::new(ResourceKind::Pin, IMU_DATA_READY_PIN, "IMU data ready"),
    ResourceClaim::new(ResourceKind::Irq, "EXTI4", "IMU data ready"),
    // The SPL06 barometer on I2C1 and the inverted buzzer.
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "I2C1",
        "SPL06-007 barometer (address 0x76)",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PB6", "I2C1 SCL barometer"),
    ResourceClaim::new(ResourceKind::Pin, "PB7", "I2C1 SDA barometer"),
    ResourceClaim::new(ResourceKind::Pin, "PB9", "Inverted beeper"),
    // The battery observation on ADC1.
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "ADC1",
        "Battery and current observation",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PC1", "ADC voltage"),
    ResourceClaim::new(ResourceKind::Pin, "PC5", "ADC current"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM4_CH0",
        "ADC1 observation",
    ),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM4", "ADC1 DMA"),
    // The motor bank: TIM3 channels 4 and 3 on PB1 and PB0, and TIM2 channels
    // 3 and 4 on PB10 and PB11, every stream on DMA1.
    ResourceClaim::new(ResourceKind::Peripheral, "TIM3", "Motor 1 and 2 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PB1", "Motor 1 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM3_CH4", "Motor 1 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PB0", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM3_CH3", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::Peripheral, "TIM2", "Motor 3 and 4 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PB10", "Motor 3 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM2_CH3", "Motor 3 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PB11", "Motor 4 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM2_CH4", "Motor 4 DShot"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM2_CH5", "Motor 1 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA1_STREAM2",
        "Motor 1 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM7_CH5", "Motor 2 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA1_STREAM7",
        "Motor 2 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM1_CH3", "Motor 3 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA1_STREAM1",
        "Motor 3 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM6_CH3", "Motor 4 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA1_STREAM6",
        "Motor 4 DShot DMA completion",
    ),
    // The gyro clock on TIM12 channel 2.
    ResourceClaim::new(ResourceKind::Peripheral, "TIM12", "IMU CLKIN static PWM"),
    ResourceClaim::new(ResourceKind::Pin, GYRO_1_CLKIN_PIN, "IMU CLKIN"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM12_CH2", "IMU CLKIN"),
    // Blackbox storage on SPI3.
    ResourceClaim::new(ResourceKind::Peripheral, "SPI3", "Onboard SPI NOR storage"),
    ResourceClaim::new(ResourceKind::Pin, "PC12", "SPI3 flash chip select"),
    ResourceClaim::new(ResourceKind::Pin, "PB3", "SPI3 flash clock"),
    ResourceClaim::new(ResourceKind::Pin, "PB4", "SPI3 flash MISO"),
    ResourceClaim::new(ResourceKind::Pin, "PB5", "SPI3 flash MOSI"),
    // The scheduler, timebase and watchdog timers.
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
    // USB CDC.
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "OTG_FS",
        "USB CDC diagnostics and configuration",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA11", "USB FS D-"),
    ResourceClaim::new(ResourceKind::Pin, "PA12", "USB FS D+"),
    ResourceClaim::new(ResourceKind::Irq, "OTG_FS", "USB CDC service"),
    // RTIC's software dispatchers.
    ResourceClaim::new(ResourceKind::Irq, "CAN1_TX", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "CAN2_TX", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "CAN1_RX0", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "CAN1_RX1", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "CAN1_SCE", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "CAN2_RX0", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "CAN2_RX1", "RTIC software dispatcher"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "OTG_HS_EP1_OUT",
        "RTIC software dispatcher",
    ),
    ResourceClaim::new(
        ResourceKind::Irq,
        "OTG_HS_EP1_IN",
        "RTIC software dispatcher",
    ),
    ResourceClaim::new(ResourceKind::Irq, "SDIO", "RTIC software dispatcher"),
    ResourceClaim::new(ResourceKind::Irq, "CAN2_SCE", "RTIC software dispatcher"),
];

pub const USB_CDC_CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "OTG_FS",
        "USB CDC diagnostics and configuration",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA11", "USB FS D-"),
    ResourceClaim::new(ResourceKind::Pin, "PA12", "USB FS D+"),
    ResourceClaim::new(ResourceKind::Irq, "OTG_FS", "USB CDC service"),
];

pub const ESC_TELEMETRY_CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(ResourceKind::Peripheral, "UART5", "BLHeli ESC telemetry"),
    ResourceClaim::new(ResourceKind::Pin, "PD2", "UART5 RX ESC telemetry"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM0_CH4",
        "UART5 RX ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Irq, "UART5", "Optional UART5 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM0", "Optional UART5 RX DMA"),
];

pub const SPI_FLASH_CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(ResourceKind::Peripheral, "SPI3", "Onboard SPI NOR storage"),
    ResourceClaim::new(ResourceKind::Pin, "PC12", "SPI3 flash chip select"),
    ResourceClaim::new(ResourceKind::Pin, "PB3", "SPI3 flash clock"),
    ResourceClaim::new(ResourceKind::Pin, "PB4", "SPI3 flash MISO"),
    ResourceClaim::new(ResourceKind::Pin, "PB5", "SPI3 flash MOSI"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_stm32::board_manifest::{
        ResourceKind, count_claims_by_kind, find_duplicate_claim,
    };

    #[test]
    fn identity_names_the_speedybee_f405_v5() {
        assert_eq!(BOARD_IDENTITY.target_id, "speedybee_f405_v5");
        assert_eq!(BOARD_IDENTITY.name, "SpeedyBee F405 V5");
        assert_eq!(BOARD_IDENTITY.mcu, "STM32F405RGT6");
        assert_eq!(HSE_FREQUENCY_HZ, 8_000_000);
        assert_eq!(SYSTEM_CLOCK_HZ, 168_000_000);
    }

    #[test]
    fn active_capabilities_include_the_bringup_services() {
        let enabled = [
            BOARD_CAPABILITIES.spi1_imu,
            BOARD_CAPABILITIES.spi3_flash,
            BOARD_CAPABILITIES.imu_data_ready_exti,
            BOARD_CAPABILITIES.sbus_receiver_on_o4_connector,
            BOARD_CAPABILITIES.uart1_configurator_line,
            BOARD_CAPABILITIES.uart5_esc_telemetry,
            BOARD_CAPABILITIES.battery_current_adc,
            BOARD_CAPABILITIES.usb_cdc_debug,
            BOARD_CAPABILITIES.spl06_baro_i2c,
            BOARD_CAPABILITIES.inverted_buzzer,
            BOARD_CAPABILITIES.gyro_clkin_output,
        ];

        assert_eq!(enabled, [true; 11]);
        assert_eq!(BOARD_CAPABILITIES.dshot_motor_count, 4);
    }

    #[test]
    fn manifest_has_no_duplicate_claims_and_includes_standard_services() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
        assert_eq!(find_duplicate_claim(USB_CDC_CLAIMS), None);
        assert_eq!(find_duplicate_claim(ESC_TELEMETRY_CLAIMS), None);
        assert_eq!(find_duplicate_claim(SPI_FLASH_CLAIMS), None);
        for required in USB_CDC_CLAIMS
            .iter()
            .chain(ESC_TELEMETRY_CLAIMS)
            .chain(SPI_FLASH_CLAIMS)
        {
            assert!(CLAIMS.iter().any(|active| {
                active.kind == required.kind && active.resource == required.resource
            }));
        }
    }

    #[test]
    fn motor_map_preserves_the_target_dshot_outputs() {
        let motor = |signal: &str| {
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
            [
                (motors[0].pin, motors[0].alternate),
                (motors[1].pin, motors[1].alternate),
                (motors[2].pin, motors[2].alternate),
                (motors[3].pin, motors[3].alternate),
            ],
            [
                ("PB1", Some(2)),
                ("PB0", Some(2)),
                ("PB10", Some(1)),
                ("PB11", Some(1)),
            ]
        );
    }

    #[test]
    fn the_bank_spans_two_timers_with_every_lane_stream_on_dma1() {
        // TIM3 channels 4 and 3 carry motors 1 and 2; TIM2 channels 3 and 4
        // carry motors 3 and 4. TIM3 leads TIM2 from one start (run R), and
        // because every motor stream sits on DMA1, DMA2 serves only the IMU,
        // the UARTs and the ADC.
        for (timer, channel) in [
            ("TIM3", "TIM3_CH4"),
            ("TIM3", "TIM3_CH3"),
            ("TIM2", "TIM2_CH3"),
            ("TIM2", "TIM2_CH4"),
        ] {
            assert!(CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::TimerChannel
                    && claim.resource == channel
                    && claim.owner.contains("DShot")
            }));
            let _ = timer;
        }
        for stream in [
            "DMA1_STREAM2_CH5",
            "DMA1_STREAM7_CH5",
            "DMA1_STREAM1_CH3",
            "DMA1_STREAM6_CH3",
        ] {
            assert!(CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::DmaStream && claim.resource == stream
            }));
        }
    }

    #[test]
    fn active_claim_counts_match_the_bringup_contract() {
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::DmaStream), 11);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::TimerChannel), 5);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::Peripheral), 14);
    }

    #[test]
    fn swd_pins_remain_unclaimed_while_imu_irq_and_usb_are_owned() {
        for reserved_pin in SWD_PINS {
            assert!(!CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::Pin && claim.resource == *reserved_pin
            }));
        }
        assert!(CLAIMS.iter().any(|claim| {
            claim.kind == ResourceKind::Pin
                && claim.resource == IMU_DATA_READY_PIN
                && claim.owner == "IMU data ready"
        }));
        assert!(CLAIMS.iter().any(|claim| {
            claim.kind == ResourceKind::Irq
                && claim.resource == "EXTI4"
                && claim.owner == "IMU data ready"
        }));
    }

    #[test]
    fn usb_debug_claim_summary_matches_the_active_fs_route() {
        let mut claimed_pins = USB_CDC_CLAIMS
            .iter()
            .filter(|claim| claim.kind == ResourceKind::Pin)
            .map(|claim| claim.resource);
        assert_eq!(claimed_pins.next(), Some(USB_FS_PINS[0]));
        assert_eq!(claimed_pins.next(), Some(USB_FS_PINS[1]));
        assert_eq!(claimed_pins.next(), None);
        assert!(
            USB_CDC_CLAIMS
                .iter()
                .any(|claim| { claim.kind == ResourceKind::Irq && claim.resource == "OTG_FS" })
        );
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

    #[test]
    fn the_buzzer_and_baro_carry_their_board_facts() {
        assert!(PIN_MAP.iter().any(|assignment| {
            assignment.signal == "BEEPER_INVERTED" && assignment.pin == "PB9"
        }));
        assert_eq!(BARO_I2C_ADDRESS, 0x76);
        assert!(
            PIN_MAP.iter().any(|assignment| {
                assignment.signal == "I2C1_SCL_BARO" && assignment.pin == "PB6"
            })
        );
        assert!(
            PIN_MAP.iter().any(|assignment| {
                assignment.signal == "I2C1_SDA_BARO" && assignment.pin == "PB7"
            })
        );
    }
}
