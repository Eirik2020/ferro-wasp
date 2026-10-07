//! The concrete STM32H743 types this board's resources resolve to. Stream
//! numbers here are the DMA plan in `routes.rs`.

use ferrowasp_stm32f4::uart_port;
use ferrowasp_stm32h7::hal_prelude::*;
use ferrowasp_stm32h7::{adc, sd_storage, spi_dma, timers, uart_dma};

pub type ImuDataReadyPin = PB2<Input>;

pub type Spi1ImuOwner = spi_dma::Spi1DmaOwner<Stream4<DMA1>, Stream5<DMA1>, spi_dma::Spi1ImuCs>;

/// The started UART ports, by the DMA streams `routes.rs` gives each.
pub type UartPorts =
    uart_dma::H743UartPorts<Stream1<DMA1>, Stream3<DMA1>, Stream0<DMA1>, Stream2<DMA1>>;
/// USART3, RX on DMA1 Stream 1 and TX on DMA1 Stream 3.
pub type Uart3RxPort =
    uart_port::UartRxPort<uart_dma::UartRxIrqSide<uart_dma::UartRxDma<Stream1<DMA1>, USART3>>>;
pub type Uart3TxDmaSide = uart_dma::UartTxDmaSide<uart_dma::UartTxDma<Stream3<DMA1>, USART3>>;
/// USART6, RX on DMA1 Stream 0.
pub type Uart6RxPort =
    uart_port::UartRxPort<uart_dma::UartRxIrqSide<uart_dma::UartRxDma<Stream0<DMA1>, USART6>>>;
/// UART8, RX on DMA1 Stream 2.
pub type Uart8RxPort =
    uart_port::UartRxPort<uart_dma::UartRxIrqSide<uart_dma::UartRxDma<Stream2<DMA1>, UART8>>>;

pub type Adc1ObservationTransfer = adc::Adc1Observation;

pub type ControlScheduler = timers::PeriodicTimer<TIM4>;
pub type IoTimebase = timers::MicrosecondTimebase;
pub type IoWatchdog = timers::PeriodicTimer<TIM6>;

/// The microSD card, presented to the flash manager as SPI NOR.
pub type SdCardSlot = sd_storage::CardSlot<sd_storage::SdCardBlocks, sd_storage::SdCardInitError>;
pub type SdFlash = sd_storage::SdNor<SdCardSlot>;
pub type SdFlashError =
    sd_storage::Error<sd_storage::CardSlotError<hal::sdmmc::Error, sd_storage::SdCardInitError>>;
