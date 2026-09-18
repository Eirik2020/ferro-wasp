use ferrowasp_stm32f4::board_manifest::{
    BoardIdentity, PinAssignment, ResourceClaim, ResourceKind, TimerGroupDescription, TimerMode,
};

pub const BOARD_IDENTITY: BoardIdentity = BoardIdentity {
    target_id: "foxeer_f405_v2",
    name: "Foxeer F405 V2",
    mcu: "STM32F405RGT6",
    package: "LQFP64",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardCapabilities {
    pub spi1_imu: bool,
    pub spi2_flash: bool,
    pub imu_data_ready_exti: bool,
    pub sbus_receiver: bool,
    pub uart4_msp_displayport: bool,
    pub battery_current_adc: bool,
    pub usb_cdc_debug: bool,
    pub dshot_motor_count: u8,
    pub onboard_debug_leds_usable_with_swd: bool,
    pub timer_dma_motor_output: bool,
}

pub const BOARD_CAPABILITIES: BoardCapabilities = BoardCapabilities {
    spi1_imu: true,
    spi2_flash: true,
    imu_data_ready_exti: true,
    sbus_receiver: true,
    uart4_msp_displayport: true,
    battery_current_adc: true,
    usb_cdc_debug: true,
    dshot_motor_count: 4,
    onboard_debug_leds_usable_with_swd: false,
    timer_dma_motor_output: true,
};

pub const HSE_FREQUENCY_HZ: u32 = 8_000_000;
pub const SYSTEM_CLOCK_HZ: u32 = 168_000_000;
pub const SWD_PINS: &[&str] = &["PA13", "PA14"];
pub const USB_FS_PINS: &[&str] = &["PA11", "PA12"];
pub const IMU_DATA_READY_PIN: &str = "PC4";
pub const CONTROL_SCHEDULER_TIMER: &str = "TIM4";
pub const IO_TIMEBASE_TIMER: &str = "TIM2";
pub const IO_WATCHDOG_TIMER: &str = "TIM6";

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Spi1ImuKind {
    Mpu6500 = 1,
    Icm42688P = 2,
}

impl Spi1ImuKind {
    pub const MPU6500_WHO_AM_I: u8 = 0x70;
    pub const ICM42688P_WHO_AM_I: u8 = 0x47;

    pub const fn from_who_am_i(who_am_i: u8) -> Option<Self> {
        match who_am_i {
            Self::MPU6500_WHO_AM_I => Some(Self::Mpu6500),
            Self::ICM42688P_WHO_AM_I => Some(Self::Icm42688P),
            _ => None,
        }
    }

    pub const fn from_discriminant(value: u8) -> Option<Self> {
        match value {
            value if value == Self::Mpu6500 as u8 => Some(Self::Mpu6500),
            value if value == Self::Icm42688P as u8 => Some(Self::Icm42688P),
            _ => None,
        }
    }

    pub const fn who_am_i(self) -> u8 {
        match self {
            Self::Mpu6500 => Self::MPU6500_WHO_AM_I,
            Self::Icm42688P => Self::ICM42688P_WHO_AM_I,
        }
    }

    pub const fn dma_burst_register(self) -> u8 {
        match self {
            Self::Mpu6500 => 0x3b,
            Self::Icm42688P => 0x1d,
        }
    }
}

pub const PIN_MAP: &[PinAssignment] = &[
    PinAssignment {
        signal: "UART4_TX_MSP_DISPLAYPORT",
        pin: "PA0",
        alternate: Some(8),
    },
    PinAssignment {
        signal: "UART4_RX_MSP_DISPLAYPORT",
        pin: "PA1",
        alternate: Some(8),
    },
    PinAssignment {
        signal: "USART2_TX_RECEIVER",
        pin: "PA2",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "USART2_RX_RECEIVER",
        pin: "PA3",
        alternate: Some(7),
    },
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
        signal: "MOTOR1_DSHOT",
        pin: "PA8",
        alternate: Some(1),
    },
    PinAssignment {
        signal: "USART1_RX_ESC_TELEMETRY",
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
        pin: "PC9",
        alternate: Some(3),
    },
    PinAssignment {
        signal: "MOTOR3_DSHOT",
        pin: "PC8",
        alternate: Some(3),
    },
    PinAssignment {
        signal: "MOTOR4_DSHOT_COMPLEMENTARY",
        pin: "PB15",
        alternate: Some(1),
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
        signal: "IMU_DATA_READY_EXTI4",
        pin: IMU_DATA_READY_PIN,
        alternate: None,
    },
    PinAssignment {
        signal: "SPI2_FLASH_CS",
        pin: "PB12",
        alternate: None,
    },
    PinAssignment {
        signal: "SPI2_FLASH_SCK",
        pin: "PB13",
        alternate: Some(5),
    },
    PinAssignment {
        signal: "SPI2_FLASH_MISO",
        pin: "PC2",
        alternate: Some(5),
    },
    PinAssignment {
        signal: "SPI2_FLASH_MOSI",
        pin: "PC3",
        alternate: Some(5),
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
];

pub const HARDWARE_IRQS: &[&str] = &[
    "USART2",
    "DMA1_STREAM5",
    "UART4",
    "DMA1_STREAM2",
    "DMA1_STREAM4",
    "DMA2_STREAM0",
    "DMA2_STREAM1",
    "DMA2_STREAM2",
    "DMA2_STREAM5",
    "DMA2_STREAM6",
    "DMA2_STREAM7",
    "USART1",
    "EXTI4",
    "DMA2_STREAM4",
    "TIM4",
    "TIM6_DAC",
    "OTG_FS",
];

pub const TIMER_GROUPS: &[TimerGroupDescription] = &[
    TimerGroupDescription {
        timer: "TIM1",
        mode: TimerMode::Dshot,
        channels: &["TIM1_CH1", "TIM1_CH3N"],
    },
    TimerGroupDescription {
        timer: "TIM8",
        mode: TimerMode::Dshot,
        channels: &["TIM8_CH3", "TIM8_CH4"],
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

pub const CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(ResourceKind::Peripheral, "USART2", "SBUS RC input"),
    ResourceClaim::new(ResourceKind::Pin, "PA2", "USART2 TX receiver"),
    ResourceClaim::new(ResourceKind::Pin, "PA3", "USART2 RX receiver"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM5_CH4",
        "USART2 RX SBUS",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART2", "USART2 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM5", "USART2 RX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "UART4", "DJI MSP DisplayPort"),
    ResourceClaim::new(ResourceKind::Pin, "PA0", "UART4 TX MSP"),
    ResourceClaim::new(ResourceKind::Pin, "PA1", "UART4 RX MSP"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM2_CH4", "UART4 RX MSP"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM4_CH4", "UART4 TX MSP"),
    ResourceClaim::new(ResourceKind::Irq, "UART4", "UART4 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM2", "UART4 RX DMA"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM4", "UART4 TX DMA"),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "SPI1",
        "IMU identity probe and MPU6500/ICM42688-P data path",
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
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "ADC1",
        "Battery and current observation",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PC0", "ADC voltage"),
    ResourceClaim::new(ResourceKind::Pin, "PC1", "ADC current"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM4_CH0",
        "ADC1 observation",
    ),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM4", "ADC1 DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "TIM1", "Motor 1 and 4 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PA8", "Motor 1 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM1_CH1", "Motor 1 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PB15", "Motor 4 complementary DShot"),
    ResourceClaim::new(
        ResourceKind::TimerChannel,
        "TIM1_CH3N",
        "Motor 4 complementary DShot",
    ),
    ResourceClaim::new(ResourceKind::Peripheral, "TIM8", "Motor 2 and 3 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PC9", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM8_CH4", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PC8", "Motor 3 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM8_CH3", "Motor 3 DShot"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM1_CH6", "Motor 1 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STREAM1",
        "Motor 1 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM7_CH7", "Motor 2 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STREAM7",
        "Motor 2 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM2_CH0", "Motor 3 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STREAM2",
        "Motor 3 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM6_CH6", "Motor 4 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STREAM6",
        "Motor 4 DShot DMA completion",
    ),
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "USART1",
        "BLHeli legacy ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA10", "USART1 RX ESC telemetry"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM5_CH4",
        "USART1 RX ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART1", "USART1 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM5", "USART1 RX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "SPI2", "Onboard SPI NOR storage"),
    ResourceClaim::new(ResourceKind::Pin, "PB12", "SPI2 flash chip select"),
    ResourceClaim::new(ResourceKind::Pin, "PB13", "SPI2 flash clock"),
    ResourceClaim::new(ResourceKind::Pin, "PC2", "SPI2 flash MISO"),
    ResourceClaim::new(ResourceKind::Pin, "PC3", "SPI2 flash MOSI"),
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
        "OTG_FS",
        "USB CDC diagnostics and configuration",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA11", "USB FS D-"),
    ResourceClaim::new(ResourceKind::Pin, "PA12", "USB FS D+"),
    ResourceClaim::new(ResourceKind::Irq, "OTG_FS", "USB CDC service"),
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
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "USART1",
        "BLHeli legacy ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA10", "USART1 RX ESC telemetry"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM5_CH4",
        "USART1 RX ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART1", "Optional USART1 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM5", "Optional USART1 RX DMA"),
];

pub const SPI_FLASH_CLAIMS: &[ResourceClaim] = &[
    ResourceClaim::new(ResourceKind::Peripheral, "SPI2", "Onboard SPI NOR storage"),
    ResourceClaim::new(ResourceKind::Pin, "PB12", "SPI2 flash chip select"),
    ResourceClaim::new(ResourceKind::Pin, "PB13", "SPI2 flash clock"),
    ResourceClaim::new(ResourceKind::Pin, "PC2", "SPI2 flash MISO"),
    ResourceClaim::new(ResourceKind::Pin, "PC3", "SPI2 flash MOSI"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_stm32f4::board_manifest::{
        ResourceKind, count_claims_by_kind, find_duplicate_claim,
    };

    #[test]
    fn identity_names_the_foxeer_f405_v2() {
        assert_eq!(BOARD_IDENTITY.target_id, "foxeer_f405_v2");
        assert_eq!(BOARD_IDENTITY.name, "Foxeer F405 V2");
        assert_eq!(BOARD_IDENTITY.mcu, "STM32F405RGT6");
        assert_eq!(HSE_FREQUENCY_HZ, 8_000_000);
        assert_eq!(SYSTEM_CLOCK_HZ, 168_000_000);
    }

    #[test]
    fn supported_imu_identities_select_their_distinct_dma_bursts() {
        let mpu = Spi1ImuKind::from_who_am_i(0x70).unwrap();
        let icm = Spi1ImuKind::from_who_am_i(0x47).unwrap();

        assert_eq!(mpu, Spi1ImuKind::Mpu6500);
        assert_eq!(mpu.dma_burst_register(), 0x3b);
        assert_eq!(icm, Spi1ImuKind::Icm42688P);
        assert_eq!(icm.dma_burst_register(), 0x1d);
        assert_eq!(Spi1ImuKind::from_who_am_i(0x00), None);
        assert_eq!(Spi1ImuKind::from_discriminant(0), None);
    }

    #[test]
    fn active_capabilities_include_standard_foxeer_services() {
        let enabled = [
            BOARD_CAPABILITIES.spi1_imu,
            BOARD_CAPABILITIES.spi2_flash,
            BOARD_CAPABILITIES.imu_data_ready_exti,
            BOARD_CAPABILITIES.sbus_receiver,
            BOARD_CAPABILITIES.uart4_msp_displayport,
            BOARD_CAPABILITIES.battery_current_adc,
            BOARD_CAPABILITIES.usb_cdc_debug,
            BOARD_CAPABILITIES.timer_dma_motor_output,
        ];
        let disabled = [BOARD_CAPABILITIES.onboard_debug_leds_usable_with_swd];

        assert_eq!(enabled, [true; 8]);
        assert_eq!(disabled, [false; 1]);
        assert_eq!(BOARD_CAPABILITIES.dshot_motor_count, 4);
    }

    #[test]
    fn manifest_has_no_duplicate_claims_and_includes_standard_services() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
        assert_eq!(find_duplicate_claim(USB_CDC_CLAIMS), None);
        assert_eq!(find_duplicate_claim(ESC_TELEMETRY_CLAIMS), None);
        assert_eq!(find_duplicate_claim(SPI_FLASH_CLAIMS), None);
        for required in USB_CDC_CLAIMS {
            assert!(CLAIMS.iter().any(|active| {
                active.kind == required.kind && active.resource == required.resource
            }));
        }
        for required in ESC_TELEMETRY_CLAIMS.iter().chain(SPI_FLASH_CLAIMS) {
            assert!(CLAIMS.iter().any(|active| {
                active.kind == required.kind && active.resource == required.resource
            }));
        }
    }

    #[test]
    fn motor_map_preserves_the_primary_foxeer_dshot_outputs() {
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
            motor("MOTOR4_DSHOT_COMPLEMENTARY"),
        ];
        assert_eq!(
            [
                (motors[0].pin, motors[0].alternate),
                (motors[1].pin, motors[1].alternate),
                (motors[2].pin, motors[2].alternate),
                (motors[3].pin, motors[3].alternate),
            ],
            [
                ("PA8", Some(1)),
                ("PC9", Some(3)),
                ("PC8", Some(3)),
                ("PB15", Some(1)),
            ]
        );
    }

    #[test]
    fn m4_is_explicitly_complementary() {
        assert!(CLAIMS.iter().any(|claim| {
            claim.kind == ResourceKind::TimerChannel
                && claim.resource == "TIM1_CH3N"
                && claim.owner == "Motor 4 complementary DShot"
        }));
        assert!(!CLAIMS.iter().any(|claim| {
            claim.kind == ResourceKind::TimerChannel && claim.resource == "TIM1_CH3"
        }));
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
    fn active_claim_counts_match_the_standard_runtime_contract() {
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::DmaStream), 11);
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
