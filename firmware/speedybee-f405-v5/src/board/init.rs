//! Requirements: BUILD-001

use super::aliases::{Adc1ObservationTransfer, Spi1ImuOwner};
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
    chip::{ADC1, I2C1, SPI1, SPI3, TIM2, TIM3, TIM4, TIM5, TIM6, UART5, USART1, USART6},
    dma::Stream,
    exti::{self, Edge},
    gpio::{Alternate, Analog, Input, Pin},
    i2c::{Config as I2cConfig, I2c, Wiring as I2cWiring},
    rcc::{
        AhbPrescaler, ApbPrescaler, Clocks, Config as ClockConfig, Hse, HseMode, Pll, PllP,
        PllSource, Rcc, Supply, Sysclk,
    },
    spi::{self, BlockingSpi, BlockingWiring, Spi, Wiring as SpiWiring},
    time::Hertz,
    timer::{Awg, Pwm, Tick, Timebase},
    uart::{RxWiring, Wiring as UartWiring},
};
use ferrowasp_stm32::uart_dma::{self, PortMemory};
use ferrowasp_stm32::uart_port::{
    SerialPortEndpoint, UartRxPort, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
};
use spl06;

/// The board's 8 MHz crystal to 168 MHz, with 48 MHz on PLLQ for USB. The same
/// tree the old HAL worked out, stated, and checked against the F405's limits
/// as it builds.
pub const CLOCK_CONFIG: ClockConfig = ClockConfig {
    hse: Some(Hse {
        freq: Hertz::mhz(8),
        mode: HseMode::Oscillator,
    }),
    pll: Some(Pll {
        source: PllSource::Hse,
        prediv: 8,
        mul: 336,
        divp: PllP::Div2,
        divq: Some(7),
    }),
    sys: Sysclk::PllP,
    ahb_pre: AhbPrescaler::Div1,
    apb1_pre: ApbPrescaler::Div4,
    apb2_pre: ApbPrescaler::Div2,
    supply: Supply::V2_7To3_6,
};
pub const CLOCKS: Clocks = match CLOCK_CONFIG.validate() {
    Ok(clocks) => clocks,
    Err(_) => panic!("CLOCK_CONFIG breaks the F405's limits"),
};
const _: () = assert!(CLOCKS.sysclk.0 == backend::clocks::SYSTEM_CLOCK_HZ);
const _: () =
    assert!(Tick::check::<TIM4>(&CLOCKS, Hertz(super::profiles::SCHEDULER_TICK_RATE_HZ)).is_ok());
const _: () =
    assert!(Tick::check::<TIM6>(&CLOCKS, Hertz(ferrowasp_stm32::watchdog::IO_WATCHDOG_HZ)).is_ok());
// The microsecond timebase rides TIM5, the F405's other 32-bit counter: this
// board's motors 3 and 4 own TIM2.
const _: () = assert!(
    Timebase::check::<TIM5>(
        &CLOCKS,
        Hertz(ferrowasp_stm32::timebase::MICROSECOND_TIMEBASE_HZ)
    )
    .is_ok()
);

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
    DshotDmaStorage, DshotInitError, DshotInterruptEvent, DshotMotor, DshotServiceEvent,
    DshotTelemetryRequestError,
};

/// The IMU bus: the rate and mode the targets validated, in FerroHAL's terms.
const IMU_SPI: spi::Config = spi::Config::new(Hertz::mhz(1)).with_mode(spi::MODE_3);
/// The flash bus: FerroHAL takes the fastest rate at or below this, 10.5 MHz.
const FLASH_SPI: spi::Config = spi::Config::new(Hertz::mhz(11));

const _: () = assert!(Spi::check::<SPI1>(&CLOCKS, &IMU_SPI).is_ok());
const _: () = assert!(Spi::check::<SPI3>(&CLOCKS, &FLASH_SPI).is_ok());

pub struct Spi1ImuResources {
    pub cs_pin: super::aliases::Spi1ImuCs,
    pub sck_pin: Pin<'A', 5, Alternate<5>>,
    pub miso_pin: Pin<'A', 6, Alternate<5>>,
    pub mosi_pin: Pin<'A', 7, Alternate<5>>,
    pub spi: SPI1,
    pub rx_dma: Stream<2, 0>,
    pub tx_dma: Stream<2, 3>,
}

