//! Hardware-neutral physical definitions used by builder authoring APIs.

/// Board declaration types and structural validation.
pub mod board_declaration;

/// Reusable STM32F4 hardware endpoints that own hardware task graphs.
pub mod hw_endpoint;

/// DMA route definitions.
pub mod dma_route;

/// Four-lane advanced-timer DShot hardware declarations.
pub mod dshot;

/// Reviewed physical DShot actuator component declaration.
pub mod dshot_actuator;

/// Host-checkable authoring surface for physical actuator tasks.
pub mod dshot_authoring;

/// GPIO electrical and initialization declarations.
pub mod gpio;

/// Mandatory bounded Foxeer service-suite declaration.
pub mod golden_services;

/// Host-checkable authoring surface for target-only golden services.
pub mod golden_service_authoring;

/// MCU and clock declarations.
pub mod mcu;

/// STM32F4 lowering from resolved applications to RTIC source fragments.
pub(crate) mod lower;

/// Physical GPIO pin definitions.
pub mod pins;

/// Periodic timer-backed control scheduler declarations.
pub mod periodic_control;

/// Serial peripheral definitions.
pub mod serial;

/// Mandatory Foxeer ADC, SPI NOR, and USB hardware declarations.
pub mod service_hardware;

/// SPI peripheral and route definitions.
pub mod spi;

/// Runtime aliases for the initial Foxeer SPI1 IMU backend.
pub mod spi_imu;

/// Timer peripheral identifiers.
pub mod timer;

/// Reusable tasks that depend directly on STM32F4 HAL capabilities.
pub mod tasks;

/// Types commonly used when declaring an STM32F4 board.
pub mod board_prelude {
    pub use super::{
        board_declaration::{
            BoardDeclaration, HardwareEndpointDeclaration, ImuInstallationDeclaration,
            ImuInstallationId, ImuOrientation, SerialHardwareDeclaration, SpiHardwareDeclaration,
        },
        dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
        dshot::{DshotBankHardwareDeclaration, DshotLaneHardwareDeclaration, DshotTimerChannel},
        gpio::{
            Drive, ExternalInterrupt, GpioHardwareDeclaration, GpioMode, InterruptEdge, Level, Pull,
        },
        mcu::{ClockDeclaration, ClockSource, Mcu, McuDeclaration},
        pins::{GpioPort, PinId},
        serial::{SerialPeripheral, SerialRoute},
        service_hardware::{
            AdcObservationHardwareDeclaration, AdcPeripheral, SpiMode, SpiNorHardwareDeclaration,
            UsbCdcHardwareDeclaration, UsbCdcIdentityDeclaration, UsbPeripheral,
        },
        spi::{SpiBus, SpiDmaRoutes, SpiPeripheral, SpiPins},
        timer::{TimerHardwareDeclaration, TimerPeripheral},
    };
}
