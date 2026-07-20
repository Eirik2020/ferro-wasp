use crate::manifest::{ResourceClaim, ResourceKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardIdentity {
    pub target_id: &'static str,
    pub name: &'static str,
    pub mcu: &'static str,
    pub package: &'static str,
}

pub const BOARD_IDENTITY: BoardIdentity = BoardIdentity {
    target_id: "foxeer_f405_v2",
    name: "Foxeer F405 V2",
    mcu: "STM32F405RGT6",
    package: "LQFP64",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardCapabilities {
    pub spi1_imu: bool,
    pub sbus_receiver: bool,
    pub uart4_msp_displayport: bool,
    pub battery_current_adc: bool,
    pub usb_cdc_debug: bool,
    pub static_pwm_motor_count: u8,
    pub onboard_debug_leds_usable_with_swd: bool,
    pub timer_dma_motor_output: bool,
}

pub const BOARD_CAPABILITIES: BoardCapabilities = BoardCapabilities {
    spi1_imu: true,
    sbus_receiver: true,
    uart4_msp_displayport: true,
    battery_current_adc: true,
    usb_cdc_debug: true,
    static_pwm_motor_count: 4,
    onboard_debug_leds_usable_with_swd: false,
    timer_dma_motor_output: false,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinAssignment {
    pub signal: &'static str,
    pub pin: &'static str,
    pub alternate: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerMode {
    ControlScheduler,
    IoWatchdog,
    MicrosecondTimebase,
    StaticPwm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerGroupDescription {
    pub timer: &'static str,
    pub mode: TimerMode,
    pub channels: &'static [&'static str],
}

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
        signal: "MOTOR1_PWM",
        pin: "PA8",
        alternate: Some(1),
    },
    PinAssignment {
        signal: "MOTOR2_PWM",
        pin: "PC9",
        alternate: Some(3),
    },
    PinAssignment {
        signal: "MOTOR3_PWM",
        pin: "PC8",
        alternate: Some(3),
    },
    PinAssignment {
        signal: "MOTOR4_PWM_COMPLEMENTARY",
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
        signal: "IMU_DATA_READY_DEFERRED",
        pin: IMU_DATA_READY_PIN,
        alternate: None,
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
    "DMA2_STREAM4",
    "TIM4",
    "TIM6_DAC",
    "OTG_FS",
];

pub const TIMER_GROUPS: &[TimerGroupDescription] = &[
    TimerGroupDescription {
        timer: "TIM1",
        mode: TimerMode::StaticPwm,
        channels: &["TIM1_CH1", "TIM1_CH3N"],
    },
    TimerGroupDescription {
        timer: "TIM8",
        mode: TimerMode::StaticPwm,
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
    claim(ResourceKind::Peripheral, "USART2", "SBUS RC input"),
    claim(ResourceKind::Pin, "PA2", "USART2 TX receiver"),
    claim(ResourceKind::Pin, "PA3", "USART2 RX receiver"),
    claim(
        ResourceKind::DmaStream,
        "DMA1_STREAM5_CH4",
        "USART2 RX SBUS",
    ),
    claim(ResourceKind::Irq, "USART2", "USART2 RX IDLE"),
    claim(ResourceKind::Irq, "DMA1_STREAM5", "USART2 RX DMA"),
    claim(ResourceKind::Peripheral, "UART4", "DJI MSP DisplayPort"),
    claim(ResourceKind::Pin, "PA0", "UART4 TX MSP"),
    claim(ResourceKind::Pin, "PA1", "UART4 RX MSP"),
    claim(ResourceKind::DmaStream, "DMA1_STREAM2_CH4", "UART4 RX MSP"),
    claim(ResourceKind::DmaStream, "DMA1_STREAM4_CH4", "UART4 TX MSP"),
    claim(ResourceKind::Irq, "UART4", "UART4 RX IDLE"),
    claim(ResourceKind::Irq, "DMA1_STREAM2", "UART4 RX DMA"),
    claim(ResourceKind::Irq, "DMA1_STREAM4", "UART4 TX DMA"),
    claim(
        ResourceKind::Peripheral,
        "SPI1",
        "IMU identity probe and MPU6500/ICM42688-P data path",
    ),
    claim(ResourceKind::Pin, "PA4", "SPI1 IMU CS"),
    claim(ResourceKind::Pin, "PA5", "SPI1 SCK"),
    claim(ResourceKind::Pin, "PA6", "SPI1 MISO"),
    claim(ResourceKind::Pin, "PA7", "SPI1 MOSI"),
    claim(ResourceKind::DmaStream, "DMA2_STREAM0_CH3", "SPI1 RX"),
    claim(ResourceKind::DmaStream, "DMA2_STREAM3_CH3", "SPI1 TX"),
    claim(ResourceKind::Irq, "DMA2_STREAM0", "SPI1 RX DMA"),
    claim(
        ResourceKind::Peripheral,
        "ADC1",
        "Battery and current observation",
    ),
    claim(ResourceKind::Pin, "PC0", "ADC voltage"),
    claim(ResourceKind::Pin, "PC1", "ADC current"),
    claim(
        ResourceKind::DmaStream,
        "DMA2_STREAM4_CH0",
        "ADC1 observation",
    ),
    claim(ResourceKind::Irq, "DMA2_STREAM4", "ADC1 DMA"),
    claim(ResourceKind::Peripheral, "TIM1", "Motor 1 and 4 RC PWM"),
    claim(ResourceKind::Pin, "PA8", "Motor 1 PWM"),
    claim(ResourceKind::TimerChannel, "TIM1_CH1", "Motor 1 PWM"),
    claim(ResourceKind::Pin, "PB15", "Motor 4 complementary PWM"),
    claim(
        ResourceKind::TimerChannel,
        "TIM1_CH3N",
        "Motor 4 complementary PWM",
    ),
    claim(ResourceKind::Peripheral, "TIM8", "Motor 2 and 3 RC PWM"),
    claim(ResourceKind::Pin, "PC9", "Motor 2 PWM"),
    claim(ResourceKind::TimerChannel, "TIM8_CH4", "Motor 2 PWM"),
    claim(ResourceKind::Pin, "PC8", "Motor 3 PWM"),
    claim(ResourceKind::TimerChannel, "TIM8_CH3", "Motor 3 PWM"),
    claim(
        ResourceKind::Peripheral,
        CONTROL_SCHEDULER_TIMER,
        "Control loop scheduler",
    ),
    claim(
        ResourceKind::Irq,
        CONTROL_SCHEDULER_TIMER,
        "Control loop scheduler",
    ),
    claim(
        ResourceKind::Peripheral,
        IO_TIMEBASE_TIMER,
        "I/O microsecond timebase",
    ),
    claim(
        ResourceKind::Peripheral,
        IO_WATCHDOG_TIMER,
        "I/O deadline watchdog",
    ),
    claim(ResourceKind::Irq, "TIM6_DAC", "I/O deadline watchdog"),
    claim(ResourceKind::Irq, "CAN1_TX", "RTIC software dispatcher"),
    claim(ResourceKind::Irq, "CAN2_TX", "RTIC software dispatcher"),
    claim(ResourceKind::Irq, "CAN1_RX0", "RTIC software dispatcher"),
    claim(ResourceKind::Irq, "CAN1_RX1", "RTIC software dispatcher"),
    claim(ResourceKind::Irq, "CAN1_SCE", "RTIC software dispatcher"),
    claim(ResourceKind::Irq, "CAN2_RX0", "RTIC software dispatcher"),
    claim(ResourceKind::Irq, "CAN2_RX1", "RTIC software dispatcher"),
    claim(
        ResourceKind::Irq,
        "OTG_HS_EP1_OUT",
        "RTIC software dispatcher",
    ),
    claim(
        ResourceKind::Irq,
        "OTG_HS_EP1_IN",
        "RTIC software dispatcher",
    ),
];

pub const OPTIONAL_USB_CDC_CLAIMS: &[ResourceClaim] = &[
    claim(
        ResourceKind::Peripheral,
        "OTG_FS",
        "Optional read-only USB CDC diagnostics",
    ),
    claim(ResourceKind::Pin, "PA11", "Optional USB FS D- diagnostics"),
    claim(ResourceKind::Pin, "PA12", "Optional USB FS D+ diagnostics"),
    claim(ResourceKind::Irq, "OTG_FS", "Optional USB CDC diagnostics"),
];

const fn claim(kind: ResourceKind, resource: &'static str, owner: &'static str) -> ResourceClaim {
    ResourceClaim {
        kind,
        resource,
        owner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ResourceKind, count_claims_by_kind, find_duplicate_claim};

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
    fn active_capabilities_are_limited_to_the_ferrowasp_subset() {
        let enabled = [
            BOARD_CAPABILITIES.spi1_imu,
            BOARD_CAPABILITIES.sbus_receiver,
            BOARD_CAPABILITIES.uart4_msp_displayport,
            BOARD_CAPABILITIES.battery_current_adc,
            BOARD_CAPABILITIES.usb_cdc_debug,
        ];
        let disabled = [
            BOARD_CAPABILITIES.onboard_debug_leds_usable_with_swd,
            BOARD_CAPABILITIES.timer_dma_motor_output,
        ];

        assert_eq!(enabled, [true; 5]);
        assert_eq!(disabled, [false; 2]);
        assert_eq!(BOARD_CAPABILITIES.static_pwm_motor_count, 4);
    }

    #[test]
    fn manifest_has_no_duplicate_exclusive_claims() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
        assert_eq!(find_duplicate_claim(OPTIONAL_USB_CDC_CLAIMS), None);
        for optional in OPTIONAL_USB_CDC_CLAIMS {
            assert!(!CLAIMS.iter().any(|active| {
                active.kind == optional.kind && active.resource == optional.resource
            }));
        }
    }

    #[test]
    fn motor_map_preserves_the_primary_foxeer_quad_outputs() {
        let motors = &PIN_MAP[8..12];
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
                && claim.owner == "Motor 4 complementary PWM"
        }));
        assert!(!CLAIMS.iter().any(|claim| {
            claim.kind == ResourceKind::TimerChannel && claim.resource == "TIM1_CH3"
        }));
    }

    #[test]
    fn swd_usb_and_deferred_imu_irq_pin_remain_unclaimed() {
        for reserved_pin in SWD_PINS
            .iter()
            .chain(USB_FS_PINS)
            .chain(core::iter::once(&IMU_DATA_READY_PIN))
        {
            assert!(!CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::Pin && claim.resource == *reserved_pin
            }));
        }
    }

    #[test]
    fn optional_usb_debug_claims_the_reserved_fs_route_only() {
        let mut claimed_pins = OPTIONAL_USB_CDC_CLAIMS
            .iter()
            .filter(|claim| claim.kind == ResourceKind::Pin)
            .map(|claim| claim.resource);
        assert_eq!(claimed_pins.next(), Some(USB_FS_PINS[0]));
        assert_eq!(claimed_pins.next(), Some(USB_FS_PINS[1]));
        assert_eq!(claimed_pins.next(), None);
        assert!(
            OPTIONAL_USB_CDC_CLAIMS
                .iter()
                .any(|claim| { claim.kind == ResourceKind::Irq && claim.resource == "OTG_FS" })
        );
    }

    #[test]
    fn active_claim_counts_match_the_initial_runtime_contract() {
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::DmaStream), 6);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::TimerChannel), 4);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::Peripheral), 9);
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
