use ferrowasp_stm32f4::hal_prelude::hal::pac::TIM8;
use ferrowasp_stm32f4::{adc, hal_prelude::*, spi_dma};

pub type Usart2TxPin = PA2<Input>;
pub type Usart2RxPin = PA3<Input>;
pub type Uart4TxPin = PA0<Input>;
pub type Uart4RxPin = PA1<Input>;

pub type Spi1CsPin = PA4<Input>;
pub type Spi1SckPin = PA5<Input>;
pub type Spi1MisoPin = PA6<Input>;
pub type Spi1MosiPin = PA7<Input>;

pub type Motor1Pin = PA8<Input>;
pub type Motor2Pin = PC9<Input>;
pub type Motor3Pin = PC8<Input>;
pub type Motor4Pin = PB15<Input>;

pub type AdcVoltagePin = PC0<Input>;
pub type AdcCurrentPin = PC1<Input>;

pub type Spi1RxTransfer = spi_dma::SpiRxTransfer<Stream0<DMA2>, SPI1, 3>;
pub type Spi1TxTransfer = spi_dma::SpiTxTransfer<Stream3<DMA2>, SPI1, 3>;
pub type Spi1ImuOwner = spi_dma::SpiDmaOwner<Spi1RxTransfer, Spi1TxTransfer, spi_dma::Spi1ImuCs>;

pub type Adc1ObservationTransfer = adc::Adc1ObservationTransferFor<Stream4<DMA2>, 0>;
pub type Adc1ObservationParts = adc::Adc1ObservationPartsFor<Stream4<DMA2>, 0>;

pub type ControlSchedulerTimer = TIM4;
pub type ControlScheduler = CounterHz<ControlSchedulerTimer>;
pub type IoTimebaseTimer = TIM2;
pub type IoWatchdogTimer = TIM6;
pub type IoWatchdog = CounterHz<IoWatchdogTimer>;

pub fn assert_active_routes_compile() {
    assert_dma_route::<Stream5<DMA1>, serial::Rx<USART2>, 4, PeripheralToMemory>();
    assert_dma_route::<Stream2<DMA1>, serial::Rx<UART4>, 4, PeripheralToMemory>();
    assert_dma_route::<Stream4<DMA1>, serial::Tx<UART4>, 4, MemoryToPeripheral>();
    assert_dma_route::<Stream0<DMA2>, spi::Rx<SPI1>, 3, PeripheralToMemory>();
    assert_dma_route::<Stream3<DMA2>, spi::Tx<SPI1>, 3, MemoryToPeripheral>();
    assert_dma_route::<Stream4<DMA2>, Adc<ADC1>, 0, PeripheralToMemory>();
    assert_timer_instance::<TIM1>();
    assert_timer_instance::<TIM8>();
}

fn assert_dma_route<StreamT, PeripheralT, const CHANNEL: u8, Direction>()
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    PeripheralT: DMASet<StreamT, CHANNEL, Direction>,
{
}

fn assert_timer_instance<TimerT: hal::timer::Instance>() {}
