use ferrowasp_stm32f4::hal_prelude::hal::gpio::{PB12, PB13, PC2, PC3};
use ferrowasp_stm32f4::hal_prelude::hal::pac::{SPI2, TIM8};
use ferrowasp_stm32f4::{adc, hal_prelude::*, spi_dma};

pub type Usart2TxPin = PA2<Input>;
pub type Usart2RxPin = PA3<Input>;
pub type Uart4TxPin = PA0<Input>;
pub type Uart4RxPin = PA1<Input>;
pub type Usart1EscTelemetryRxPin = PA10<Input>;

pub type Spi1CsPin = PA4<Input>;
pub type Spi1SckPin = PA5<Input>;
pub type Spi1MisoPin = PA6<Input>;
pub type Spi1MosiPin = PA7<Input>;
pub type ImuDataReadyPin = PC4<Input>;

pub type Spi2FlashCsPin = PB12<Input>;
pub type Spi2FlashSckPin = PB13<Input>;
pub type Spi2FlashMisoPin = PC2<Input>;
pub type Spi2FlashMosiPin = PC3<Input>;
pub type Spi2FlashBus = Spi<SPI2>;
pub type Spi2FlashCs = PB12<Output<PushPull>>;
pub type Spi2Flash = ferrowasp_drivers::spi_nor::SpiNor<Spi2FlashBus, Spi2FlashCs>;
pub type Spi2FlashError = ferrowasp_drivers::spi_nor::Error<
    stm32f4xx_hal::spi::Error,
    core::convert::Infallible,
>;

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
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream5<DMA1>,
        serial::Rx<USART2>,
        4,
        PeripheralToMemory,
    >();
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream2<DMA1>,
        serial::Rx<UART4>,
        4,
        PeripheralToMemory,
    >();
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream4<DMA1>,
        serial::Tx<UART4>,
        4,
        MemoryToPeripheral,
    >();
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream0<DMA2>,
        spi::Rx<SPI1>,
        3,
        PeripheralToMemory,
    >();
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream3<DMA2>,
        spi::Tx<SPI1>,
        3,
        MemoryToPeripheral,
    >();
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream4<DMA2>,
        Adc<ADC1>,
        0,
        PeripheralToMemory,
    >();
    ferrowasp_stm32f4::board_routes::assert_timer_instance::<TIM1>();
    ferrowasp_stm32f4::board_routes::assert_timer_instance::<TIM8>();
}

pub fn assert_usart1_esc_telemetry_route_compile() {
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream5<DMA2>,
        serial::Rx<USART1>,
        4,
        PeripheralToMemory,
    >();
}

pub fn assert_four_motor_dshot_routes_compile() {
    ferrowasp_stm32f4::dshot::assert_foxeer_four_motor_dma_routes_compile();
}
