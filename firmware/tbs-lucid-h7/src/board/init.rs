//! Board bring-up: the Lucid's pins and streams handed to the STM32H7
//! backend.

use super::aliases::{SdFlash, Spi1ImuOwner};
use super::manifest::Spi1ImuKind;
use embedded_hal::delay::DelayNs;
use ferrowasp_drivers::{icm42688p, mpu6000, mpu6500};
use ferrowasp_io_core::serial::{LogicalSerialPort, ResolvedBindings, SerialFunctionSlots};
use ferrowasp_stm32::app_storage::{AdcStorageResources, SpiDmaStorageResources};
use ferrowasp_stm32::status_led::{LedPolarity, StatusLed};
use ferrowasp_stm32::uart_port::{
    SerialPortEndpoint, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
};
use ferrowasp_stm32h7 as backend;
use ferrowasp_stm32h7::eh1::CycleDelay;
use ferrowasp_stm32h7::hal_prelude::*;
use ferrowasp_stm32h7::uart_dma;

const _: () = {
    assert!(Spi1ImuKind::MPU6500_WHO_AM_I == mpu6500::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::ICM42688P_WHO_AM_I == icm42688p::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::MPU6000_WHO_AM_I == mpu6000::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::Mpu6500.dma_burst_register() == mpu6500::Register::AccelXoutH as u8);
    assert!(Spi1ImuKind::Icm42688P.dma_burst_register() == icm42688p::Register::TempData1 as u8);
    assert!(Spi1ImuKind::Mpu6000.dma_burst_register() == mpu6000::Register::AccelXoutH as u8);
};

use super::aliases::{Adc1ObservationParts, DshotMotorBank};
use backend::dshot::channel::{Ch1, Ch2, Ch3, Ch4};
use hal::rcc::ResetEnable;

pub struct Adc1BatteryResources {
    pub adc: pac::ADC1,
    pub prec: rec::Adc12,
    pub voltage_pin: PC0,
    pub current_pin: PC1,
}
pub use backend::dshot::{
    DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS, DshotCommandError, DshotDmaBuffer,
    DshotDmaStorage, DshotInitError, DshotInterruptEvent, DshotMotor, DshotServiceEvent,
    DshotTelemetryRequestError,
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

    let mut cs = cs_pin.into_push_pull_output();
    cs.set_high();
    let mut cs = backend::eh1::OutputPin::new(cs);
    let mut spi = backend::eh1::SpiBus::new(spi.spi(
        (
            sck_pin.into_alternate::<5>(),
            miso_pin.into_alternate::<5>(),
            mosi_pin.into_alternate::<5>(),
        ),
        hal::spi::MODE_3,
        1_000_000.Hz(),
        prec,
        clocks,
    ));

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

    backend::adc::init_adc1_observation(
        resources.adc,
        resources.prec,
        resources.voltage_pin.into_analog(),
        resources.current_pin.into_analog(),
        super::profiles::ADC_REFERENCE_MV,
        delay,
        clocks,
        primary,
        spare,
    )
}

/// The microSD card as the flash manager's storage. Nothing is written here;
/// the card is claimed, and may be written, on the first JEDEC probe. A card
/// that fails to come up leaves storage disabled rather than stopping boot.
/// Logs write back through the card's RAM window; configuration below the
/// log region writes through.
pub fn init_sd_flash(resources: SdCardResources, clocks: &CoreClocks) -> SdFlash {
    let slot = match backend::sd_storage::init_sd_card(resources, clocks) {
        Ok(card) => CardSlot::Ready(card),
        Err(error) => CardSlot::Failed(error),
    };
    backend::sd_storage::SdNor::with_write_back_from(
        slot,
        ferrowasp_flight::flash_storage::LOG_START_ADDRESS,
    )
}

/// The Lucid's two controllable LEDs, per ArduPilot's TBS_LUCID_H7 hwdef:
/// green "B/E" on PE4 and blue "ACT" on PE3. The third LED is power.
pub struct StatusLedPins {
    pub heartbeat: PE4,
    pub armed: PE3,
}

/// Both LEDs light with the pin low: on the bench (2026-10-12), PE3 low lit
/// the blue LED and PE4 high left the green one dark.
pub const STATUS_LED_POLARITY: LedPolarity = LedPolarity::ActiveLow;

