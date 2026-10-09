//! Board bring-up: the Lucid's pins and streams handed to the STM32H7
//! backend.

use super::aliases::{SdFlash, Spi1ImuOwner};
use super::manifest::Spi1ImuKind;
use embedded_hal::delay::DelayNs;
use ferrowasp_drivers::{icm42688p, mpu6000, mpu6500};
use ferrowasp_stm32::app_storage::{AdcStorageResources, SpiDmaStorageResources};
use ferrowasp_stm32h7 as backend;
use ferrowasp_stm32h7::eh1::CycleDelay;
use ferrowasp_stm32h7::hal_prelude::*;

const _: () = {
    assert!(Spi1ImuKind::MPU6500_WHO_AM_I == mpu6500::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::ICM42688P_WHO_AM_I == icm42688p::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::MPU6000_WHO_AM_I == mpu6000::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::Mpu6500.dma_burst_register() == mpu6500::Register::AccelXoutH as u8);
    assert!(Spi1ImuKind::Icm42688P.dma_burst_register() == icm42688p::Register::TempData1 as u8);
    assert!(Spi1ImuKind::Mpu6000.dma_burst_register() == mpu6000::Register::AccelXoutH as u8);
};

pub use backend::adc::{Adc1BatteryResources, Adc1ObservationParts};
pub use backend::dshot::{
    DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS, DshotCommandError, DshotDmaBuffer,
    DshotDmaStorage, DshotInitError, DshotInterruptEvent, DshotMotor, DshotMotorBank,
    DshotMotorBankResources, DshotServiceEvent, DshotTelemetryRequestError, init_dshot_motor_bank,
};
use backend::sd_storage::CardSlot;
pub use backend::sd_storage::SdCardResources;

pub struct Spi1ImuResources {
    pub cs_pin: PC15,
    pub sck_pin: PA5,
    pub miso_pin: PA6,
    pub mosi_pin: PD7,
    pub spi: SPI1,
    pub prec: rec::Spi1,
    pub rx_dma: Stream4<DMA1>,
    pub tx_dma: Stream5<DMA1>,
}

pub struct Spi1ImuParts {
    pub owner: Spi1ImuOwner,
    pub parser: backend::spi_dma::SpiRxParserSide,
    pub bringup: Spi1ImuBringupStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Spi1ImuBringupStatus {
    Ready { kind: Spi1ImuKind, who_am_i: u8 },
    UnsupportedIdentity { who_am_i: u8 },
    ProbeFailed,
    ConfigurationFailed { kind: Spi1ImuKind, who_am_i: u8 },
}

impl Spi1ImuBringupStatus {
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub const fn kind(self) -> Option<Spi1ImuKind> {
        match self {
            Self::Ready { kind, .. } => Some(kind),
            _ => None,
        }
    }
}

/// Probe and configure the IMU on a 1 MHz mode-3 blocking bus, then hand
/// SPI1 to DMA.
pub fn init_spi1_imu(
    resources: Spi1ImuResources,
    clocks: &CoreClocks,
    delay: &mut CycleDelay,
    storage: SpiDmaStorageResources,
) -> Spi1ImuParts {
    let Spi1ImuResources {
        cs_pin,
        sck_pin,
        miso_pin,
        mosi_pin,
        spi,
        prec,
        rx_dma,
        tx_dma,
    } = resources;

    let mut cs = backend::spi_dma::init_spi1_imu_cs(cs_pin);
    let mut spi = backend::spi_dma::init_spi1_bus(
        spi,
        backend::spi_dma::Spi1BusPins {
            sck: sck_pin,
            miso: miso_pin,
            mosi: mosi_pin,
        },
        hal::spi::MODE_3,
        1_000_000,
        prec,
        clocks,
    );

    delay.delay_ms(10);
    let bringup = match icm42688p::read_who_am_i(&mut spi, &mut cs) {
        Ok(who_am_i) => match Spi1ImuKind::from_who_am_i(who_am_i) {
            Some(kind) => {
                let configured = match kind {
                    Spi1ImuKind::Mpu6500 => mpu6500::init(&mut spi, &mut cs, delay).is_ok(),
                    Spi1ImuKind::Icm42688P => icm42688p::init(&mut spi, &mut cs, delay).is_ok(),
                    Spi1ImuKind::Mpu6000 => mpu6000::init(&mut spi, &mut cs, delay).is_ok(),
                };

                if configured {
                    Spi1ImuBringupStatus::Ready { kind, who_am_i }
                } else {
                    Spi1ImuBringupStatus::ConfigurationFailed { kind, who_am_i }
                }
            }
            None => Spi1ImuBringupStatus::UnsupportedIdentity { who_am_i },
        },
        Err(_) => Spi1ImuBringupStatus::ProbeFailed,
    };

    let backend::spi_dma::Spi1DmaParts {
        irq,
        poller,
        parser,
        recovery_rx_buffer,
    } = backend::spi_dma::init_spi1_dma(spi, rx_dma, tx_dma, storage.into_backend());
    let owner = backend::spi_dma::SpiDmaOwner::new(irq, poller, recovery_rx_buffer, cs);

    Spi1ImuParts {
        owner,
        parser,
        bringup,
    }
}

pub fn init_imu_data_ready(
    pin: PB2,
    syscfg: &mut pac::SYSCFG,
    exti: &mut pac::EXTI,
) -> super::aliases::ImuDataReadyPin {
    backend::exti::init_input(pin.into_floating_input(), syscfg, exti, Edge::Rising)
}

pub fn init_adc1_battery(
    resources: Adc1BatteryResources,
    delay: &mut CycleDelay,
    clocks: &CoreClocks,
    storage: AdcStorageResources,
) -> Adc1ObservationParts {
    let (primary, spare) = storage.split();

    backend::adc::init_adc1_observation(resources, delay, clocks, primary, spare)
}

/// The microSD card as the flash manager's storage. Nothing is written here;
/// the card is claimed, and may be written, on the first JEDEC probe. A card
/// that fails to come up leaves storage disabled rather than stopping boot.
pub fn init_sd_flash(resources: SdCardResources, clocks: &CoreClocks) -> SdFlash {
    let slot = match backend::sd_storage::init_sd_card(resources, clocks) {
        Ok(card) => CardSlot::Ready(card),
        Err(error) => CardSlot::Failed(error),
    };
    backend::sd_storage::SdNor::new(slot)
}
