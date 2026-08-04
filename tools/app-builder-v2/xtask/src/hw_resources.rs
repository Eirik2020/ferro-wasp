//! HAL-independent hardware target, GPIO, and UART DMA resource declarations.
//!
//! These types describe physical hardware facts and requested electrical
//! configuration without exposing vendor HAL types. MCU backends validate the
//! declarations and translate them into target-specific initialization code.

#![deny(missing_docs)]

// ############# PIN IDENTIFICATION #############

/// Identifies a physical GPIO pin on the selected microcontroller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PinId {
    /// GPIO controller or port number.
    ///
    /// A backend may map `0` to GPIOA, `1` to GPIOB, and so on.
    pub port: u8,

    /// Pin number within the GPIO port.
    ///
    /// For example, PA5 would use pin number `5`.
    pub pin: u8,
}

impl PinId {
    /// Creates a physical pin identifier from a numeric port and pin number.
    ///
    /// Port numbering is backend-defined. For STM32 GPIO, port `0` maps to
    /// GPIOA, port `1` maps to GPIOB, and so on.
    pub const fn new(port: u8, pin: u8) -> Self {
        Self { port, pin }
    }
}

// ############# GPIO CONFIGURATION #############

/// Configures the internal pull resistor for a GPIO pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pull {
    /// No internal pull resistor.
    #[default]
    None,

    /// Enable the internal pull-up resistor.
    Up,

    /// Enable the internal pull-down resistor.
    Down,
}

/// Configures the electrical output-driver mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Drive {
    /// Actively drives both high and low output levels.
    #[default]
    PushPull,
    // TODO: Add OpenDrain after a backend implements and tests it.
}

/// Represents a digital GPIO logic level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// Logic-low electrical level.
    #[default]
    Low,

    /// Logic-high electrical level.
    High,
}

/// Selects the electrical edge that triggers a GPIO interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptEdge {
    /// Trigger when the pin transitions from low to high.
    Rising,

    /// Trigger when the pin transitions from high to low.
    Falling,

    /// Trigger on either a rising or falling transition.
    Both,
}

/// Describes external-interrupt configuration for a GPIO input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalInterrupt {
    /// Electrical edge that triggers the interrupt.
    pub edge: InterruptEdge,
}

/// Describes whether a GPIO resource is configured as an input or output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpioMode {
    /// Configure the pin as a digital input.
    Input {
        /// Optional external-interrupt configuration.
        ///
        /// When `None`, the pin is configured as a normal polled input.
        interrupt: Option<ExternalInterrupt>,
    },

    /// Configure the pin as a digital output.
    Output {
        /// Electrical output-driver mode.
        drive: Drive,

        /// Output level applied during initialization.
        initial_level: Level,
    },
}

// ############# GPIO RESOURCE #############

/// Declarative description of a GPIO resource used by the application builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gpio {
    /// Application-level resource identifier.
    ///
    /// Examples include `"status_led"` and `"user_button"`.
    pub id: &'static str,

    /// Physical pin assigned to this resource.
    pub pin: PinId,

    /// Internal pull-resistor configuration.
    pub pull: Pull,

    /// Input or output configuration for the pin.
    pub mode: GpioMode,
}

impl Gpio {
    /// Creates a digital input resource with no pull resistor or interrupt.
    pub const fn input(id: &'static str, pin: PinId) -> Self {
        Self {
            id,
            pin,
            pull: Pull::None,
            mode: GpioMode::Input { interrupt: None },
        }
    }

    /// Creates a push-pull digital output resource.
    ///
    /// The supplied initial level is applied when the generated application
    /// initializes the pin.
    pub const fn output(id: &'static str, pin: PinId, initial_level: Level) -> Self {
        Self {
            id,
            pin,
            pull: Pull::None,
            mode: GpioMode::Output {
                drive: Drive::PushPull,
                initial_level,
            },
        }
    }

    /// Creates an active-high push-pull output initialized electrically low.
    pub const fn output_low(id: &'static str, pin: PinId) -> Self {
        Self::output(id, pin, Level::Low)
    }

    /// Enables the internal pull-up resistor.
    pub const fn pull_up(mut self) -> Self {
        self.pull = Pull::Up;
        self
    }

    /// Enables the internal pull-down resistor.
    pub const fn pull_down(mut self) -> Self {
        self.pull = Pull::Down;
        self
    }

    /// Enables an external interrupt on an input resource.
    ///
    /// # Panics
    ///
    /// Panics when called on an output resource. Board resources are expected
    /// to be constants, so this becomes a compile-time declaration error.
    pub const fn interrupt_on(mut self, edge: InterruptEdge) -> Self {
        self.mode = match self.mode {
            GpioMode::Input { .. } => GpioMode::Input {
                interrupt: Some(ExternalInterrupt { edge }),
            },
            GpioMode::Output { .. } => {
                panic!("GPIO interrupts can only be configured on input resources")
            }
        };

        self
    }

    /// Wraps this GPIO declaration as an application hardware resource.
    pub const fn into_resource(self) -> HardwareResource {
        HardwareResource::Gpio(self)
    }
}

// ############# UART RX DMA RESOURCE #############

/// Identifies a numbered UART or USART peripheral without naming a vendor HAL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UartId {
    /// One-based peripheral number, such as `2` for USART2.
    pub number: u8,
}

