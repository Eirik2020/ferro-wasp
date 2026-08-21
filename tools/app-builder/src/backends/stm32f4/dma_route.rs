//! STM32F4 DMA route identifiers.
//!
//! A DMA route is the controller, stream, and request channel selected for a
//! peripheral transfer. These types describe the route without owning a DMA
//! stream or configuring a transfer.

/// Identifies one STM32F4 DMA controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DmaController {
    /// DMA controller 1.
    Dma1,
    /// DMA controller 2.
    Dma2,
}

/// Identifies one stream within an STM32F4 DMA controller.
///
/// Each controller provides streams `0` through `7`. Whether a stream can
/// serve a particular peripheral is determined by the selected request
/// channel and must be validated by the STM32 backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DmaStream {
    /// DMA stream 0.
    Stream0,
    /// DMA stream 1.
    Stream1,
    /// DMA stream 2.
    Stream2,
    /// DMA stream 3.
    Stream3,
    /// DMA stream 4.
    Stream4,
    /// DMA stream 5.
    Stream5,
    /// DMA stream 6.
    Stream6,
    /// DMA stream 7.
    Stream7,
}

/// Identifies an STM32F4 DMA request channel selected by a stream.
///
/// A request channel connects a peripheral request line to a compatible DMA
/// stream. It is a hardware-routing property, not a software priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DmaChannel {
    /// DMA request channel 0.
    Channel0,
    /// DMA request channel 1.
    Channel1,
    /// DMA request channel 2.
    Channel2,
    /// DMA request channel 3.
    Channel3,
    /// DMA request channel 4.
    Channel4,
    /// DMA request channel 5.
    Channel5,
    /// DMA request channel 6.
    Channel6,
    /// DMA request channel 7.
    Channel7,
}

/// Identifies a concrete STM32F4 DMA route.
///
/// A route records the selected DMA hardware. It does not prove that the
/// route is valid for a specific peripheral or that it is not already claimed
/// by another declaration; those checks belong to the STM32 backend.
///
/// # Examples
///
/// ```
/// use xtask::backends::stm32f4::dma_route::{
///     DmaChannel, DmaController, DmaRoute, DmaStream,
/// };
///
/// let uart4_tx = DmaRoute::new(
///     DmaController::Dma1,
///     DmaStream::Stream4,
///     DmaChannel::Channel4,
/// );
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DmaRoute {
    /// DMA controller that owns [`Self::stream`].
    pub controller: DmaController,

    /// DMA stream allocated to the transfer.
    pub stream: DmaStream,

    /// Peripheral request channel selected for the stream.
    pub channel: DmaChannel,
}

impl DmaRoute {
    /// Creates an STM32F4 DMA route declaration.
    ///
    /// This constructor does not validate peripheral compatibility or check
    /// that the route is unique within a board declaration.
    pub const fn new(controller: DmaController, stream: DmaStream, channel: DmaChannel) -> Self {
        Self {
            controller,
            stream,
            channel,
        }
    }
}