pub struct StatusLeds {
    pub heartbeat: StatusLed,
    pub armed: StatusLed,
}

/// Drive both LED pins as push-pull outputs, with both LEDs off.
pub fn init_status_leds(pins: StatusLedPins) -> StatusLeds {
    let StatusLedPins { heartbeat, armed } = pins;
    let heartbeat = backend::eh1::OutputPin::new(heartbeat.into_push_pull_output());
    let armed = backend::eh1::OutputPin::new(armed.into_push_pull_output());
    let heartbeat = cortex_m::singleton!(: backend::eh1::OutputPin<'E', 4> = heartbeat).unwrap();
    let armed = cortex_m::singleton!(: backend::eh1::OutputPin<'E', 3> = armed).unwrap();

    StatusLeds {
        heartbeat: StatusLed::new(heartbeat, STATUS_LED_POLARITY),
        armed: StatusLed::new(armed, STATUS_LED_POLARITY),
    }
}

/// Every UART this board routes, with the DMA streams `routes.rs` gives each.
pub struct UartPortResources {
    /// The DJI connector's SBUS pin, PA10 USART1_RX.
    pub uart1_rx_pin: PA10,
    pub uart1: USART1,
    pub uart1_prec: rec::Usart1,
    pub uart1_rx_dma: Stream4<DMA2>,
    /// PD8 USART3_TX to the DJI unit's RX, PD9 USART3_RX from its TX.
    pub uart3_tx_pin: PD8,
    pub uart3_rx_pin: PD9,
    pub uart3: USART3,
    pub uart3_prec: rec::Usart3,
    pub uart3_rx_dma: Stream1<DMA1>,
    pub uart3_tx_dma: Stream3<DMA1>,
    pub uart6_tx_pin: PC6,
    pub uart6_rx_pin: PC7,
    pub uart6: USART6,
    pub uart6_prec: rec::Usart6,
    pub uart6_rx_dma: Stream0<DMA1>,
    pub uart6_tx_dma: Stream6<DMA1>,
    /// ESC telemetry, PE0 UART8_RX.
    pub uart8_rx_pin: PE0,
    pub uart8: UART8,
    pub uart8_prec: rec::Uart8,
    pub uart8_rx_dma: Stream2<DMA1>,
}

/// Static buffers and stream owners for each port.
pub struct UartPortStorage {
    pub uart1: UartRxPortStorage,
    pub uart3: UartRxTxPortStorage,
    pub uart6: UartRxTxPortStorage,
    pub uart8: UartRxPortStorage,
}

/// The started ports. A port left unbound stays `None`: its peripheral is
/// never enabled, so its interrupts never fire.
pub struct UartPorts {
    pub uart1_rx: Option<super::aliases::Uart1RxPort>,
    pub uart3_rx: Option<super::aliases::Uart3RxPort>,
    pub uart3_tx: UartTxPort<super::aliases::Uart3TxDmaSide>,
    pub uart6_rx: Option<super::aliases::Uart6RxPort>,
    pub uart6_tx: UartTxPort<super::aliases::Uart6TxDmaSide>,
    pub uart8_rx: Option<super::aliases::Uart8RxPort>,
    pub functions: SerialFunctionSlots<SerialPortEndpoint>,
}

