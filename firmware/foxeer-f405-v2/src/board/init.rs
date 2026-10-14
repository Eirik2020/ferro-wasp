use super::aliases::{Adc1ObservationParts, Spi1ImuOwner, Spi1RxTransfer, Spi1TxTransfer};
use super::manifest::Spi1ImuKind;
use ferrowasp_drivers::{icm42688p, mpu6000, mpu6500};
use ferrowasp_io_core::serial::{LogicalSerialPort, ResolvedBindings, SerialFunctionSlots};
use ferrowasp_stm32::app_storage::{AdcStorageResources, SpiDmaStorageResources};
use ferrowasp_stm32::uart_port::{
    SerialPortEndpoint, UartRxPort, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
};
use ferrowasp_stm32f4 as backend;
use ferrowasp_stm32f4::hal_prelude::*;
use ferrowasp_stm32f4::uart_dma;

const _: () = {
    assert!(Spi1ImuKind::MPU6500_WHO_AM_I == mpu6500::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::ICM42688P_WHO_AM_I == icm42688p::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::MPU6000_WHO_AM_I == mpu6000::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::Mpu6500.dma_burst_register() == mpu6500::Register::AccelXoutH as u8);
    assert!(Spi1ImuKind::Icm42688P.dma_burst_register() == icm42688p::Register::TempData1 as u8);
    assert!(Spi1ImuKind::Mpu6000.dma_burst_register() == mpu6000::Register::AccelXoutH as u8);
};

