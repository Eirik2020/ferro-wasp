pub use hal::{
    dma::{
        DBTransfer, MemoryToPeripheral, PeripheralToMemory, Transfer,
        dma::{
            DmaConfig, Stream0, Stream1, Stream2, Stream3, Stream4, Stream5, Stream6, Stream7,
            StreamsTuple,
        },
    },
    gpio::{
        Alternate, Analog, Edge, ExtiPin, Input, Output, PA0, PA1, PA5, PA6, PA11, PA12, PB0, PB1,
        PB2, PC0, PC1, PC6, PC7, PC8, PC9, PC10, PC11, PC12, PC15, PD2, PD7, PD8, PD9, PE0,
        PushPull, Speed,
    },
    pac::{
        self, ADC1, DMA1, DMA2, SDMMC1, SPI1, TIM2, TIM3, TIM4, TIM5, TIM6, UART8, USART3, USART6,
    },
    prelude::*,
    rcc::{CoreClocks, rec},
};
pub use stm32h7xx_hal as hal;
