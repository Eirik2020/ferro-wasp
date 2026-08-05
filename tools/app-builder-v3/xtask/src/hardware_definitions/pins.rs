//! STM32F4 physical GPIO pin identifiers.
//!
//! These types describe package-level pin coordinates only. Pin mode,
//! alternate function, pull configuration, electrical drive, and peripheral
//! ownership are configured separately.

/// Identifies one STM32F4 GPIO port bank.
///
/// A port identifies the letter portion of a pin name. For example,
/// [`GpioPort::A`] with pin number `5` identifies physical pin `PA5`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum GpioPort {
    /// GPIO port A.
    A,
    /// GPIO port B.
    B,
    /// GPIO port C.
    C,
    /// GPIO port D.
    D,
    /// GPIO port E.
    E,
    /// GPIO port F.
    F,
    /// GPIO port G.
    G,
    /// GPIO port H.
    H,
    /// GPIO port I.
    I,
    /// GPIO port J.
    J,
    /// GPIO port K.
    K,
}

/// Identifies one physical STM32F4 GPIO pin.
///
/// `PinId` represents the board-level coordinate, such as `PA5`. It does not
/// imply that the pin exists on every MCU package, is bonded out on the board,
/// or supports a particular alternate function. The selected STM32 backend
/// validates those hardware-specific constraints.
///
/// # Examples
///
/// ```
/// use xtask::hardware_definitions::pins::{GpioPort, PinId};
///
/// let pa5 = PinId::new(GpioPort::A, 5);
/// assert_eq!(pa5.port, GpioPort::A);
/// assert_eq!(pa5.pin, 5);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PinId {
    /// GPIO port bank containing the pin.
    pub port: GpioPort,

    /// Pin number within [`Self::port`], in the inclusive range `0..=15`.
    pub pin: u8,
}

impl PinId {
    /// Creates an STM32F4 physical pin identifier.
    ///
    /// The constructor checks only the GPIO pin-number range. It does not
    /// validate package bonding, board routing, or alternate-function support.
    ///
    /// # Panics
    ///
    /// Panics if `pin` is greater than `15`.
    pub const fn new(port: GpioPort, pin: u8) -> Self {
        assert!(pin <= 15);

        Self { port, pin }
    }
}