impl UartId {
    /// Creates a numbered UART identifier.
    pub const fn new(number: u8) -> Self {
        Self { number }
    }
}

/// Identifies a DMA controller, stream, and channel using zero-based controller numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaChannel {
    /// Zero-based DMA controller number; STM32 maps `0` to DMA1.
    pub controller: u8,

    /// DMA stream number within the controller.
    pub stream: u8,

    /// Peripheral-request channel selected by the stream.
    pub channel: u8,
}

impl DmaChannel {
    /// Creates a physical DMA route.
    pub const fn new(controller: u8, stream: u8, channel: u8) -> Self {
        Self {
            controller,
            stream,
            channel,
        }
    }
}

/// Selects the serial framing and parser protocol required by a UART receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialProtocol {
    /// Futaba SBUS framing at 100 kbaud, even parity, and two stop bits.
    Sbus,
}

/// Declares one receive-only UART connected to a DMA stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UartRxDma {
    /// Application-level resource identifier.
    pub id: &'static str,

    /// Numbered UART or USART peripheral.
    pub uart: UartId,

    /// Physical receive pin.
    pub rx_pin: PinId,

    /// Physical DMA route used for peripheral-to-memory transfers.
    pub dma: DmaChannel,

    /// Serial framing and protocol selected for this receiver.
    pub protocol: SerialProtocol,
}

impl UartRxDma {
    /// Declares a DMA-backed SBUS receiver.
    pub const fn sbus(id: &'static str, uart: UartId, rx_pin: PinId, dma: DmaChannel) -> Self {
        Self {
            id,
            uart,
            rx_pin,
            dma,
            protocol: SerialProtocol::Sbus,
        }
    }

    /// Wraps this UART declaration as an application hardware resource.
    pub const fn into_resource(self) -> HardwareResource {
        HardwareResource::UartRxDma(self)
    }
}

// ############# HARDWARE RESOURCE #############

/// Hardware resource that can be owned by an RTIC application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareResource {
    /// Digital GPIO input or output resource.
    Gpio(Gpio),

    /// Receive-only UART serviced by DMA and peripheral-IDLE interrupts.
    UartRxDma(UartRxDma),
}

impl HardwareResource {
    /// Returns the application-level resource identifier.
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Gpio(gpio) => gpio.id,
            Self::UartRxDma(uart) => uart.id,
        }
    }

    /// Returns the physical GPIO pin assigned to the resource.
    pub const fn pin(&self) -> PinId {
        match self {
            Self::Gpio(gpio) => gpio.pin,
            Self::UartRxDma(uart) => uart.rx_pin,
        }
    }

    /// Returns the GPIO declaration contained by this resource.
    pub const fn gpio(&self) -> Option<&Gpio> {
        match self {
            Self::Gpio(gpio) => Some(gpio),
            Self::UartRxDma(_) => None,
        }
    }

    /// Returns the UART RX DMA declaration when this is a serial resource.
    pub const fn uart_rx_dma(&self) -> Option<&UartRxDma> {
        match self {
            Self::Gpio(_) => None,
            Self::UartRxDma(uart) => Some(uart),
        }
    }
}

/// Identifies a supported microcontroller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mcu {
    /// STM32F401 PAC/backend selection.
    Stm32F401,
}

/// Selects the MCU's primary clock source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSource {
    /// Use the MCU's internal high-speed oscillator.
    InternalHighSpeed,

    /// Use an external crystal or resonator.
    ExternalCrystal {
        /// External crystal or resonator frequency in hertz.
        frequency_hz: u32,
    },

    /// Use an externally generated clock signal.
    ExternalClock {
        /// External clock-signal frequency in hertz.
        frequency_hz: u32,
    },
}

/// Clock configuration requested by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    /// Primary oscillator source.
    pub source: ClockSource,

    /// Requested system/core clock frequency.
    pub sysclk_hz: u32,
}

impl Clock {
    /// Creates a system-clock request using the internal high-speed oscillator.
    pub const fn internal_high_speed(sysclk_hz: u32) -> Self {
        Self {
            source: ClockSource::InternalHighSpeed,
            sysclk_hz,
        }
    }
}

/// Describes the MCU target used to generate the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// Microcontroller selection used to choose the backend and PAC.
    pub mcu: Mcu,

    /// MCU clock configuration.
    pub clock: Clock,
}

impl Target {
    /// Creates a target using its internal high-speed oscillator.
    pub const fn internal_high_speed(mcu: Mcu, sysclk_hz: u32) -> Self {
        Self {
            mcu,
            clock: Clock::internal_high_speed(sysclk_hz),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "GPIO interrupts can only be configured on input resources")]
    fn output_interrupt_configuration_fails_explicitly() {
        let _ =
            Gpio::output("led", PinId::new(0, 5), Level::Low).interrupt_on(InterruptEdge::Rising);
    }

    #[test]
    fn common_constructors_preserve_explicit_defaults() {
        let output = Gpio::output_low("led", PinId::new(0, 5));
        assert_eq!(
            output.mode,
            GpioMode::Output {
                drive: Drive::PushPull,
                initial_level: Level::Low,
            }
        );

        let target = Target::internal_high_speed(Mcu::Stm32F401, 84_000_000);
        assert_eq!(target.clock.source, ClockSource::InternalHighSpeed);
        assert_eq!(target.clock.sysclk_hz, 84_000_000);
    }
}
