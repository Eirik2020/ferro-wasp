//! Board bring-up: the Lucid's pins and streams handed to the STM32H7
//! backend.

use super::aliases::{Adc1ObservationTransfer, DshotMotorBank, SdFlash, Spi1ImuOwner};
use super::manifest::Spi1ImuKind;
use core::cell::RefCell;
use critical_section::Mutex;
use ferrowasp_icm42688p as icm42688p;
use ferrowasp_mpu6000 as mpu6000;
use ferrowasp_mpu6500 as mpu6500;
use ferrowasp_serial::{LogicalSerialPort, ResolvedBindings, SerialFunctionSlots};
use ferrowasp_stm32 as backend;
use ferrowasp_stm32::app_storage::{AdcStorageResources, SpiDmaStorageResources};
use ferrowasp_stm32::chip_time::CycleDelay;
use ferrowasp_stm32::hal::{
    adc::{Adc, Config as AdcConfig},
    chip::{ADC1, I2C2, SDMMC1, SPI1, TIM2, TIM3, TIM4, TIM5, TIM6, UART8, USART1, USART3, USART6},
    dma::Stream,
    exti::{self, Edge},
    gpio::{Alternate, Analog, Input, Output, Pin},
    i2c::{Config as I2cConfig, I2c, Wiring as I2cWiring},
    rcc::{
        AhbPrescaler, ApbPrescaler, Clocks, Config as ClockConfig, Hse, HseMode, Pll, PllSource,
        Rcc, Supply, Sysclk, VoltageScale,
    },
    sdmmc::{self, Sdmmc},
    spi::{self, Spi, Wiring as SpiWiring},
    time::Hertz,
    timer::{Awg, Pwm, Tick, Timebase},
    uart::{RxWiring, Wiring as UartWiring},
};
use ferrowasp_stm32::status_led::{LedPolarity, StatusLed};
use ferrowasp_stm32::uart_dma::{self, PortMemory};
use ferrowasp_stm32::uart_port::{
    SerialPortEndpoint, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
};
use spl06;

/// The board's 8 MHz crystal to a 400 MHz core: 200 MHz AXI and AHB, every APB at
/// 100 MHz, 100 MHz on PLL1Q for SPI1 to SPI3. USB runs from HSI48. Checked against
/// the H743's limits as it builds.
pub const CLOCK_CONFIG: ClockConfig = ClockConfig {
    hse: Some(Hse {
        freq: Hertz::mhz(8),
        mode: HseMode::Oscillator,
    }),
    pll1: Some(Pll {
        source: PllSource::Hse,
        prediv: 1,
        mul: 100,
        divp: Some(2),
        divq: Some(8),
        divr: None,
    }),
    sys: Sysclk::Pll1P,
    cpu_pre: AhbPrescaler::Div1,
    ahb_pre: AhbPrescaler::Div2,
    apb1_pre: ApbPrescaler::Div2,
    apb2_pre: ApbPrescaler::Div2,
    apb3_pre: ApbPrescaler::Div2,
    apb4_pre: ApbPrescaler::Div2,
    supply: Supply::Ldo,
    voltage_scale: VoltageScale::Scale1,
};
pub const CLOCKS: Clocks = match CLOCK_CONFIG.validate() {
    Ok(clocks) => clocks,
    Err(_) => panic!("CLOCK_CONFIG breaks the H743's limits"),
};
const _: () = assert!(CLOCKS.cpu.0 == backend::clocks::SYSTEM_CLOCK_HZ);
const _: () =
    assert!(Tick::check::<TIM4>(&CLOCKS, Hertz(super::profiles::SCHEDULER_TICK_RATE_HZ)).is_ok());
const _: () =
    assert!(Tick::check::<TIM6>(&CLOCKS, Hertz(ferrowasp_stm32::watchdog::IO_WATCHDOG_HZ)).is_ok());
const _: () = assert!(
    Timebase::check::<TIM2>(
        &CLOCKS,
        Hertz(ferrowasp_stm32::timebase::MICROSECOND_TIMEBASE_HZ)
    )
    .is_ok()
);

