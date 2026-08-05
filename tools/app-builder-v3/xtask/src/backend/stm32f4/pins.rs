//! STM32F4 package-pin validation and generated pin-name helpers.

use anyhow::{Result, bail};

use crate::hw_resources::{Mcu, PinId};

/// Converts a numeric GPIO port index into its STM32 port letter.
pub(super) fn port_letter(port: u8) -> Result<char> {
    let letter = b'A'
        .checked_add(port)
        .filter(u8::is_ascii_uppercase)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "STM32F4 backend GPIO port index {port} cannot be represented as port A through Z"
            )
        })?;
    Ok(char::from(letter))
}

/// Formats a numeric pin identifier using STM32 notation such as `PA5`.
pub(super) fn pin_name(pin: PinId) -> Result<String> {
    Ok(format!("P{}{}", port_letter(pin.port)?, pin.pin))
}

/// Verifies that a pin is valid and bonded on the selected LQFP64 package.
pub(super) fn validate_lqfp64(mcu: Mcu, pin: PinId) -> Result<()> {
    if pin.pin > 15 {
        bail!(
            "STM32F4 backend pin on port {} has pin number {}; GPIO pin numbers must be 0 through 15",
            pin.port,
            pin.pin
        );
    }

    let port = port_letter(pin.port)?;
    let (part, bonded) = match mcu {
        Mcu::Stm32F401 => ("STM32F401RE", is_bonded_f401re_lqfp64(pin)),
        Mcu::Stm32F405 => ("STM32F405RG", is_bonded_f405rg_lqfp64(pin)),
    };
    if !bonded {
        bail!(
            "{part} LQFP64 package does not expose physical pin `P{port}{}`",
            pin.pin
        );
    }
    Ok(())
}

/// Returns the STM32 EXTI vector associated with a GPIO pin number.
pub(super) fn exti_binding(pin: PinId) -> Result<&'static str> {
    match pin.pin {
        0 => Ok("EXTI0"),
        1 => Ok("EXTI1"),
        2 => Ok("EXTI2"),
        3 => Ok("EXTI3"),
        4 => Ok("EXTI4"),
        5..=9 => Ok("EXTI9_5"),
        10..=15 => Ok("EXTI15_10"),
        number => bail!(
            "STM32F4 backend cannot derive an EXTI binding for GPIO pin number {number}; pin numbers must be 0 through 15"
        ),
    }
}

fn is_bonded_f401re_lqfp64(pin: PinId) -> bool {
    // Audited against STMicroelectronics DS10086 Rev 5, Figure 12,
    // "STM32F401xD/xE LQFP64 pinout". This catalog only establishes that
    // a GPIO is bonded to the MCU package; board routing and electrical
    // conflicts remain facts owned by the board declaration author.
    // https://www.st.com/resource/en/datasheet/stm32f401re.pdf
    match pin.port {
        0 | 2 => pin.pin <= 15,
        1 => pin.pin <= 10 || (12..=15).contains(&pin.pin),
        3 => pin.pin == 2,
        7 => pin.pin <= 1,
        _ => false,
    }
}

fn is_bonded_f405rg_lqfp64(pin: PinId) -> bool {
    // Audited against STMicroelectronics DS8626 Rev 9, Figure 12,
    // "STM32F40x LQFP64 pinout". This establishes package bonding only;
    // board routing and electrical conflicts remain declaration-owned facts.
    // https://www.st.com/resource/en/datasheet/dm00037051.pdf
    match pin.port {
        0..=2 => pin.pin <= 15,
        3 => pin.pin == 2,
        7 => pin.pin <= 1,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_numeric_pin_ids_to_stm32_ports() {
        assert_eq!(port_letter(0).unwrap(), 'A');
        assert_eq!(port_letter(2).unwrap(), 'C');
        assert_eq!(pin_name(PinId::new(0, 5)).unwrap(), "PA5");
        assert_eq!(pin_name(PinId::new(2, 13)).unwrap(), "PC13");
    }

    #[test]
    fn rejects_out_of_range_numeric_pin_coordinates() {
        assert!(validate_lqfp64(Mcu::Stm32F401, PinId::new(0, 16)).is_err());
        assert!(validate_lqfp64(Mcu::Stm32F401, PinId::new(26, 0)).is_err());
    }

    #[test]
    fn validates_stm32f401re_lqfp64_package_pins() {
        for pin in [
            PinId::new(0, 0),
            PinId::new(0, 15),
            PinId::new(1, 4),
            PinId::new(1, 10),
            PinId::new(1, 12),
            PinId::new(2, 15),
            PinId::new(3, 2),
            PinId::new(7, 1),
        ] {
            validate_lqfp64(Mcu::Stm32F401, pin).unwrap();
        }
        for pin in [
            PinId::new(1, 11),
            PinId::new(3, 0),
            PinId::new(4, 0),
            PinId::new(7, 2),
        ] {
            assert!(validate_lqfp64(Mcu::Stm32F401, pin).is_err());
        }
    }

    #[test]
    fn validates_stm32f405rg_lqfp64_package_pins() {
        for pin in [
            PinId::new(0, 1),
            PinId::new(0, 3),
            PinId::new(1, 1),
            PinId::new(1, 11),
            PinId::new(2, 15),
            PinId::new(3, 2),
            PinId::new(7, 1),
        ] {
            validate_lqfp64(Mcu::Stm32F405, pin).unwrap();
        }
        for pin in [PinId::new(3, 1), PinId::new(4, 0), PinId::new(7, 2)] {
            assert!(validate_lqfp64(Mcu::Stm32F405, pin).is_err());
        }
    }

    #[test]
    fn derives_exti_vector_from_pin_number() {
        let binding = |number| exti_binding(PinId::new(0, number)).unwrap();
        assert_eq!(binding(0), "EXTI0");
        assert_eq!(binding(4), "EXTI4");
        assert_eq!(binding(5), "EXTI9_5");
        assert_eq!(binding(9), "EXTI9_5");
        assert_eq!(binding(10), "EXTI15_10");
        assert_eq!(binding(15), "EXTI15_10");
        assert!(exti_binding(PinId::new(0, 16)).is_err());
    }
}
