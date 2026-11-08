//! Requirements: BUILD-001

use ferrowasp_stm32::hal::{
    chip::{TIM2, TIM3},
    gpio::{Output, Pin},
    spi::BlockingSpi,
    timer::CompareLane,
};
use ferrowasp_stm32::{adc, dshot, spi_dma, uart_dma};

pub type Spi3FlashBus = BlockingSpi;
pub type Spi3FlashCs = Pin<'C', 12, Output>;
pub type Spi3Flash = ferrowasp_storage::spi_nor::SpiNor<Spi3FlashBus, Spi3FlashCs>;
pub type Spi3FlashError =
    ferrowasp_storage::spi_nor::Error<ferrowasp_stm32::hal::spi::Error, core::convert::Infallible>;

/// The IMU data-ready line, PC4, as an interrupt input.
pub type ImuDataReadyPin = ferrowasp_stm32::exti::ExtiInput;

pub type Spi1ImuCs = Pin<'A', 4, Output>;
pub type Spi1ImuOwner = spi_dma::SpiDmaOwner<Spi1ImuCs>;

/// UART1 (USART1) on PA9/PA10, RX on DMA2 Stream 5 and TX on Stream 7.
pub type Uart1RxIrq = uart_dma::UartRxIrq;
pub type Uart1TxDmaSide = uart_dma::UartTxSide;
/// UART5 receive-only on PD2, RX on DMA1 Stream 0.
pub type Uart5RxIrq = uart_dma::UartRxIrq;
/// UART6 (USART6) receive-only on PC7, the DJI O4 connector's RC line, RX on
/// DMA2 Stream 2.
pub type Uart6RxIrq = uart_dma::UartRxIrq;

/// The four motor lanes on two timers, TIM3 leading TIM2 from one start:
/// TIM3 CH4 on PB1 and CH3 on PB0, TIM2 CH3 on PB10 and CH4 on PB11, every
/// stream on DMA1. FerroHAL's routing traits refuse a stream that can't serve
/// its channel.
pub type DshotMotorBank = dshot::DshotMotorBank<
    TIM3,
    TIM2,
    (CompareLane<TIM3, 4>, CompareLane<TIM3, 3>),
    (CompareLane<TIM2, 3>, CompareLane<TIM2, 4>),
>;

/// Each motor's lane in the bank's tuple order: motor 1 is TIM3 CH4, 2 is
/// TIM3 CH3, 3 is TIM2 CH3 and 4 is TIM2 CH4.
pub const MOTOR_LANES: [usize; 4] = [0, 1, 2, 3];

pub type Adc1ObservationTransfer = adc::Adc1ObservationTransfer<ferrowasp_stm32::hal::chip::ADC1>;

pub type ControlScheduler = ferrowasp_stm32::chip_time::PeriodicTimer;
pub type IoWatchdog = ferrowasp_stm32::chip_time::PeriodicTimer;
