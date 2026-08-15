#[cfg(feature = "stm32f405")]
use crate::spi_dma;
use crate::uart_dma;

pub const UART_RX_BUFFER_COUNT: usize = 4;
#[cfg(feature = "stm32f405")]
pub const SPI_DMA_BUFFER_COUNT: usize = 6;
#[cfg(feature = "stm32f405")]
pub const ADC_BUFFER_COUNT: usize = 2;

pub type UartRxBufferBank = [[u8; uart_dma::UART_RX_BUFFER_SIZE]; UART_RX_BUFFER_COUNT];
pub type UartRxFreeQueue = uart_dma::FreeQueue;
pub type UartRxFilledQueue = uart_dma::FilledQueue;
#[cfg(feature = "stm32f405")]
pub type UartTxBuffer = [u8; uart_dma::UART_TX_BUFFER_SIZE];
#[cfg(feature = "stm32f405")]
pub type Uart4TxBuffer = UartTxBuffer;

#[cfg(feature = "stm32f405")]
pub type SpiDmaBufferBank = [[u8; spi_dma::SPI_BUFFER_SIZE]; SPI_DMA_BUFFER_COUNT];
#[cfg(feature = "stm32f405")]
pub type SpiFreeQueue = spi_dma::FreeQueue;
#[cfg(feature = "stm32f405")]
pub type SpiFilledQueue = spi_dma::FilledQueue;

#[cfg(feature = "stm32f405")]
pub type AdcBufferBank = [[u16; 3]; ADC_BUFFER_COUNT];

pub const fn new_uart_rx_buffer_bank() -> UartRxBufferBank {
    [[0; uart_dma::UART_RX_BUFFER_SIZE]; UART_RX_BUFFER_COUNT]
}

#[cfg(feature = "stm32f405")]
pub const fn new_spi_dma_buffer_bank() -> SpiDmaBufferBank {
    [[0; spi_dma::SPI_BUFFER_SIZE]; SPI_DMA_BUFFER_COUNT]
}

#[cfg(feature = "stm32f405")]
pub const fn new_adc_buffer_bank() -> AdcBufferBank {
    [[0; 3]; ADC_BUFFER_COUNT]
}

pub struct UartRxStorageResources {
    pub buffers: &'static mut UartRxBufferBank,
    pub free_queue: &'static mut UartRxFreeQueue,
    pub filled_queue: &'static mut UartRxFilledQueue,
}

impl UartRxStorageResources {
    pub fn into_backend(self) -> uart_dma::UartRxStorage {
        let [buffer1, buffer2, buffer3, buffer4] = self.buffers.each_mut();

        uart_dma::UartRxStorage {
            buffer1,
            buffer2,
            buffer3,
            buffer4,
            free_queue: self.free_queue,
            filled_queue: self.filled_queue,
        }
    }
}

#[cfg(feature = "stm32f405")]
pub struct SpiDmaStorageResources {
    pub buffers: &'static mut SpiDmaBufferBank,
    pub free_queue: &'static mut SpiFreeQueue,
    pub filled_queue: &'static mut SpiFilledQueue,
}

#[cfg(feature = "stm32f405")]
impl SpiDmaStorageResources {
    pub fn into_backend(self) -> spi_dma::SpiDmaStorage {
        let [
            rx_buffer1,
            rx_buffer2,
            rx_buffer3,
            rx_buffer4,
            recovery_rx_buffer,
            tx_buffer,
        ] = self.buffers.each_mut();

        spi_dma::SpiDmaStorage {
            rx_buffer1,
            rx_buffer2,
            rx_buffer3,
            _rx_buffer4: rx_buffer4,
            recovery_rx_buffer,
            tx_buffer,
            free_queue: self.free_queue,
            filled_queue: self.filled_queue,
        }
    }
}

#[cfg(feature = "stm32f405")]
pub struct AdcStorageResources {
    pub buffers: &'static mut AdcBufferBank,
}

#[cfg(feature = "stm32f405")]
impl AdcStorageResources {
    pub fn split(self) -> (&'static mut [u16; 3], &'static mut [u16; 3]) {
        let [primary, spare] = self.buffers.each_mut();
        (primary, spare)
    }
}
