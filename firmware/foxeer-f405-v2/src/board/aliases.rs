use ferrowasp_stm32f4::hal_prelude::hal::gpio::{PB12, PB13, PC2, PC3, PC10, PC11};
use ferrowasp_stm32f4::hal_prelude::hal::pac::{SPI2, TIM8, USART3};
use ferrowasp_stm32f4::{adc, dshot, hal_prelude::*, spi_dma, uart_dma};

pub type Usart2TxPin = PA2<Input>;
pub type Usart2RxPin = PA3<Input>;
pub type Usart3TxPin = PC10<Input>;
pub type Usart3RxPin = PC11<Input>;
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
pub type Spi2FlashError =
    ferrowasp_drivers::spi_nor::Error<stm32f4xx_hal::spi::Error, core::convert::Infallible>;

pub type Motor1Pin = PA8<Input>;
pub type Motor2Pin = PC9<Input>;
pub type Motor3Pin = PC8<Input>;
pub type Motor4Pin = PB15<Input>;

pub type AdcVoltagePin = PC0<Input>;
pub type AdcCurrentPin = PC1<Input>;

pub type Spi1ImuCs = PA4<Output<PushPull>>;
pub type Spi1RxTransfer = spi_dma::SpiRxTransfer<Stream0<DMA2>, SPI1, 3>;
pub type Spi1TxTransfer = spi_dma::SpiTxTransfer<Stream3<DMA2>, SPI1, 3>;
pub type Spi1ImuOwner = spi_dma::SpiDmaOwner<Spi1RxTransfer, Spi1TxTransfer, Spi1ImuCs>;

/// UART1 (USART1) receive-only on PA10, RX on DMA2 Stream 5.
pub type Uart1RxIrq = uart_dma::UartRxIrq<Stream5<DMA2>, USART1, 4>;
/// UART2 (USART2) on PA2/PA3, RX on DMA1 Stream 5 and TX on Stream 6.
pub type Uart2RxIrq = uart_dma::UartRxIrq<Stream5<DMA1>, USART2, 4>;
pub type Uart2TxDmaSide = uart_dma::UartTxSide<Stream6<DMA1>, USART2, 4>;
/// UART3 (USART3) on PC10/PC11, RX on DMA1 Stream 1 and TX on Stream 3.
pub type Uart3RxIrq = uart_dma::UartRxIrq<Stream1<DMA1>, USART3, 4>;
pub type Uart3TxDmaSide = uart_dma::UartTxSide<Stream3<DMA1>, USART3, 4>;
/// UART4 on PA0/PA1, RX on DMA1 Stream 2 and TX on Stream 4.
pub type Uart4RxIrq = uart_dma::UartRxIrq<Stream2<DMA1>, UART4, 4>;
pub type Uart4TxDmaSide = uart_dma::UartTxSide<Stream4<DMA1>, UART4, 4>;

/// The four motor lanes: TIM1 CH1 on PA8, TIM8 CH4 on PC9, TIM8 CH3 on PC8,
/// and TIM1 CH3N on PB15, carried by DMA2 streams 1, 7, 2, and 6, each
/// naming its pad for bidirectional replies. The type compiles only if the
/// backend declares each lane's stream route, both ways.
pub type DshotMotorBank = dshot::DshotMotorBank<
    (dshot::DshotTimer<TIM1>, dshot::DshotTimer<TIM8>),
    dshot::Lane<Stream1<DMA2>, TIM1, dshot::channel::Ch1, 6, dshot::ReplyPin<dshot::GpioA, 8>>,
    dshot::Lane<Stream7<DMA2>, TIM8, dshot::channel::Ch4, 7, dshot::ReplyPin<dshot::GpioC, 9>>,
    dshot::Lane<Stream2<DMA2>, TIM8, dshot::channel::Ch3, 0, dshot::ReplyPin<dshot::GpioC, 8>>,
    dshot::Lane<Stream6<DMA2>, TIM1, dshot::channel::Ch3N, 6, dshot::ReplyPin<dshot::GpioB, 15>>,
>;

pub type Adc1ObservationTransfer = adc::Adc1ObservationTransferFor<Stream4<DMA2>, 0>;
pub type Adc1ObservationParts = adc::Adc1ObservationPartsFor<Stream4<DMA2>, 0>;

pub type ControlSchedulerTimer = TIM4;
pub type ControlScheduler = ferrowasp_stm32f4::timer_tick::PeriodicTimer<ControlSchedulerTimer>;
pub type IoTimebaseTimer = TIM2;
pub type IoWatchdogTimer = TIM6;
pub type IoWatchdog = ferrowasp_stm32f4::timer_tick::PeriodicTimer<IoWatchdogTimer>;

pub fn assert_active_routes_compile() {
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream5<DMA1>,
        serial::Rx<USART2>,
        4,
        PeripheralToMemory,
    >();
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream1<DMA1>,
        serial::Rx<USART3>,
        4,
        PeripheralToMemory,
    >();
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream3<DMA1>,
        serial::Tx<USART3>,
        4,
        MemoryToPeripheral,
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

pub fn assert_uart1_esc_telemetry_route_compile() {
    ferrowasp_stm32f4::board_routes::assert_dma_route::<
        Stream5<DMA2>,
        serial::Rx<USART1>,
        4,
        PeripheralToMemory,
    >();
}
