//! The concrete STM32H743 types this board's resources resolve to. Stream
//! numbers here are the DMA plan in `routes.rs`.

use ferrowasp_stm32h7::hal_prelude::*;
use ferrowasp_stm32h7::{adc, sd_storage, spi_dma, timers, uart_dma};

pub type ImuDataReadyPin = PB2<Input>;

pub type Spi1ImuOwner = spi_dma::Spi1DmaOwner<Stream4<DMA1>, Stream5<DMA1>, spi_dma::Spi1ImuCs>;

/// Logical UART1: BLHeli ESC telemetry on UART8.
pub type Uart1RxIrq = uart_dma::UartRxIrqSide<uart_dma::UartRxDma<Stream2<DMA1>, UART8>>;
/// Logical UART2: SBUS on USART6.
pub type Uart2RxIrq = uart_dma::UartRxIrqSide<uart_dma::UartRxDma<Stream0<DMA1>, USART6>>;
/// Logical UART4: MSP DisplayPort on USART3.
pub type Uart4RxIrq = uart_dma::UartRxIrqSide<uart_dma::UartRxDma<Stream1<DMA1>, USART3>>;
pub type Uart4TxDmaSide = uart_dma::UartTxDmaSide<uart_dma::UartTxDma<Stream3<DMA1>, USART3>>;

pub type Adc1ObservationTransfer = adc::Adc1Observation;

pub type ControlScheduler = timers::PeriodicTimer<TIM4>;
pub type IoTimebase = timers::MicrosecondTimebase;
pub type IoWatchdog = timers::PeriodicTimer<TIM6>;

/// The microSD card, presented to the flash manager as SPI NOR.
pub type SdCardSlot = sd_storage::CardSlot<sd_storage::SdCardBlocks, sd_storage::SdCardInitError>;
pub type SdFlash = sd_storage::SdNor<SdCardSlot>;
pub type SdFlashError =
    sd_storage::Error<sd_storage::CardSlotError<hal::sdmmc::Error, sd_storage::SdCardInitError>>;
