use crate::manifest::{ResourceClaim, ResourceKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardIdentity {
    pub target_id: &'static str,
    pub name: &'static str,
    pub mcu: &'static str,
    pub package: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardCapabilities {
    pub onboard_led: bool,
    pub stlink_vcp: bool,
    pub attached_imu: bool,
    pub actuator_outputs: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinAssignment {
    pub signal: &'static str,
    pub pin: &'static str,
    pub alternate: Option<u8>,
}

pub const BOARD_IDENTITY: BoardIdentity = BoardIdentity {
    target_id: "nucleo_f401re",
    name: "ST NUCLEO-F401RE",
    mcu: "STM32F401RET6",
    package: "LQFP64",
};

pub const BOARD_CAPABILITIES: BoardCapabilities = BoardCapabilities {
    onboard_led: true,
    stlink_vcp: true,
    attached_imu: false,
    actuator_outputs: false,
};

pub const SYSTEM_CLOCK_HZ: u32 = 84_000_000;
pub const HEARTBEAT_BAUD: u32 = 115_200;

pub const PIN_MAP: &[PinAssignment] = &[
    PinAssignment {
        signal: "STLINK_VCP_USART2_TX",
        pin: "PA2",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "STLINK_VCP_USART2_RX",
        pin: "PA3",
        alternate: Some(7),
    },
    PinAssignment {
        signal: "USER_LED_LD2",
        pin: "PA5",
        alternate: None,
    },
];

pub const CLAIMS: &[ResourceClaim] = &[
    claim(ResourceKind::Peripheral, "USART2", "ST-LINK VCP heartbeat"),
    claim(ResourceKind::Pin, "PA2", "USART2 TX to ST-LINK VCP"),
    claim(ResourceKind::Pin, "PA5", "LD2 user LED"),
    claim(ResourceKind::Peripheral, "SYST", "RTIC SysTick monotonic"),
    claim(ResourceKind::Irq, "EXTI0", "RTIC software dispatcher"),
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
    use crate::manifest::{ResourceKind, find_duplicate_claim};

    #[test]
    fn identity_names_the_nucleo_f401re() {
        assert_eq!(BOARD_IDENTITY.target_id, "nucleo_f401re");
        assert_eq!(BOARD_IDENTITY.mcu, "STM32F401RET6");
        assert_eq!(SYSTEM_CLOCK_HZ, 84_000_000);
    }

    #[test]
    fn bringup_target_has_no_actuator_or_attached_imu_capability() {
        assert_bringup_capabilities(BOARD_CAPABILITIES);
        assert!(!CLAIMS.iter().any(|claim| {
            claim.kind == ResourceKind::TimerChannel || claim.owner.contains("motor")
        }));
    }

    #[test]
    fn manifest_has_no_duplicate_exclusive_claims() {
        assert_eq!(find_duplicate_claim(CLAIMS), None);
    }

    #[test]
    fn exti0_is_reserved_for_the_rtic_software_dispatcher() {
        assert!(CLAIMS.iter().any(|claim| {
            claim.kind == ResourceKind::Irq
                && claim.resource == "EXTI0"
                && claim.owner == "RTIC software dispatcher"
        }));
    }

    #[test]
    fn vcp_and_led_pin_map_matches_the_nucleo_manual() {
        assert_eq!(PIN_MAP[0].pin, "PA2");
        assert_eq!(PIN_MAP[0].alternate, Some(7));
        assert_eq!(PIN_MAP[1].pin, "PA3");
        assert_eq!(PIN_MAP[2].pin, "PA5");
    }

    fn assert_bringup_capabilities(capabilities: BoardCapabilities) {
        assert!(capabilities.onboard_led);
        assert!(capabilities.stlink_vcp);
        assert!(!capabilities.attached_imu);
        assert!(!capabilities.actuator_outputs);
    }
}
