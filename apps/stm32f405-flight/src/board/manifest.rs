use ferrowasp_stm32f4::board_manifest::{
    BoardIdentity, PinAssignment, ResourceClaim, ResourceKind, TimerGroupDescription, TimerMode,
};

pub const BOARD_IDENTITY: BoardIdentity = BoardIdentity {
    target_id: "ferrowasp_fcu3",
    name: "FerroWasp FCU3",
    mcu: "STM32F405RGT6",
    package: "LQFP64",
};

pub const SWD_PINS: &[&str] = &["PA13", "PA14"];
pub const USB_FS_PINS: &[&str] = &["PA11", "PA12"];
pub const CONTROL_SCHEDULER_TIMER: &str = "TIM4";
pub const IO_TIMEBASE_TIMER: &str = "TIM2";
pub const IO_WATCHDOG_TIMER: &str = "TIM6";

pub const PIN_MAP: &[PinAssignment] = &[
    PinAssignment {
        signal: "USART1_RX_ESC_TELEMETRY",
        pin: "PA10",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "UART4_TX_MSP_OSD",
        pin: "PA0",
        alternate: Some(8),
    },
    PinAssignment {
        signal: "UART4_RX_MSP_OSD",
        pin: "PA1",
        alternate: Some(8),
    },
    PinAssignment {
        signal: "USART2_TX_SBUS",
        pin: "PA2",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "USART2_RX_SBUS",
        pin: "PA3",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "SPI1_MPU6500_CS",
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
        signal: "MOTOR4_DSHOT",
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
        signal: "DEBUG_LED_RED",
        pin: "PB0",
        alternate: None,
    },
    PinAssignment {
        signal: "DEBUG_LED_GREEN",
        pin: "PB1",
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
    "USART1",
    "DMA2_STREAM5",
    "USART2",
    "DMA1_STREAM5",
    "UART4",
    "DMA1_STREAM2",
    "DMA1_STREAM4",
    "DMA2_STREAM2",
    "DMA2_STREAM0",
    // Mandatory four-motor DShot completion interrupts.
    "DMA2_STREAM1",
    "DMA2_STREAM4",
    "DMA2_STREAM6",
    "DMA2_STREAM7",
    "TIM4",
    "TIM6_DAC",
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
    ResourceClaim::new(
        ResourceKind::Peripheral,
        "USART1",
        "BLHeli legacy ESC telemetry RX",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PA10", "USART1 RX ESC telemetry"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA2_STREAM5_CH4",
        "USART1 RX ESC telemetry",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART1", "USART1 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM5", "USART1 RX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "USART2", "SBUS RC input"),
    ResourceClaim::new(ResourceKind::Pin, "PA2", "USART2 TX SBUS"),
    ResourceClaim::new(ResourceKind::Pin, "PA3", "USART2 RX SBUS"),
    ResourceClaim::new(
        ResourceKind::DmaStream,
        "DMA1_STREAM5_CH4",
        "USART2 RX SBUS",
    ),
    ResourceClaim::new(ResourceKind::Irq, "USART2", "USART2 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM5", "USART2 RX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "UART4", "DJI O4 MSP OSD"),
    ResourceClaim::new(ResourceKind::Pin, "PA0", "UART4 TX MSP"),
    ResourceClaim::new(ResourceKind::Pin, "PA1", "UART4 RX MSP"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM2_CH4", "UART4 RX MSP"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA1_STREAM4_CH4", "UART4 TX MSP"),
    ResourceClaim::new(ResourceKind::Irq, "UART4", "UART4 RX IDLE"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM2", "UART4 RX DMA"),
    ResourceClaim::new(ResourceKind::Irq, "DMA1_STREAM4", "UART4 TX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "SPI1", "MPU6500 IMU"),
    ResourceClaim::new(ResourceKind::Pin, "PA4", "SPI1 MPU6500 CS"),
    ResourceClaim::new(ResourceKind::Pin, "PA5", "SPI1 SCK"),
    ResourceClaim::new(ResourceKind::Pin, "PA6", "SPI1 MISO"),
    ResourceClaim::new(ResourceKind::Pin, "PA7", "SPI1 MOSI"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM2_CH3", "SPI1 RX"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM3_CH3", "SPI1 TX"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM2", "SPI1 RX DMA"),
    ResourceClaim::new(ResourceKind::Peripheral, "ADC1", "Battery observation"),
    ResourceClaim::new(ResourceKind::Pin, "PC0", "ADC voltage"),
    ResourceClaim::new(ResourceKind::Pin, "PC1", "ADC current"),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM0", "ADC1 observation"),
    ResourceClaim::new(ResourceKind::Irq, "DMA2_STREAM0", "ADC1 DMA"),
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
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM4_CH7", "Motor 3 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STREAM4",
        "Motor 3 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::DmaStream, "DMA2_STREAM6_CH6", "Motor 4 DShot"),
    ResourceClaim::new(
        ResourceKind::Irq,
        "DMA2_STREAM6",
        "Motor 4 DShot DMA completion",
    ),
    ResourceClaim::new(ResourceKind::Pin, "PB0", "Red debug LED"),
    ResourceClaim::new(ResourceKind::Pin, "PB1", "Green debug LED"),
    ResourceClaim::new(ResourceKind::Peripheral, "TIM1", "Motor 1 and 4 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PA8", "Motor 1 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM1_CH1", "Motor 1 DShot"),
    ResourceClaim::new(
        ResourceKind::Pin,
        "PB15",
        "Motor 4 DShot complementary output",
    ),
    ResourceClaim::new(
        ResourceKind::TimerChannel,
        "TIM1_CH3N",
        "Motor 4 DShot complementary output",
    ),
    ResourceClaim::new(ResourceKind::Peripheral, "TIM8", "Motor 2 and 3 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PC9", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM8_CH4", "Motor 2 DShot"),
    ResourceClaim::new(ResourceKind::Pin, "PC8", "Motor 3 DShot"),
    ResourceClaim::new(ResourceKind::TimerChannel, "TIM8_CH3", "Motor 3 DShot"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_stm32f4::board_manifest::{
        ResourceKind, count_claims_by_kind, find_duplicate_claim,
    };

    #[test]
    fn board_identity_names_the_active_fcu3_target() {
        assert_eq!(BOARD_IDENTITY.target_id, "ferrowasp_fcu3");
        assert_eq!(BOARD_IDENTITY.name, "FerroWasp FCU3");
        assert_eq!(BOARD_IDENTITY.mcu, "STM32F405RGT6");
        assert_eq!(BOARD_IDENTITY.package, "LQFP64");
    }

    #[test]
    fn manifest_has_no_duplicate_exclusive_claims() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
    }

    #[test]
    fn frozen_pin_map_preserves_fcu3_dshot_outputs() {
        assert_eq!(PIN_MAP[9].pin, "PA8");
        assert_eq!(PIN_MAP[10].pin, "PC9");
        assert_eq!(PIN_MAP[11].pin, "PC8");
        assert_eq!(PIN_MAP[12].pin, "PB15");
    }

    #[test]
    fn manifest_claims_both_fcu3_debug_leds() {
        for (pin, owner) in [("PB0", "Red debug LED"), ("PB1", "Green debug LED")] {
            assert!(CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::Pin && claim.resource == pin && claim.owner == owner
            }));
        }
    }

    #[test]
    fn manifest_claim_counts_match_current_firmware_routes() {
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::DmaStream), 11);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::TimerChannel), 4);
        assert_eq!(count_claims_by_kind(CLAIMS, ResourceKind::Peripheral), 10);
    }

    #[test]
    fn dispatcher_irqs_match_the_rtic_app_reservations() {
        assert_eq!(
            DISPATCHER_IRQS,
            [
                "CAN1_TX",
                "CAN2_TX",
                "CAN1_RX0",
                "CAN1_RX1",
                "CAN1_SCE",
                "CAN2_RX0",
                "CAN2_RX1",
                "OTG_HS_EP1_OUT",
                "OTG_HS_EP1_IN",
            ]
        );

        for irq in DISPATCHER_IRQS {
            assert!(CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::Irq
                    && claim.resource == *irq
                    && claim.owner == "RTIC software dispatcher"
            }));
        }
    }

    #[test]
    fn hardware_irqs_do_not_overlap_software_dispatchers() {
        for hardware_irq in HARDWARE_IRQS {
            assert!(!DISPATCHER_IRQS.contains(hardware_irq));
        }
    }

    #[test]
    fn swd_and_optional_usb_pins_remain_unclaimed() {
        for reserved_pin in SWD_PINS.iter().chain(USB_FS_PINS) {
            assert!(
                !PIN_MAP
                    .iter()
                    .any(|assignment| assignment.pin == *reserved_pin)
            );
            assert!(!CLAIMS.iter().any(|claim| {
                claim.kind == ResourceKind::Pin && claim.resource == *reserved_pin
            }));
        }
    }

    #[test]
    fn each_timer_has_one_compatible_mode() {
        for (index, group) in TIMER_GROUPS.iter().enumerate() {
            assert!(
                !TIMER_GROUPS[index + 1..]
                    .iter()
                    .any(|other| other.timer == group.timer)
            );
        }

        assert_eq!(TIMER_GROUPS[0].mode, TimerMode::Dshot);
        assert_eq!(TIMER_GROUPS[1].mode, TimerMode::Dshot);
        assert_eq!(TIMER_GROUPS[1].channels, ["TIM8_CH3", "TIM8_CH4"]);
        assert_eq!(TIMER_GROUPS[2].mode, TimerMode::ControlScheduler);
        assert_eq!(TIMER_GROUPS[2].timer, CONTROL_SCHEDULER_TIMER);
        assert_eq!(TIMER_GROUPS[3].mode, TimerMode::MicrosecondTimebase);
        assert_eq!(TIMER_GROUPS[3].timer, IO_TIMEBASE_TIMER);
        assert_eq!(TIMER_GROUPS[4].mode, TimerMode::IoWatchdog);
        assert_eq!(TIMER_GROUPS[4].timer, IO_WATCHDOG_TIMER);
    }
}