/// The IMU bus: the rate and mode the targets validated, in FerroHAL's terms.
const IMU_SPI: spi::Config = spi::Config::new(Hertz::mhz(1)).with_mode(spi::MODE_3);
const _: () = assert!(Spi::check::<SPI1>(&CLOCKS, &IMU_SPI).is_ok());

const _: () = {
    assert!(Spi1ImuKind::MPU6500_WHO_AM_I == mpu6500::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::ICM42688P_WHO_AM_I == icm42688p::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::MPU6000_WHO_AM_I == mpu6000::WHO_AM_I_EXPECTED);
    assert!(Spi1ImuKind::Mpu6500.dma_burst_register() == mpu6500::Register::AccelXoutH as u8);
    assert!(Spi1ImuKind::Icm42688P.dma_burst_register() == icm42688p::Register::TempData1 as u8);
    assert!(Spi1ImuKind::Mpu6000.dma_burst_register() == mpu6000::Register::AccelXoutH as u8);
};

pub struct Adc1BatteryResources {
    pub adc: ADC1,
    pub voltage_pin: Pin<'C', 0, Analog>,
    pub current_pin: Pin<'C', 1, Analog>,
    pub dma: Stream<2, 5>,
}
pub use backend::dshot::{
    DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS, DshotCommandError, DshotDmaBuffer,
    DshotDmaStorage, DshotInitError, DshotInterruptEvent, DshotMotor, DshotServiceEvent,
    DshotTelemetryRequestError,
};
use backend::sd_storage::CardSlot;

pub struct Spi1ImuResources {
    pub cs_pin: super::aliases::Spi1ImuCs,
    pub sck_pin: Pin<'A', 5, Alternate<5>>,
    pub miso_pin: Pin<'A', 6, Alternate<5>>,
    pub mosi_pin: Pin<'D', 7, Alternate<5>>,
    pub spi: SPI1,
    pub rx_dma: Stream<1, 4>,
    pub tx_dma: Stream<1, 5>,
}

pub struct Spi1ImuParts {
    pub owner: Spi1ImuOwner,
    pub parser: backend::spi_dma::SpiRxParserSide,
    pub bringup: Spi1ImuBringupStatus,
}

