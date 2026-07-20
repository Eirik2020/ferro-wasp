use ferrowasp_stm32f4::hal_prelude::*;

pub type Usart2TxPin = PA2<Input>;
pub type Usart2RxPin = PA3<Input>;
pub type Usart1EscTelemetryRxPin = PA10<Input>;
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

pub type RedLedPin = PB0<Input>;
pub type GreenLedPin = PB1<Input>;
pub type RedLed = PB0<Output<PushPull>>;
pub type GreenLed = PB1<Output<PushPull>>;

pub type Motor1PwmChannel = PwmChannel<TIM1, 0>;
pub type Motor2PwmChannel = PwmChannel<TIM3, 3>;
pub type Motor3PwmChannel = PwmChannel<TIM3, 2>;
pub type Motor4PwmChannel = PwmChannel<TIM12, 1>;
pub type ControlSchedulerTimer = TIM4;
pub type ControlScheduler = CounterHz<ControlSchedulerTimer>;
pub type IoTimebaseTimer = TIM2;
pub type IoWatchdogTimer = TIM6;
pub type IoWatchdog = CounterHz<IoWatchdogTimer>;

pub type Adc1ObservationTransfer =
    Transfer<Stream0<DMA2>, 0, Adc<ADC1>, PeripheralToMemory, &'static mut [u16; 3]>;

pub fn assert_active_routes_compile() {
    assert_dma_route::<Stream5<DMA2>, serial::Rx<USART1>, 4, PeripheralToMemory>();
    assert_dma_route::<Stream5<DMA1>, serial::Rx<USART2>, 4, PeripheralToMemory>();
    assert_dma_route::<Stream2<DMA1>, serial::Rx<UART4>, 4, PeripheralToMemory>();
    assert_dma_route::<Stream4<DMA1>, serial::Tx<UART4>, 4, MemoryToPeripheral>();
    assert_dma_route::<Stream2<DMA2>, spi::Rx<SPI1>, 3, PeripheralToMemory>();
    assert_dma_route::<Stream3<DMA2>, spi::Tx<SPI1>, 3, MemoryToPeripheral>();
    assert_dma_route::<Stream0<DMA2>, Adc<ADC1>, 0, PeripheralToMemory>();
    assert_static_pwm_channel::<Motor1PwmChannel>();
    assert_static_pwm_channel::<Motor2PwmChannel>();
    assert_static_pwm_channel::<Motor3PwmChannel>();
    assert_static_pwm_channel::<Motor4PwmChannel>();
}

#[cfg(feature = "ferrowasp-fcu3-dshot-four")]
pub fn assert_four_motor_dshot_routes_compile() {
    ferrowasp_stm32f4::dshot::assert_four_motor_dma_routes_compile();
}

fn assert_dma_route<StreamT, PeripheralT, const CHANNEL: u8, Direction>()
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    PeripheralT: DMASet<StreamT, CHANNEL, Direction>,
{
}

fn assert_static_pwm_channel<ChannelT>() {}
