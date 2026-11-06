//! Requirements: BUILD-001

use ferrowasp_ferrohal::hal::{
    chip::{TIM1, TIM8},
    gpio::{Output, Pin},
    spi::BlockingSpi,
    timer::CompareLane,
};
use ferrowasp_ferrohal::{adc, dshot, spi_dma, uart_dma};

pub type Spi2FlashBus = BlockingSpi;
pub type Spi2FlashCs = Pin<'B', 12, Output>;
pub type Spi2Flash = ferrowasp_storage::spi_nor::SpiNor<Spi2FlashBus, Spi2FlashCs>;
pub type Spi2FlashError = ferrowasp_storage::spi_nor::Error<
    ferrowasp_ferrohal::hal::spi::Error,
    core::convert::Infallible,
>;

/// The IMU data-ready line, PC4, as an interrupt input.
pub type ImuDataReadyPin = ferrowasp_ferrohal::exti::ExtiInput;

pub type Spi1ImuCs = Pin<'A', 4, Output>;
pub type Spi1ImuOwner = spi_dma::SpiDmaOwner<Spi1ImuCs>;

/// UART1 (USART1) receive-only on PA10, RX on DMA2 Stream 5.
pub type Uart1RxIrq = uart_dma::UartRxIrq;
/// UART2 (USART2) on PA2/PA3, RX on DMA1 Stream 5 and TX on Stream 6.
pub type Uart2RxIrq = uart_dma::UartRxIrq;
pub type Uart2TxDmaSide = uart_dma::UartTxSide;
/// UART3 (USART3) on PC10/PC11, RX on DMA1 Stream 1 and TX on Stream 3.
pub type Uart3RxIrq = uart_dma::UartRxIrq;
pub type Uart3TxDmaSide = uart_dma::UartTxSide;
/// UART4 on PA0/PA1, RX on DMA1 Stream 2 and TX on Stream 4.
pub type Uart4RxIrq = uart_dma::UartRxIrq;
pub type Uart4TxDmaSide = uart_dma::UartTxSide;

/// The four motor lanes, on the waveform generator: TIM1 CH1 on PA8 and CH3N on
/// PB15, TIM8 CH4 on PC9 and CH3 on PC8, carried by DMA2 streams 1, 6, 7 and 2.
/// FerroHAL's routing traits refuse a stream that can't serve its channel.
pub type DshotMotorBank = dshot::DshotMotorBank<
    TIM1,
    TIM8,
    (CompareLane<TIM1, 1>, CompareLane<TIM1, 3>),
    (CompareLane<TIM8, 4>, CompareLane<TIM8, 3>),
>;

/// Each motor's lane: motor 1 is PA8, 2 is PC9, 3 is PC8 and 4 is PB15.
pub const MOTOR_LANES: [usize; 4] = [0, 2, 3, 1];

pub type Adc1ObservationTransfer =
    adc::Adc1ObservationTransfer<ferrowasp_ferrohal::hal::chip::ADC1>;

pub type ControlScheduler = ferrowasp_ferrohal::chip_time::PeriodicTimer;
pub type IoWatchdog = ferrowasp_ferrohal::chip_time::PeriodicTimer;
