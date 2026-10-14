//! The concrete STM32H743 types this board's resources resolve to. Stream
//! numbers here are the DMA plan in `routes.rs`.

use ferrowasp_stm32::uart_port;
use ferrowasp_stm32h7::hal_prelude::*;
use ferrowasp_stm32h7::{adc, sd_storage, spi_dma, timers, uart_dma};

pub type ImuDataReadyPin = PB2<Input>;

pub type Spi1ImuOwner = spi_dma::Spi1DmaOwner<Stream4<DMA1>, Stream5<DMA1>, spi_dma::Spi1ImuCs>;

/// UART3 (USART3), RX on DMA1 Stream 1 and TX on DMA1 Stream 3.
pub type Uart3RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq<Stream1<DMA1>, USART3>>;
pub type Uart3TxDmaSide = uart_dma::UartTxSide<Stream3<DMA1>, USART3>;
/// UART6 (USART6), RX on DMA1 Stream 0 and TX on DMA1 Stream 6.
pub type Uart6RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq<Stream0<DMA1>, USART6>>;
pub type Uart6TxDmaSide = uart_dma::UartTxSide<Stream6<DMA1>, USART6>;
/// UART1 (USART1), the DJI connector's SBUS pin, RX on DMA2 Stream 4.
pub type Uart1RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq<Stream4<DMA2>, USART1>>;
/// UART8, RX on DMA1 Stream 2.
pub type Uart8RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq<Stream2<DMA1>, UART8>>;

pub type Adc1ObservationTransfer = adc::Adc1Observation;

pub type ControlScheduler = timers::PeriodicTimer<TIM4>;
pub type IoTimebase = timers::MicrosecondTimebase;
pub type IoWatchdog = timers::PeriodicTimer<TIM6>;

/// The microSD card, presented to the flash manager as SPI NOR.
pub type SdCardSlot = sd_storage::CardSlot<sd_storage::SdCardBlocks, sd_storage::SdCardInitError>;
pub type SdFlash = sd_storage::SdNor<SdCardSlot>;
pub type SdFlashError =
    sd_storage::Error<sd_storage::CardSlotError<hal::sdmmc::Error, sd_storage::SdCardInitError>>;
