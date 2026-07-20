use ferrowasp_io_core::serial::{
    DiscontinuityReader, SerialProtocol, SerialReader, SerialRxChannel, SerialRxProducer,
    SerialTxChannel, SerialTxCompletion, SerialTxOwner, SerialWriter,
};

pub const UART_RX_PORTS: usize = 2;
pub const UART_RX_BUFFERS_PER_PORT: usize = 4;
pub const UART_RX_BUFFER_BYTES: usize = SerialProtocol::max_frame_size();
pub const UART_RX_QUEUE_CAPACITY: usize = 4;
pub const OWNED_UART_RX_QUEUE_DEPTH: usize = 4;
pub const OWNED_UART_TX_QUEUE_DEPTH: usize = 16;
pub const SPI_RX_BUFFERS: usize = 5;
pub const SPI_RX_BUFFER_BYTES: usize = 15;
pub const SPI_RX_QUEUE_CAPACITY: usize = 4;
pub const SPI1_JOB_MAX_OPERATIONS: usize = 8;
pub const SPI1_JOB_MAX_BYTES: usize = 64;
pub const ADC_SAMPLE_WORDS: usize = 3;

pub type UartOwnedRxChannel = SerialRxChannel<UART_RX_BUFFER_BYTES, OWNED_UART_RX_QUEUE_DEPTH>;
pub type UartOwnedRxProducer<'a> =
    SerialRxProducer<'a, UART_RX_BUFFER_BYTES, OWNED_UART_RX_QUEUE_DEPTH>;
pub type UartOwnedReader<'a> = SerialReader<'a, UART_RX_BUFFER_BYTES, OWNED_UART_RX_QUEUE_DEPTH>;
pub type UartOwnedDiscontinuities<'a> =
    DiscontinuityReader<'a, UART_RX_BUFFER_BYTES, OWNED_UART_RX_QUEUE_DEPTH>;
pub type UartOwnedTxChannel = SerialTxChannel<UART_RX_BUFFER_BYTES, OWNED_UART_TX_QUEUE_DEPTH>;
pub type UartOwnedWriter<'a> = SerialWriter<'a, UART_RX_BUFFER_BYTES, OWNED_UART_TX_QUEUE_DEPTH>;
pub type UartOwnedTxOwner<'a> = SerialTxOwner<'a, UART_RX_BUFFER_BYTES, OWNED_UART_TX_QUEUE_DEPTH>;
pub type UartOwnedTxCompletion<'a> =
    SerialTxCompletion<'a, UART_RX_BUFFER_BYTES, OWNED_UART_TX_QUEUE_DEPTH>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticStoragePlan {
    pub uart_rx_ports: usize,
    pub uart_rx_buffers_per_port: usize,
    pub uart_rx_buffer_bytes: usize,
    pub uart_rx_queue_capacity: usize,
    pub owned_uart_rx_channels: usize,
    pub owned_uart_rx_channel_bytes: usize,
    pub owned_uart_tx_channel_bytes: usize,
    pub spi_rx_buffers: usize,
    pub spi_rx_buffer_bytes: usize,
    pub spi_rx_queue_capacity: usize,
    pub spi_mailbox_bytes: usize,
    pub adc_sample_words: usize,
}

pub const DEFAULT_STATIC_STORAGE: StaticStoragePlan = StaticStoragePlan {
    uart_rx_ports: UART_RX_PORTS,
    uart_rx_buffers_per_port: UART_RX_BUFFERS_PER_PORT,
    uart_rx_buffer_bytes: UART_RX_BUFFER_BYTES,
    uart_rx_queue_capacity: UART_RX_QUEUE_CAPACITY,
    owned_uart_rx_channels: UART_RX_PORTS,
    owned_uart_rx_channel_bytes: core::mem::size_of::<UartOwnedRxChannel>(),
    owned_uart_tx_channel_bytes: core::mem::size_of::<UartOwnedTxChannel>(),
    spi_rx_buffers: SPI_RX_BUFFERS,
    spi_rx_buffer_bytes: SPI_RX_BUFFER_BYTES,
    spi_rx_queue_capacity: SPI_RX_QUEUE_CAPACITY,
    spi_mailbox_bytes: core::mem::size_of::<
        ferrowasp_io_core::spi::SpiRequestMailbox<SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>,
    >(),
    adc_sample_words: ADC_SAMPLE_WORDS,
};

impl StaticStoragePlan {
    pub const fn uart_rx_bytes(self) -> usize {
        self.uart_rx_ports * self.uart_rx_buffers_per_port * self.uart_rx_buffer_bytes
    }

    pub const fn total_uart_rx_bytes(self) -> usize {
        self.uart_rx_bytes() + self.owned_uart_rx_channels * self.owned_uart_rx_channel_bytes
    }

    pub const fn total_uart_channel_bytes(self) -> usize {
        self.total_uart_rx_bytes() + self.owned_uart_tx_channel_bytes
    }

    pub const fn spi_rx_bytes(self) -> usize {
        self.spi_rx_buffers * self.spi_rx_buffer_bytes
    }

    pub const fn adc_bytes(self) -> usize {
        self.adc_sample_words * core::mem::size_of::<u16>() * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_storage_budget_matches_current_firmware_buffers() {
        assert_eq!(DEFAULT_STATIC_STORAGE.uart_rx_bytes(), 560);
        assert_eq!(
            DEFAULT_STATIC_STORAGE.owned_uart_rx_channel_bytes,
            core::mem::size_of::<UartOwnedRxChannel>()
        );
        assert_eq!(
            DEFAULT_STATIC_STORAGE.total_uart_rx_bytes(),
            560 + UART_RX_PORTS * core::mem::size_of::<UartOwnedRxChannel>()
        );
        assert_eq!(
            DEFAULT_STATIC_STORAGE.owned_uart_tx_channel_bytes,
            core::mem::size_of::<UartOwnedTxChannel>()
        );
        assert_eq!(DEFAULT_STATIC_STORAGE.spi_rx_bytes(), 75);
        assert_eq!(
            DEFAULT_STATIC_STORAGE.spi_mailbox_bytes,
            core::mem::size_of::<
                ferrowasp_io_core::spi::SpiRequestMailbox<
                    SPI1_JOB_MAX_OPERATIONS,
                    SPI1_JOB_MAX_BYTES,
                >,
            >()
        );
        assert_eq!(DEFAULT_STATIC_STORAGE.adc_bytes(), 12);
    }
}