/// The IMU driver's own memory, beside FerroWasp's rotating buffers.
pub struct Spi1Memory {
    pub core: backend::spi_dma::SharedSpi,
    pub tx: &'static mut [u8],
    pub rx: &'static mut [u8],
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

/// Probe and configure the IMU on a 1 MHz mode-3 bus, then hand SPI1 to the owner.
/// The internal barometer's bus, at 100 kHz: ArduPilot's `TBS_LUCID_H7`
/// hwdef carries the part's pull-ups, and 100 kHz is generous for them.
/// I2C2 times it from the 100 MHz APB1 clock.
const BARO_I2C: I2cConfig = I2cConfig::new(Hertz::khz(100));
const _: () = if BARO_I2C.check(Hertz(CLOCKS.pclk1.0)).is_err() {
    panic!("I2C2 cannot time the barometer bus from the APB1 clock")
};

/// I2C2 for the internal barometer, on PB10/PB11.
pub struct I2c2BaroResources {
    pub scl_pin: Pin<'B', 10, Alternate<4>>,
    pub sda_pin: Pin<'B', 11, Alternate<4>>,
    pub i2c: I2C2,
}

/// Open I2C2 and probe its barometer at the board's address: the shared
/// task measures an SPL06-007, and this board's part is a DPS368 or a
/// BMP390, so the probe reports what answers instead. When no barometer
/// answers the bus is handed over open, so the task keeps probing and
/// sweeps the bus; only a bus that refuses to open reports absence.
pub fn init_i2c2_baro(
    resources: I2c2BaroResources,
    rcc: &mut Rcc,
) -> backend::tasks::baro::BaroHandover {
    let i2c = match I2c::new(
        rcc,
        I2cWiring {
            i2c: resources.i2c,
            scl: resources.scl_pin,
            sda: resources.sda_pin,
        },
        &BARO_I2C,
    ) {
        Ok(i2c) => i2c,
        Err(refused) => {
            defmt::warn!("I2C2 barometer bus refused: {:?}", refused.error());
            return backend::tasks::baro::BaroHandover::Absent;
        }
    };
    match backend::tasks::baro::probe_spl06(spl06::Spl06::new(
        i2c,
        super::manifest::BARO_I2C_ADDRESS,
    )) {
        Ok(probe) => backend::tasks::baro::BaroHandover::Probe(probe),
        Err(failed) => {
            defmt::warn!(
                "I2C2 barometer probe stopped at stage {} (chip ID {:#x})",
                failed.stage(),
                failed.chip_id()
            );
            backend::tasks::baro::BaroHandover::Open(failed.release())
        }
    }
}

pub fn init_spi1_imu(
    resources: Spi1ImuResources,
    rcc: &mut Rcc,
    delay: &mut CycleDelay,
    storage: SpiDmaStorageResources,
    memory: Spi1Memory,
) -> Option<Spi1ImuParts> {
    let Spi1ImuResources {
        mut cs_pin,
        sck_pin,
        miso_pin,
        mosi_pin,
        spi,
        rx_dma,
        tx_dma,
    } = resources;

    cs_pin.set_high();
    let mut spi = match Spi::new(
        rcc,
        SpiWiring {
            spi,
            sck: sck_pin,
            miso: miso_pin,
            mosi: mosi_pin,
            tx_dma,
            rx_dma,
        },
        &IMU_SPI,
        spi::Buffers {
            tx: memory.tx,
            rx: memory.rx,
        },
    ) {
        Ok(spi) => spi,
        Err(refused) => {
            defmt::warn!("SPI1 IMU bus refused: {:?}", refused.error());
            return None;
        }
    };

    embedded_hal::delay::DelayNs::delay_ms(delay, 10);
    let cs = &mut cs_pin;
    let bringup = match icm42688p::read_who_am_i(&mut spi, cs) {
        Ok(who_am_i) => match Spi1ImuKind::from_who_am_i(who_am_i) {
            Some(kind) => {
                let configured = match kind {
                    Spi1ImuKind::Mpu6500 => mpu6500::init(&mut spi, cs, delay).is_ok(),
                    Spi1ImuKind::Icm42688P => icm42688p::init(&mut spi, cs, delay).is_ok(),
                    Spi1ImuKind::Mpu6000 => mpu6000::init(&mut spi, cs, delay).is_ok(),
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

    let parts = backend::spi_dma::init_spi_dma(spi, memory.core, storage.into_backend());
    let (owner, parser) = backend::spi_dma::owner(parts, cs_pin);
    Some(Spi1ImuParts {
        owner,
        parser,
        bringup,
    })
}

/// The IMU's data-ready line on PB2. The pin has to be an input: an H7 pin resets to
/// analog, whose input buffer is off, and EXTI would never see an edge.
pub fn init_imu_data_ready(
    _pin: Pin<'B', 2, Input>,
    line: exti::Line<2>,
    ctrl: &mut exti::Ctrl,
) -> super::aliases::ImuDataReadyPin {
    let mut routed = line.route::<'B'>(ctrl, Edge::Rising);
    routed.clear();
    routed.listen(ctrl);
    backend::exti::ExtiInput(routed.handle())
}

/// ADC1 reading the battery's voltage and current over DMA, in millivolts of the
/// reference this board's profile states.
pub fn init_adc1_battery(
    resources: Adc1BatteryResources,
    rcc: &mut Rcc,
    storage: AdcStorageResources,
) -> Option<(Adc1ObservationTransfer, &'static mut [u16; 3])> {
    let (primary, spare) = storage.split();
    let adc = Adc::new(rcc, resources.adc, &AdcConfig::slow_source()).ok()?;
    let max_sample = adc.max_sample();
    let channels = [
        adc.channel(&resources.voltage_pin),
        adc.channel(&resources.current_pin),
    ];
    let scan = adc
        .into_scan(resources.dma, &channels, &mut primary[..2])
        .ok()?;
    Some((
        Adc1ObservationTransfer::new(scan, 0, 1, super::profiles::ADC_REFERENCE_MV, max_sample),
        spare,
    ))
}

/// The microSD card as the flash manager's storage. Nothing is written here;
/// the card is claimed, and may be written, on the first JEDEC probe. A card
/// that fails to come up leaves storage disabled rather than stopping boot.
/// Logs write back through the card's RAM window; configuration below the
/// log region writes through.
pub fn init_sd_flash(wiring: SdWiring, rcc: &mut Rcc) -> Option<SdFlash> {
    let sd = Sdmmc::new(rcc, wiring, &sdmmc::Config::new(Hertz::mhz(25))).ok()?;
    let slot = match backend::sd_storage::init_sd_card(sd) {
        Ok(card) => CardSlot::Ready(card),
        Err(error) => CardSlot::Failed(error),
    };
    Some(backend::sd_storage::SdNor::with_write_back_from(
        slot,
        ferrowasp_storage::layout::LOG_START_ADDRESS,
    ))
}

/// The card's six lines, in their alternate function.
pub type SdWiring = sdmmc::Wiring<
    SDMMC1,
    Pin<'C', 12, Alternate<12>>,
    Pin<'D', 2, Alternate<12>>,
    Pin<'C', 8, Alternate<12>>,
    Pin<'C', 9, Alternate<12>>,
    Pin<'C', 10, Alternate<12>>,
    Pin<'C', 11, Alternate<12>>,
>;

/// The Lucid's two controllable LEDs, per ArduPilot's TBS_LUCID_H7 hwdef:
/// green "B/E" on PE4 and blue "ACT" on PE3. The third LED is power.
pub struct StatusLedPins {
    pub heartbeat: Pin<'E', 4, Output>,
    pub armed: Pin<'E', 3, Output>,
}

/// Both LEDs light with the pin low: on the bench (2026-10-12), PE3 low lit
/// the blue LED and PE4 high left the green one dark.
pub const STATUS_LED_POLARITY: LedPolarity = LedPolarity::ActiveLow;

pub struct StatusLeds {
    pub heartbeat: StatusLed,
    pub armed: StatusLed,
}

/// Takes both LED pins, with both LEDs off.
pub fn init_status_leds(pins: StatusLedPins) -> StatusLeds {
    let heartbeat = cortex_m::singleton!(: Pin<'E', 4, Output> = pins.heartbeat).unwrap();
    let armed = cortex_m::singleton!(: Pin<'E', 3, Output> = pins.armed).unwrap();
    StatusLeds {
        heartbeat: StatusLed::new(heartbeat, STATUS_LED_POLARITY),
        armed: StatusLed::new(armed, STATUS_LED_POLARITY),
    }
}

/// Every UART this board routes, with the DMA streams `routes.rs` gives each.
pub struct UartPortResources {
    /// The DJI connector's SBUS pin, PA10 USART1_RX.
    pub uart1_rx_pin: Pin<'A', 10, Alternate<7>>,
    pub uart1: USART1,
    pub uart1_rx_dma: Stream<2, 4>,
    /// PD8 USART3_TX to the DJI unit's RX, PD9 USART3_RX from its TX.
    pub uart3_tx_pin: Pin<'D', 8, Alternate<7>>,
    pub uart3_rx_pin: Pin<'D', 9, Alternate<7>>,
    pub uart3: USART3,
    pub uart3_rx_dma: Stream<1, 1>,
    pub uart3_tx_dma: Stream<1, 3>,
    pub uart6_tx_pin: Pin<'C', 6, Alternate<7>>,
    pub uart6_rx_pin: Pin<'C', 7, Alternate<7>>,
    pub uart6: USART6,
    pub uart6_rx_dma: Stream<1, 0>,
    pub uart6_tx_dma: Stream<1, 6>,
    /// ESC telemetry, PE0 UART8_RX.
    pub uart8_rx_pin: Pin<'E', 0, Alternate<8>>,
    pub uart8: UART8,
    pub uart8_rx_dma: Stream<1, 2>,
}

/// Static buffers and stream owners for each port.
pub struct UartPortStorage {
    pub uart1: UartRxPortStorage,
    pub uart3: UartRxTxPortStorage,
    pub uart6: UartRxTxPortStorage,
    pub uart8: UartRxPortStorage,
}

/// Each port's driver cell and receive ring.
pub struct UartPortMemory {
    pub uart1: PortMemory,
    pub uart3: PortMemory,
    pub uart6: PortMemory,
    pub uart8: PortMemory,
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
/// function its endpoint. The Lucid's USART1 connector takes SBUS as the old HAL set
/// it up, with nothing inverted here.
pub fn init_uart_ports(
    resources: UartPortResources,
    rcc: &mut Rcc,
    storage: UartPortStorage,
    memory: UartPortMemory,
    bindings: &ResolvedBindings,
) -> UartPorts {
    let mut functions = SerialFunctionSlots::empty();
    let r = resources;
    let (uart3_rx, uart3_tx) = uart_dma::start_rx_tx_port(
        LogicalSerialPort::Uart3,
        UartWiring {
            uart: r.uart3,
            tx: r.uart3_tx_pin,
            rx: r.uart3_rx_pin,
            tx_dma: r.uart3_tx_dma,
            rx_dma: r.uart3_rx_dma,
        },
        rcc,
        storage.uart3,
        memory.uart3,
        false,
        bindings,
        &mut functions,
    );
    let (uart6_rx, uart6_tx) = uart_dma::start_rx_tx_port(
        LogicalSerialPort::Uart6,
        UartWiring {
            uart: r.uart6,
            tx: r.uart6_tx_pin,
            rx: r.uart6_rx_pin,
            tx_dma: r.uart6_tx_dma,
            rx_dma: r.uart6_rx_dma,
        },
        rcc,
        storage.uart6,
        memory.uart6,
        false,
        bindings,
        &mut functions,
    );
    let uart1_rx = uart_dma::start_rx_port(
        LogicalSerialPort::Uart1,
        RxWiring {
            uart: r.uart1,
            rx: r.uart1_rx_pin,
            rx_dma: r.uart1_rx_dma,
        },
        rcc,
        storage.uart1,
        memory.uart1,
        false,
        bindings,
        &mut functions,
    );
    let uart8_rx = uart_dma::start_rx_port(
        LogicalSerialPort::Uart8,
        RxWiring {
            uart: r.uart8,
            rx: r.uart8_rx_pin,
            rx_dma: r.uart8_rx_dma,
        },
        rcc,
        storage.uart8,
        memory.uart8,
        false,
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

pub struct DshotMotorBankResources {
    pub tim3: Pwm<TIM3>,
    pub tim5: Pwm<TIM5>,
    pub motor1_pin: Pin<'B', 0, Alternate<2>>,
    pub motor2_pin: Pin<'B', 1, Alternate<2>>,
    pub motor3_pin: Pin<'A', 0, Alternate<2>>,
    pub motor4_pin: Pin<'A', 1, Alternate<2>>,
    pub motor1_dma: Stream<2, 0>,
    pub motor2_dma: Stream<2, 1>,
    pub motor3_dma: Stream<2, 2>,
    pub motor4_dma: Stream<2, 3>,
}

/// Lane storage, for the generator's own copies of each frame.
pub struct MotorLaneMemory {
    pub motor1: &'static mut [u32],
    pub motor2: &'static mut [u32],
    pub motor3: &'static mut [u32],
    pub motor4: &'static mut [u32],
}

/// Build the four-motor bank on the waveform generator. Every channel comes up at
/// duty 0 with its timer stopped, so no pad sees a pulse. The two timers start in
/// software, as they always have here: there's no trigger route between them.
pub fn init_dshot_motor_bank(
    resources: DshotMotorBankResources,
    memory: MotorLaneMemory,
    clocks: &Clocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank, DshotInitError> {
    let mut r = resources;
    let channel3 = r.tim3.channel::<3, _>(r.motor1_pin);
    let channel4 = r.tim3.channel::<4, _>(r.motor2_pin);
    let channel1 = r.tim5.channel::<1, _>(r.motor3_pin);
    let channel2 = r.tim5.channel::<2, _>(r.motor4_pin);
    let (Ok(lane1), Ok(lane2), Ok(lane3), Ok(lane4)) = (
        channel3.into_lane(r.motor1_dma, memory.motor1),
        channel4.into_lane(r.motor2_dma, memory.motor2),
        channel1.into_lane(r.motor3_dma, memory.motor3),
        channel2.into_lane(r.motor4_dma, memory.motor4),
    ) else {
        return Err(DshotInitError::LaneTimerMissing);
    };
    let awg = Awg::independent(r.tim3, r.tim5, (lane1, lane2), (lane3, lane4))
        .map_err(|_| DshotInitError::LaneClocksDiffer)?;
    // APB1 timers, both.
    backend::dshot::init_dshot_motor_bank(
        awg,
        super::aliases::MOTOR_LANES,
        clocks.timclk1.0,
        storage,
    )
}

static UART1: Mutex<RefCell<Option<uart_dma::UartCore>>> = Mutex::new(RefCell::new(None));
static UART3: Mutex<RefCell<Option<uart_dma::UartCore>>> = Mutex::new(RefCell::new(None));
static UART6: Mutex<RefCell<Option<uart_dma::UartCore>>> = Mutex::new(RefCell::new(None));
static UART8_CORE: Mutex<RefCell<Option<uart_dma::UartCore>>> = Mutex::new(RefCell::new(None));
static SPI1_CORE: Mutex<RefCell<Option<backend::spi_dma::SpiCore>>> =
    Mutex::new(RefCell::new(None));

/// Each port's driver cell and receive ring. Call it once, from `init`.
pub fn uart_port_memory() -> UartPortMemory {
    const RING: usize = uart_dma::RX_RING_BYTES;
    UartPortMemory {
        uart1: PortMemory {
            core: &UART1,
            ring: cortex_m::singleton!(: [u8; RING] = [0; RING]).unwrap(),
        },
        uart3: PortMemory {
            core: &UART3,
            ring: cortex_m::singleton!(: [u8; RING] = [0; RING]).unwrap(),
        },
        uart6: PortMemory {
            core: &UART6,
            ring: cortex_m::singleton!(: [u8; RING] = [0; RING]).unwrap(),
        },
        uart8: PortMemory {
            core: &UART8_CORE,
            ring: cortex_m::singleton!(: [u8; RING] = [0; RING]).unwrap(),
        },
    }
}

/// The IMU driver's cell and buffers. Call it once, from `init`.
pub fn spi1_memory() -> Spi1Memory {
    const LEN: usize = backend::spi_dma::SPI_BUFFER_SIZE;
    Spi1Memory {
        core: &SPI1_CORE,
        tx: cortex_m::singleton!(: [u8; LEN] = [0; LEN]).unwrap(),
        rx: cortex_m::singleton!(: [u8; LEN] = [0; LEN]).unwrap(),
    }
}

/// Each motor lane's own copy of a frame. Call it once, from `init`.
pub fn motor_lane_memory() -> MotorLaneMemory {
    const LEN: usize = core::mem::size_of::<DshotDmaBuffer>() / 2;
    MotorLaneMemory {
        motor1: cortex_m::singleton!(: [u32; LEN] = [0; LEN]).unwrap(),
        motor2: cortex_m::singleton!(: [u32; LEN] = [0; LEN]).unwrap(),
        motor3: cortex_m::singleton!(: [u32; LEN] = [0; LEN]).unwrap(),
        motor4: cortex_m::singleton!(: [u32; LEN] = [0; LEN]).unwrap(),
    }
}
