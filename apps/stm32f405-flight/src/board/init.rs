use ferrowasp_drivers::mpu6500 as imu;
use ferrowasp_stm32f4 as backend;
use ferrowasp_stm32f4::app_storage::{AdcStorageResources, SpiDmaStorageResources};
use ferrowasp_stm32f4::hal_prelude::*;

pub use backend::adc::Adc1BatteryResources;
pub use backend::dshot::{
    DSHOT_FRAME_TIMEOUT_MS, DSHOT_SERVICE_PERIOD_MS, DshotCommandError, DshotDmaBuffer,
    DshotDmaStorage, DshotInitError, DshotInterruptEvent, DshotMotor, DshotMotorBank,
    DshotServiceEvent, DshotTelemetryRequestError,
};
pub use backend::spi_dma::Spi1Mpu6500Resources;

pub struct Spi1Mpu6500Parts {
    pub owner: backend::spi_dma::Spi1Mpu6500Owner,
    pub parser: backend::spi_dma::SpiRxParserSide,
}

pub type Adc1BatteryParts = backend::adc::Adc1ObservationParts;

pub struct DebugLedResources {
    pub red: super::aliases::RedLedPin,
    pub green: super::aliases::GreenLedPin,
}

pub struct DebugLeds {
    pub red: super::aliases::RedLed,
    pub green: super::aliases::GreenLed,
}

pub fn init_debug_leds(resources: DebugLedResources) -> DebugLeds {
    DebugLeds {
        red: resources.red.into_push_pull_output(),
        green: resources.green.into_push_pull_output(),
    }
}

pub fn init_spi1_mpu6500<D>(
    resources: Spi1Mpu6500Resources,
    clocks: &mut Rcc,
    delay: &mut D,
    storage: SpiDmaStorageResources,
) -> Spi1Mpu6500Parts
where
    D: hal::hal::delay::DelayNs,
{
    let Spi1Mpu6500Resources {
        cs_pin,
        sck_pin,
        miso_pin,
        mosi_pin,
        spi,
        rx_dma,
        tx_dma,
    } = resources;

    let mut cs = backend::spi_dma::init_spi1_mpu6500_cs(cs_pin);
    let mut spi = backend::spi_dma::init_spi1_mpu6500_bus(spi, sck_pin, miso_pin, mosi_pin, clocks);

    imu::init(&mut spi, &mut cs, delay).unwrap();

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
    } = dma;
    let owner = backend::spi_dma::SpiDmaOwner::new(irq, poller, recovery_rx_buffer, cs);

    Spi1Mpu6500Parts { owner, parser }
}

pub fn init_adc1_battery(
    resources: Adc1BatteryResources,
    rcc: &mut Rcc,
    storage: AdcStorageResources,
) -> Adc1BatteryParts {
    let (buffer_1, buffer_2) = storage.split();

    backend::adc::init_adc1_observation(
        resources.adc,
        resources.voltage_pin,
        resources.current_pin,
        resources.dma,
        rcc,
        buffer_1,
        buffer_2,
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
    pub motor3_dma: Stream4<DMA2>,
    pub motor4_dma: Stream6<DMA2>,
}

pub fn init_dshot_motor_bank(
    resources: DshotMotorBankResources,
    clocks: &hal::rcc::Clocks,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank, DshotInitError> {
    DshotMotorBank::new(
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
