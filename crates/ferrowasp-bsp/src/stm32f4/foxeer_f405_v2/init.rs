use super::aliases::{Adc1ObservationParts, Spi1ImuOwner, Spi1RxTransfer, Spi1TxTransfer};
use super::manifest::Spi1ImuKind;
use super::storage::{AdcStorageResources, SpiDmaStorageResources, UartRxStorageResources};
use ferrowasp_drivers::{icm42688p, mpu6500};
use ferrowasp_mspv1 as mspv1;
use ferrowasp_stm32f4 as backend;
use ferrowasp_stm32f4::hal_prelude::*;

const _: () = {
    assert!(Spi1ImuKind::MPU6500_WHO_AM_I == mpu6500::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::ICM42688P_WHO_AM_I == icm42688p::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::Mpu6500.dma_burst_register() == mpu6500::Register::AccelXoutH as u8);
    assert!(Spi1ImuKind::Icm42688P.dma_burst_register() == icm42688p::Register::TempData1 as u8);
};

pub use backend::adc::Adc1BatteryResources;
#[cfg(feature = "foxeer-f405-v2-dshot-four")]
pub use backend::dshot::{
    DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS, DshotCommandError, DshotDmaBuffer,
    DshotDmaStorage, DshotInitError, DshotInterruptEvent, DshotMotor, DshotMotorBank,
    DshotServiceEvent, DshotTelemetryRequestError,
};
pub use backend::uart_dma::{
    Uart4MspParts, Uart4MspResources, Usart1EscTelemetryResources, Usart2SbusResources,
};

pub struct Spi1ImuResources {
    pub cs_pin: PA4<Input>,
    pub sck_pin: PA5<Input>,
    pub miso_pin: PA6<Input>,
    pub mosi_pin: PA7<Input>,
    pub spi: SPI1,
    pub rx_dma: Stream0<DMA2>,
    pub tx_dma: Stream3<DMA2>,
}

pub struct Spi1ImuParts {
    pub owner: Spi1ImuOwner,
    pub parser: backend::spi_dma::SpiRxParserSide,
    pub bringup: Spi1ImuBringupStatus,
}

pub struct Spi2FlashResources {
    pub cs_pin: super::aliases::Spi2FlashCsPin,
    pub sck_pin: super::aliases::Spi2FlashSckPin,
    pub miso_pin: super::aliases::Spi2FlashMisoPin,
    pub mosi_pin: super::aliases::Spi2FlashMosiPin,
    pub spi: pac::SPI2,
}

pub fn init_spi2_flash(
    resources: Spi2FlashResources,
    clocks: &mut Rcc,
) -> super::aliases::Spi2Flash {
    let mut cs = resources.cs_pin.into_push_pull_output();
    let _ = cs.set_high();
    let mode = spi::Mode {
        polarity: spi::Polarity::IdleLow,
        phase: spi::Phase::CaptureOnFirstTransition,
    };
    let bus = Spi::new(
        resources.spi,
        (
            Some(resources.sck_pin.into_alternate()),
            Some(resources.miso_pin.into_alternate()),
            Some(resources.mosi_pin.into_alternate()),
        ),
        mode,
        10_000_000.Hz(),
        clocks,
    );
    ferrowasp_drivers::spi_nor::SpiNor::new(bus, cs)
}

