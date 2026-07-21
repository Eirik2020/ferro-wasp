pub use hal::{
    adc::{
        Adc, Temperature,
        config::{AdcConfig, Dma, SampleTime, Scan, Sequence},
    },
    dma::{
        ChannelX, CurrentBuffer, DMAError, DmaFlag, MemoryToPeripheral, PeripheralToMemory,
        Stream0, Stream1, Stream2, Stream3, Stream4, Stream5, Stream6, Stream7, StreamsTuple,
        Transfer,
        config::DmaConfig,
        traits::{Channel, DMASet, Stream},
    },
    gpio::{
        Edge, ExtiPin, GpioExt, Input, Output, PA0, PA1, PA2, PA3, PA4, PA5, PA6, PA7, PA8, PA10,
        PB0, PB1, PB15, PC0, PC1, PC4, PC7, PC8, PC9, Pin, PushPull,
    },
    pac::{
        self, ADC1, DMA1, DMA2, SPI1, TIM1, TIM2, TIM3, TIM4, TIM6, TIM8, TIM12, UART4, USART1,
        USART2,
    },
    prelude::*,
    rcc::{Config as rcc_cfg, Rcc, RccExt},
    serial::{
        self, RxListen, Serial,
        config::{Parity, StopBits, WordLength},
    },
    signature::{VtempCal30, VtempCal110},
    spi::{self, Rx, Spi, Tx},
    timer::{CounterHz, PwmChannel, Timer},
};
pub use stm32f4xx_hal as hal;