pub struct Spi1ImuParts {
    pub owner: Spi1ImuOwner,
    pub parser: backend::spi_dma::SpiRxParserSide,
    pub bringup: Spi1ImuBringupStatus,
}

pub struct Spi3FlashResources {
    pub cs_pin: super::aliases::Spi3FlashCs,
    pub sck_pin: Pin<'B', 3, Alternate<6>>,
    pub miso_pin: Pin<'B', 4, Alternate<6>>,
    pub mosi_pin: Pin<'B', 5, Alternate<6>>,
    pub spi: SPI3,
}

pub fn init_spi3_flash(
    resources: Spi3FlashResources,
    rcc: &mut Rcc,
) -> Option<super::aliases::Spi3Flash> {
    let mut cs = resources.cs_pin;
    cs.set_high();
    let bus = match BlockingSpi::new(
        rcc,
        BlockingWiring {
            spi: resources.spi,
            sck: resources.sck_pin,
            miso: resources.miso_pin,
            mosi: resources.mosi_pin,
        },
        &FLASH_SPI,
    ) {
        Ok(bus) => bus,
        Err(refused) => {
            defmt::warn!("SPI3 flash bus refused: {:?}", refused.error());
            return None;
        }
    };
    Some(ferrowasp_storage::spi_nor::SpiNor::new(bus, cs))
}