/// Start every bound port with its function's line settings and give each
/// function its endpoint.
pub fn init_uart_ports(
    resources: UartPortResources,
    clocks: &CoreClocks,
    storage: UartPortStorage,
    bindings: &ResolvedBindings,
) -> UartPorts {
    let mut functions = SerialFunctionSlots::empty();
    let r = resources;
    let (uart3_rx, uart3_tx) = uart_dma::start_rx_tx_port(
        LogicalSerialPort::Uart3,
        |config| {
            r.uart3
                .serial(
                    (
                        r.uart3_tx_pin.into_alternate::<7>(),
                        r.uart3_rx_pin.into_alternate::<7>(),
                    ),
                    config,
                    r.uart3_prec,
                    clocks,
                )
                .unwrap()
                .release()
        },
        r.uart3_rx_dma,
        r.uart3_tx_dma,
        storage.uart3,
        bindings,
        &mut functions,
    );
    let (uart6_rx, uart6_tx) = uart_dma::start_rx_tx_port(
        LogicalSerialPort::Uart6,
        |config| {
            r.uart6
                .serial(
                    (
                        r.uart6_tx_pin.into_alternate::<7>(),
                        r.uart6_rx_pin.into_alternate::<7>(),
                    ),
                    config,
                    r.uart6_prec,
                    clocks,
                )
                .unwrap()
                .release()
        },
        r.uart6_rx_dma,
        r.uart6_tx_dma,
        storage.uart6,
        bindings,
        &mut functions,
    );
    let uart1_rx = uart_dma::start_rx_port(
        LogicalSerialPort::Uart1,
        |config| {
            r.uart1
                .serial(
                    (hal::serial::NoTx, r.uart1_rx_pin.into_alternate::<7>()),
                    config,
                    r.uart1_prec,
                    clocks,
                )
                .unwrap()
                .release()
        },
        r.uart1_rx_dma,
        storage.uart1,
        bindings,
        &mut functions,
    );
    let uart8_rx = uart_dma::start_rx_port(
        LogicalSerialPort::Uart8,
        |config| {
            r.uart8
                .serial(
                    (hal::serial::NoTx, r.uart8_rx_pin.into_alternate::<8>()),
                    config,
                    r.uart8_prec,
                    clocks,
                )
                .unwrap()
                .release()
        },
        r.uart8_rx_dma,
        storage.uart8,
        bindings,
        &mut functions,
    );
    UartPorts {
        uart1_rx,
        uart3_rx,
        uart3_tx,
        uart6_rx,
        uart6_tx,
        uart8_rx,
        functions,
    }
}

/// The I/O deadline watchdog, on TIM6 at the shared watchdog rate.
pub fn init_io_watchdog(
    timer: pac::TIM6,
    prec: rec::Tim6,
    clocks: &CoreClocks,
) -> super::aliases::IoWatchdog {
    super::aliases::IoWatchdog::new(
        timer,
        prec,
        clocks,
        ferrowasp_stm32::watchdog::IO_WATCHDOG_HZ,
    )
}

pub struct DshotMotorBankResources {
    pub tim3: TIM3,
    pub tim3_rec: rec::Tim3,
    pub tim5: TIM5,
    pub tim5_rec: rec::Tim5,
    pub motor1_pin: PB0,
    pub motor2_pin: PB1,
    pub motor3_pin: PA0,
    pub motor4_pin: PA1,
    pub motor1_dma: Stream0<DMA2>,
    pub motor2_dma: Stream1<DMA2>,
    pub motor3_dma: Stream2<DMA2>,
    pub motor4_dma: Stream3<DMA2>,
}

/// Build the four-motor bank. The pads are latched low as GPIO until both
/// timers are configured with their outputs parked low, then connected.
pub fn init_dshot_motor_bank(
    resources: DshotMotorBankResources,
    clocks: &CoreClocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank, DshotInitError> {
    let r = resources;
    let mut motor1_pin = r.motor1_pin.into_push_pull_output();
    let mut motor2_pin = r.motor2_pin.into_push_pull_output();
    let mut motor3_pin = r.motor3_pin.into_push_pull_output();
    let mut motor4_pin = r.motor4_pin.into_push_pull_output();
    motor1_pin.set_low();
    motor2_pin.set_low();
    motor3_pin.set_low();
    motor4_pin.set_low();

    r.tim3_rec.enable().reset();
    r.tim5_rec.enable().reset();
    let bank = backend::dshot::init_dshot_motor_bank(
        (
            backend::dshot::DshotTimer(r.tim3),
            backend::dshot::DshotTimer(r.tim5),
        ),
        (
            backend::dshot::lane::<TIM3, Ch3, _>(r.motor1_dma),
            backend::dshot::lane::<TIM3, Ch4, _>(r.motor2_dma),
            backend::dshot::lane::<TIM5, Ch1, _>(r.motor3_dma),
            backend::dshot::lane::<TIM5, Ch2, _>(r.motor4_dma),
        ),
        clocks,
        storage,
    )?;

    motor1_pin.into_alternate::<2>().speed(Speed::VeryHigh);
    motor2_pin.into_alternate::<2>().speed(Speed::VeryHigh);
    motor3_pin.into_alternate::<2>().speed(Speed::VeryHigh);
    motor4_pin.into_alternate::<2>().speed(Speed::VeryHigh);
    Ok(bank)
}