pub fn init_imu_data_ready(
    mut pin: super::aliases::ImuDataReadyPin,
    syscfg: &mut hal::syscfg::SysCfg,
    exti: &mut pac::EXTI,
) -> super::aliases::ImuDataReadyPin {
    pin.make_interrupt_source(syscfg);
    pin.trigger_on_edge(exti, Edge::Rising);
    pin.clear_interrupt_pending_bit();
    pin.enable_interrupt(exti);
    pin
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

pub struct Adc1BatteryResourcesFoxeer {
    pub adc: ADC1,
    pub voltage_pin: PC0<Input>,
    pub current_pin: PC1<Input>,
    pub dma: Stream4<DMA2>,
}

pub fn init_usart2_sbus(
    resources: Usart2SbusResources,
    clocks: &mut Rcc,
    storage: UartRxStorageResources,
) -> backend::uart_dma::UartRxParts<Stream5<DMA1>, USART2, 4> {
    backend::uart_dma::init_usart2_sbus_rx_dma(resources, clocks, storage.into_backend())
}

pub fn init_usart1_esc_telemetry(
    resources: Usart1EscTelemetryResources,
    clocks: &mut Rcc,
    storage: UartRxStorageResources,
) -> backend::uart_dma::UartRxParts<Stream5<DMA2>, USART1, 4> {
    backend::uart_dma::init_usart1_esc_telemetry_rx_dma(resources, clocks, storage.into_backend())
}

pub fn init_uart4_msp_osd(
    resources: Uart4MspResources,
    clocks: &mut Rcc,
    rx_storage: UartRxStorageResources,
    tx_buffer: &'static mut [u8; mspv1::OSD_TX_BUFFER_LEN],
) -> Uart4MspParts {
    let tx_dma = resources.tx_dma;
    let uart4 = backend::uart_dma::init_uart4_msp_rx_dma_with_tx(
        backend::uart_dma::Uart4MspRxResources {
            tx_pin: resources.tx_pin,
            rx_pin: resources.rx_pin,
            uart: resources.uart,
            rx_dma: resources.rx_dma,
        },
        clocks,
        rx_storage.into_backend(),
    );

    Uart4MspParts {
        rx_irq: uart4.irq,
        parser: uart4.parser,
        tx_dma: backend::uart_dma::init_uart4_tx_dma(tx_dma, uart4.tx, tx_buffer),
    }
}

pub fn init_spi1_imu<D>(
    resources: Spi1ImuResources,
    clocks: &mut Rcc,
    delay: &mut D,
    storage: SpiDmaStorageResources,
) -> Spi1ImuParts
where
    D: hal::hal::delay::DelayNs,
{
    let Spi1ImuResources {
        cs_pin,
        sck_pin,
        miso_pin,
        mosi_pin,
        spi,
        rx_dma,
        tx_dma,
    } = resources;

    let mut cs = backend::spi_dma::init_spi1_imu_cs(cs_pin);
    let mode = spi::Mode {
        polarity: spi::Polarity::IdleHigh,
        phase: spi::Phase::CaptureOnSecondTransition,
    };
    let mut spi =
        backend::spi_dma::init_spi1_bus(spi, sck_pin, miso_pin, mosi_pin, mode, 1_000_000, clocks);

    delay.delay_ms(10);
    let bringup = match icm42688p::read_who_am_i(&mut spi, &mut cs) {
        Ok(who_am_i) => match Spi1ImuKind::from_who_am_i(who_am_i) {
            Some(kind) => {
                let configured = match kind {
                    Spi1ImuKind::Mpu6500 => mpu6500::init(&mut spi, &mut cs, delay).is_ok(),
                    Spi1ImuKind::Icm42688P => icm42688p::init(&mut spi, &mut cs, delay).is_ok(),
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

    let dma = backend::spi_dma::init_spi_dma::<_, _, _, 3, 3>(
        spi,
        rx_dma,
        tx_dma,
        storage.into_backend(),
    );
    let backend::spi_dma::SpiDmaParts {
        irq,
        poller,
        parser,
        recovery_rx_buffer,
    }: backend::spi_dma::SpiDmaParts<Spi1RxTransfer, Spi1TxTransfer> = dma;
    let owner = backend::spi_dma::SpiDmaOwner::new(irq, poller, recovery_rx_buffer, cs);

    Spi1ImuParts {
        owner,
        parser,
        bringup,
    }
}

pub fn init_adc1_battery(
    resources: Adc1BatteryResourcesFoxeer,
    rcc: &mut Rcc,
    storage: AdcStorageResources,
) -> Adc1ObservationParts {
    let (primary, spare) = storage.split();

    backend::adc::init_adc1_observation_for::<_, 0>(
        resources.adc,
        resources.voltage_pin,
        resources.current_pin,
        resources.dma,
        rcc,
        primary,
        spare,
    )
}

#[cfg(feature = "foxeer-f405-v2-dshot-four")]
pub struct DshotMotorBankResources {
    pub tim1: Timer<TIM1>,
    pub tim8: Timer<TIM8>,
    pub motor1_pin: super::aliases::Motor1Pin,
    pub motor2_pin: super::aliases::Motor2Pin,
    pub motor3_pin: super::aliases::Motor3Pin,
    pub motor4_pin: super::aliases::Motor4Pin,
    pub motor1_dma: Stream1<DMA2>,
    pub motor2_dma: Stream7<DMA2>,
    pub motor3_dma: Stream2<DMA2>,
    pub motor4_dma: Stream6<DMA2>,
}

#[cfg(feature = "foxeer-f405-v2-dshot-four")]
pub fn init_dshot_motor_bank(
    resources: DshotMotorBankResources,
    clocks: &hal::rcc::Clocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank, DshotInitError> {
    DshotMotorBank::new_foxeer(
        resources.motor1_pin,
        resources.motor2_pin,
        resources.motor3_pin,
        resources.motor4_pin,
        resources.tim1,
        resources.tim8,
        resources.motor1_dma,
        resources.motor2_dma,
        resources.motor3_dma,
        resources.motor4_dma,
        clocks,
        storage,
    )
}