/// The IMU's data-ready line on PC4, which has to be an input.
pub fn init_imu_data_ready(
    _pin: Pin<'C', 4, Input>,
    line: exti::Line<4>,
    ctrl: &mut exti::Ctrl,
) -> super::aliases::ImuDataReadyPin {
    let mut routed = line.route::<'C'>(ctrl, Edge::Rising);
    routed.clear();
    routed.listen(ctrl);
    backend::exti::ExtiInput(routed.handle())
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

pub struct Adc1BatteryResourcesSpeedybee {
    pub adc: ADC1,
    pub voltage_pin: Pin<'C', 1, Analog>,
    pub current_pin: Pin<'C', 5, Analog>,
    pub dma: Stream<2, 4>,
}

/// The barometer bus, at 100 kHz for bench bring-up. The SPL06-007 carries
/// up to 400 kHz, but the bench asked at 400, 100 and 50 kHz - the last the
/// speed the F401 bench proved the master with only internal pull-ups -
/// and every ask read a clean NACK with nothing on the bus answering a
/// full-address sweep, so the rate reopens with a part that answers; 100
/// kHz is generous for any pull-ups until then. I2C1 times it from the
/// 42 MHz APB1 clock.
const BARO_I2C: I2cConfig = I2cConfig::new(Hertz::khz(100));
const _: () = if BARO_I2C.check(Hertz(CLOCKS.pclk1.0)).is_err() {
    panic!("I2C1 cannot time the barometer bus from the APB1 clock")
};

/// I2C1 for the onboard SPL06-007 barometer, on PB6/PB7.
pub struct I2c1BaroResources {
    pub scl_pin: Pin<'B', 6, Alternate<4>>,
    pub sda_pin: Pin<'B', 7, Alternate<4>>,
    pub i2c: I2C1,
}

/// Open I2C1 and probe the SPL06-007 at 0x76: its chip ID, its calibration
/// page and its oversampling, gathered into the probe the polling task
/// measures with. When no barometer answers the bus is handed over open,
/// so the task keeps probing and sweeps the bus; only a bus that refuses
/// to open reports absence.
pub fn init_i2c1_baro(
    resources: I2c1BaroResources,
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
            defmt::warn!("I2C1 barometer bus refused: {:?}", refused.error());
            return backend::tasks::baro::BaroHandover::Absent;
        }
    };
    match backend::tasks::baro::probe_spl06(i2c, spl06::ADDRESS) {
        Ok(probe) => backend::tasks::baro::BaroHandover::Probe(probe),
        Err(failed) => {
            defmt::warn!(
                "I2C1 barometer probe stopped at stage {} (chip ID {:#x})",
                failed.stage(),
                failed.chip_id()
            );
            backend::tasks::baro::BaroHandover::Open {
                bus: failed.release(),
                kind: backend::tasks::baro::PartKind::Spl06,
            }
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

/// The IMU driver's own memory, beside FerroWasp's rotating buffers.
pub struct Spi1Memory {
    pub core: backend::spi_dma::SharedSpi,
    pub tx: &'static mut [u8],
    pub rx: &'static mut [u8],
}

/// ADC1 reading the temperature sensor, then the battery's voltage and current.
pub fn init_adc1_battery(
    resources: Adc1BatteryResourcesSpeedybee,
    rcc: &mut Rcc,
    storage: AdcStorageResources,
) -> Option<(Adc1ObservationTransfer, &'static mut [u16; 3])> {
    let (primary, spare) = storage.split();
    let mut adc = Adc::new(rcc, resources.adc, &AdcConfig::slow_source()).ok()?;
    let vdda_mv = adc.vdda_millivolts().unwrap_or(3300);
    let max_sample = adc.max_sample();
    let channels = [
        adc.temperature_channel(),
        adc.channel(&resources.voltage_pin),
        adc.channel(&resources.current_pin),
    ];
    let scan = adc
        .into_scan(resources.dma, &channels, &mut primary[..])
        .ok()?;
    Some((
        Adc1ObservationTransfer::new(scan, 1, 2, vdda_mv, max_sample),
        spare,
    ))
}

pub struct DshotMotorBankResources {
    pub tim3: Pwm<TIM3>,
    pub tim2: Pwm<TIM2>,
    pub motor1_pin: Pin<'B', 1, Alternate<2>>,
    pub motor2_pin: Pin<'B', 0, Alternate<2>>,
    pub motor3_pin: Pin<'B', 10, Alternate<1>>,
    pub motor4_pin: Pin<'B', 11, Alternate<1>>,
    pub motor1_dma: Stream<1, 2>,
    pub motor2_dma: Stream<1, 7>,
    pub motor3_dma: Stream<1, 1>,
    pub motor4_dma: Stream<1, 6>,
}

/// Lane storage, for the generator's own copies of each frame.
pub struct MotorLaneMemory {
    pub motor1: &'static mut [u32],
    pub motor2: &'static mut [u32],
    pub motor3: &'static mut [u32],
    pub motor4: &'static mut [u32],
}

/// Build the four-motor bank on the waveform generator. Every channel comes up
/// at duty 0 with its timer stopped, so no pad sees a pulse, and TIM3 starts
/// TIM2 through its trigger output — the F405's TIM2 ITR2 route, proven on
/// hardware by FerroHAL's run R — so all four lanes begin on one edge.
pub fn init_dshot_motor_bank(
    resources: DshotMotorBankResources,
    memory: MotorLaneMemory,
    clocks: &Clocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<super::aliases::DshotMotorBank, DshotInitError> {
    let mut r = resources;
    let channel4 = r.tim3.channel::<4, _>(r.motor1_pin);
    let channel3 = r.tim3.channel::<3, _>(r.motor2_pin);
    let channel23 = r.tim2.channel::<3, _>(r.motor3_pin);
    let channel24 = r.tim2.channel::<4, _>(r.motor4_pin);
    let (Ok(lane1), Ok(lane2), Ok(lane3), Ok(lane4)) = (
        channel4.into_lane(r.motor1_dma, memory.motor1),
        channel3.into_lane(r.motor2_dma, memory.motor2),
        channel23.into_lane(r.motor3_dma, memory.motor3),
        channel24.into_lane(r.motor4_dma, memory.motor4),
    ) else {
        return Err(DshotInitError::LaneTimerMissing);
    };
    let awg = Awg::linked(r.tim3, r.tim2, (lane1, lane2), (lane3, lane4))
        .map_err(|_| DshotInitError::LaneClocksDiffer)?;
    // APB1 timers, both: twice the bus clock.
    backend::dshot::init_dshot_motor_bank(
        awg,
        super::aliases::MOTOR_LANES,
        clocks.pclk1.0 * 2,
        storage,
    )
}

/// Every UART this board routes, by logical port.
pub struct UartPortResources {
    pub uart1_tx_pin: Pin<'A', 9, Alternate<7>>,
    /// Often nothing is plugged into R1; a pull-up holds an open line idle
    /// instead of letting noise reach the command line.
    pub uart1_rx_pin: Pin<'A', 10, Alternate<7>>,
    pub uart1: USART1,
    pub uart1_rx_dma: Stream<2, 5>,
    pub uart1_tx_dma: Stream<2, 7>,
    pub uart5_rx_pin: Pin<'D', 2, Alternate<8>>,
    pub uart5: UART5,
    pub uart5_rx_dma: Stream<1, 0>,
    pub uart6_rx_pin: Pin<'C', 7, Alternate<8>>,
    pub uart6: USART6,
    pub uart6_rx_dma: Stream<2, 2>,
}

/// Static buffers and stream owners for each port.
pub struct UartPortStorage {
    pub uart1: UartRxTxPortStorage,
    pub uart5: UartRxPortStorage,
    pub uart6: UartRxPortStorage,
}

/// Each port's driver cell and receive ring.
pub struct UartPortMemory {
    pub uart1: PortMemory,
    pub uart5: PortMemory,
    pub uart6: PortMemory,
}

/// The started ports. A port left unbound stays `None`: its peripheral is
/// never enabled, so its interrupts never fire.
pub struct UartPorts {
    pub uart1_rx: Option<UartRxPort<super::aliases::Uart1RxIrq>>,
    pub uart1_tx: UartTxPort<super::aliases::Uart1TxDmaSide>,
    pub uart5_rx: Option<UartRxPort<super::aliases::Uart5RxIrq>>,
    pub uart6_rx: Option<UartRxPort<super::aliases::Uart6RxIrq>>,
    pub functions: SerialFunctionSlots<SerialPortEndpoint>,
}

/// Start every bound port with its function's line settings and give each
/// function its endpoint. The board inverts nothing: the O4 connector's RC
/// line is straight in, and a receiver that emits SBUS non-inverted binds
/// there.
pub fn init_uart_ports(
    resources: UartPortResources,
    rcc: &mut Rcc,
    storage: UartPortStorage,
    memory: UartPortMemory,
    bindings: &ResolvedBindings,
) -> UartPorts {
    let mut functions = SerialFunctionSlots::empty();
    let r = resources;
    let (uart1_rx, uart1_tx) = uart_dma::start_rx_tx_port(
        LogicalSerialPort::Uart1,
        UartWiring {
            uart: r.uart1,
            tx: r.uart1_tx_pin,
            rx: r.uart1_rx_pin,
            tx_dma: r.uart1_tx_dma,
            rx_dma: r.uart1_rx_dma,
        },
        rcc,
        storage.uart1,
        memory.uart1,
        false,
        bindings,
        &mut functions,
    );
    let uart5_rx = uart_dma::start_rx_port(
        LogicalSerialPort::Uart5,
        RxWiring {
            uart: r.uart5,
            rx: r.uart5_rx_pin,
            rx_dma: r.uart5_rx_dma,
        },
        rcc,
        storage.uart5,
        memory.uart5,
        false,
        bindings,
        &mut functions,
    );
    let uart6_rx = uart_dma::start_rx_port(
        LogicalSerialPort::Uart6,
        RxWiring {
            uart: r.uart6,
            rx: r.uart6_rx_pin,
            rx_dma: r.uart6_rx_dma,
        },
        rcc,
        storage.uart6,
        memory.uart6,
        false,
        bindings,
        &mut functions,
    );
    UartPorts {
        uart1_rx,
        uart1_tx,
        uart5_rx,
        uart6_rx,
        functions,
    }
}

static UART1_CORE: Mutex<RefCell<Option<uart_dma::UartCore>>> = Mutex::new(RefCell::new(None));
static UART5_CORE: Mutex<RefCell<Option<uart_dma::UartCore>>> = Mutex::new(RefCell::new(None));
static UART6_CORE: Mutex<RefCell<Option<uart_dma::UartCore>>> = Mutex::new(RefCell::new(None));
static SPI1_CORE: Mutex<RefCell<Option<backend::spi_dma::SpiCore>>> =
    Mutex::new(RefCell::new(None));

/// Each port's driver cell and receive ring. Call it once, from `init`.
pub fn uart_port_memory() -> UartPortMemory {
    const RING: usize = uart_dma::RX_RING_BYTES;
    UartPortMemory {
        uart1: PortMemory {
            core: &UART1_CORE,
            ring: cortex_m::singleton!(: [u8; RING] = [0; RING]).unwrap(),
        },
        uart5: PortMemory {
            core: &UART5_CORE,
            ring: cortex_m::singleton!(: [u8; RING] = [0; RING]).unwrap(),
        },
        uart6: PortMemory {
            core: &UART6_CORE,
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
