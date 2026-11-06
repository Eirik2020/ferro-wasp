//! The concrete types this board's resources resolve to. Stream numbers here are
//! the DMA plan in `routes.rs`.

use ferrowasp_ferrohal::hal::{
    chip::{ADC1, TIM3, TIM5},
    gpio::{Output, Pin},
    timer::CompareLane,
};
use ferrowasp_ferrohal::{adc, dshot, sd_storage, spi_dma, uart_dma};
use ferrowasp_stm32::uart_port;

/// The IMU data-ready line, PB2, as an interrupt input.
pub type ImuDataReadyPin = ferrowasp_ferrohal::exti::ExtiInput;

/// The IMU chip select, PC15.
pub type Spi1ImuCs = Pin<'C', 15, Output>;
pub type Spi1ImuOwner = spi_dma::SpiDmaOwner<Spi1ImuCs>;

/// UART3 (USART3), RX on DMA1 Stream 1 and TX on DMA1 Stream 3.
pub type Uart3RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq>;
pub type Uart3TxDmaSide = uart_dma::UartTxSide;
/// UART6 (USART6), RX on DMA1 Stream 0 and TX on DMA1 Stream 6.
pub type Uart6RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq>;
pub type Uart6TxDmaSide = uart_dma::UartTxSide;
/// UART1 (USART1), the DJI connector's SBUS pin, RX on DMA2 Stream 4.
pub type Uart1RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq>;
/// UART8, RX on DMA1 Stream 2.
pub type Uart8RxPort = uart_port::UartRxPort<uart_dma::UartRxIrq>;

/// The four motor lanes, on the waveform generator: TIM3 CH3/CH4 on PB0/PB1 and TIM5
/// CH1/CH2 on PA0/PA1, carried by DMA2 streams 0-3.
pub type DshotMotorBank = dshot::DshotMotorBank<
    TIM3,
    TIM5,
    (CompareLane<TIM3, 3>, CompareLane<TIM3, 4>),
    (CompareLane<TIM5, 1>, CompareLane<TIM5, 2>),
>;

/// Each motor's lane: motors 1 to 4 in order.
pub const MOTOR_LANES: [usize; 4] = [0, 1, 2, 3];

/// Battery voltage on PC0 and current on PC1.
pub type Adc1ObservationTransfer = adc::Adc1ObservationTransfer<ADC1>;

pub type ControlScheduler = ferrowasp_ferrohal::chip_time::PeriodicTimer;
pub type IoTimebase = ferrowasp_ferrohal::chip_time::MicrosecondTimebase;
pub type IoWatchdog = ferrowasp_ferrohal::chip_time::PeriodicTimer;

/// The microSD card, presented to the flash manager as SPI NOR.
pub type SdCardSlot = sd_storage::CardSlot<sd_storage::SdCardBlocks, sd_storage::SdCardInitError>;
pub type SdFlash = sd_storage::SdNor<SdCardSlot>;
pub type SdFlashError = sd_storage::Error<
    sd_storage::CardSlotError<ferrowasp_ferrohal::hal::sdmmc::Error, sd_storage::SdCardInitError>,
>;
