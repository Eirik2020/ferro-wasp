//! Compile-time checks that a board's DMA and timer routes exist on the
//! STM32F4, beside the chip-neutral route descriptions.
//!
//! Requirements: HW-ROUTE-001, HW-ROUTE-002
//! Safety-critical.

pub use ferrowasp_stm32_critical::board_routes::*;

pub fn assert_dma_route<StreamT, PeripheralT, const CHANNEL: u8, Direction>()
where
    StreamT: stm32f4xx_hal::dma::traits::Stream,
    stm32f4xx_hal::dma::ChannelX<CHANNEL>: stm32f4xx_hal::dma::traits::Channel,
    PeripheralT: stm32f4xx_hal::dma::traits::DMASet<StreamT, CHANNEL, Direction>,
{
}

pub fn assert_timer_instance<TimerT>()
where
    TimerT: stm32f4xx_hal::timer::Instance,
{
}