pub use backend::dshot::{
    DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS, DshotCommandError, DshotDmaBuffer,
    DshotDmaStorage, DshotInitError, DshotInterruptEvent, DshotMotor, DshotMotorBank,
    DshotServiceEvent, DshotTelemetryRequestError,
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
    cs.set_high();
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
    pin: super::aliases::ImuDataReadyPin,
    syscfg: &mut hal::syscfg::SysCfg,
    exti: &mut pac::EXTI,
) -> super::aliases::ImuDataReadyPin {
    backend::exti::init_input(pin, syscfg, exti, Edge::Rising)
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

    let mut cs = cs_pin.into_push_pull_output();
    cs.set_high();
    let mode = spi::Mode {
        polarity: spi::Polarity::IdleHigh,
        phase: spi::Phase::CaptureOnSecondTransition,
    };
    let mut spi = Spi::new(
        spi,
        (
            Some(sck_pin.into_alternate()),
            Some(miso_pin.into_alternate()),
            Some(mosi_pin.into_alternate()),
        ),
        mode,
        1_000_000.Hz(),
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

    backend::adc::init_adc1_observation_for::<_, _, _, 0>(
        resources.adc,
        resources.voltage_pin.into_analog(),
        resources.current_pin.into_analog(),
        resources.dma,
        rcc,
        primary,
        spare,
    )
}

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

pub fn init_dshot_motor_bank(
    resources: DshotMotorBankResources,
    clocks: &hal::rcc::Clocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank, DshotInitError> {
    backend::dshot::new_foxeer(
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

/// Every UART this board routes, by logical port.
pub struct UartPortResources {
    pub uart1_rx_pin: super::aliases::Usart1EscTelemetryRxPin,
    pub uart1: USART1,
    pub uart1_rx_dma: Stream5<DMA2>,
    pub uart2_tx_pin: super::aliases::Usart2TxPin,
    pub uart2_rx_pin: super::aliases::Usart2RxPin,
    pub uart2: USART2,
    pub uart2_rx_dma: Stream5<DMA1>,
    pub uart2_tx_dma: Stream6<DMA1>,
    pub uart3_tx_pin: super::aliases::Usart3TxPin,
    pub uart3_rx_pin: super::aliases::Usart3RxPin,
    pub uart3: pac::USART3,
    pub uart3_rx_dma: Stream1<DMA1>,
    pub uart3_tx_dma: Stream3<DMA1>,
    pub uart4_tx_pin: super::aliases::Uart4TxPin,
    pub uart4_rx_pin: super::aliases::Uart4RxPin,
    pub uart4: UART4,
    pub uart4_rx_dma: Stream2<DMA1>,
    pub uart4_tx_dma: Stream4<DMA1>,
}

/// Static buffers and stream owners for each port.
pub struct UartPortStorage {
    pub uart1: UartRxPortStorage,
    pub uart2: UartRxTxPortStorage,
    pub uart3: UartRxTxPortStorage,
    pub uart4: UartRxTxPortStorage,
}

/// The started ports. A port left unbound stays `None`: its peripheral is
/// never enabled, so its interrupts never fire.
pub struct UartPorts {
    pub uart1_rx: Option<UartRxPort<super::aliases::Uart1RxIrq>>,
    pub uart2_rx: Option<UartRxPort<super::aliases::Uart2RxIrq>>,
    pub uart2_tx: UartTxPort<super::aliases::Uart2TxDmaSide>,
    pub uart3_rx: Option<UartRxPort<super::aliases::Uart3RxIrq>>,
    pub uart3_tx: UartTxPort<super::aliases::Uart3TxDmaSide>,
    pub uart4_rx: Option<UartRxPort<super::aliases::Uart4RxIrq>>,
    pub uart4_tx: UartTxPort<super::aliases::Uart4TxDmaSide>,
    pub functions: SerialFunctionSlots<SerialPortEndpoint>,
}

/// Start every bound port with its function's line settings and give each
/// function its endpoint.
pub fn init_uart_ports(
    resources: UartPortResources,
    rcc: &mut Rcc,
    storage: UartPortStorage,
    bindings: &ResolvedBindings,
) -> UartPorts {
    let mut functions = SerialFunctionSlots::empty();
    let uart1_rx = uart_dma::start_rx_port::<_, _, _, 4>(
        LogicalSerialPort::Uart1,
        resources.uart1_rx_pin.into_alternate::<7>(),
        resources.uart1,
        resources.uart1_rx_dma,
        rcc,
        storage.uart1,
        bindings,
        &mut functions,
    );
    let (uart2_rx, uart2_tx) = uart_dma::start_rx_tx_port::<_, _, _, _, _, 4, 4>(
        LogicalSerialPort::Uart2,
        resources.uart2_tx_pin.into_alternate::<7>(),
        resources.uart2_rx_pin.into_alternate::<7>(),
        resources.uart2,
        resources.uart2_rx_dma,
        resources.uart2_tx_dma,
        rcc,
        storage.uart2,
        bindings,
        &mut functions,
    );
    // Often nothing is plugged into R3; the pull-up holds an open line idle
    // instead of letting noise reach the command line.
    let (uart3_rx, uart3_tx) = uart_dma::start_rx_tx_port::<_, _, _, _, _, 4, 4>(
        LogicalSerialPort::Uart3,
        resources.uart3_tx_pin.into_alternate::<7>(),
        resources
            .uart3_rx_pin
            .into_alternate::<7>()
            .internal_pull_up(true),
        resources.uart3,
        resources.uart3_rx_dma,
        resources.uart3_tx_dma,
        rcc,
        storage.uart3,
        bindings,
        &mut functions,
    );
    let (uart4_rx, uart4_tx) = uart_dma::start_rx_tx_port::<_, _, _, _, _, 4, 4>(
        LogicalSerialPort::Uart4,
        resources.uart4_tx_pin.into_alternate::<8>(),
        resources.uart4_rx_pin.into_alternate::<8>(),
        resources.uart4,
        resources.uart4_rx_dma,
        resources.uart4_tx_dma,
        rcc,
        storage.uart4,
        bindings,
        &mut functions,
    );
    UartPorts {
        uart1_rx,
        uart2_rx,
        uart2_tx,
        uart3_rx,
        uart3_tx,
        uart4_rx,
        uart4_tx,
        functions,
    }
}
