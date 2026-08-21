//! Typed physical declarations for a four-lane STM32F405 DShot bank.

use super::{dma_route::DmaRoute, pins::PinId};

/// One advanced-timer output supported by the reviewed four-lane backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DshotTimerChannel {
    /// TIM1 channel 1 main output.
    Tim1Ch1,
    /// TIM8 channel 4 main output.
    Tim8Ch4,
    /// TIM8 channel 3 main output.
    Tim8Ch3,
    /// TIM1 channel 3 complementary output.
    Tim1Ch3N,
}

impl DshotTimerChannel {
    /// Whether the physical pad uses the advanced timer's complementary output.
    pub const fn is_complementary(self) -> bool {
        matches!(self, Self::Tim1Ch3N)
    }

    /// GPIO alternate-function number required by this timer channel.
    pub const fn alternate_function(self) -> u8 {
        match self {
            Self::Tim1Ch1 | Self::Tim1Ch3N => 1,
            Self::Tim8Ch4 | Self::Tim8Ch3 => 3,
        }
    }
}

/// One physical DShot lane, before any airframe motor remapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DshotLaneHardwareDeclaration {
    /// One-based physical output number.
    pub physical_output: u8,
    /// One-based logical motor selected by the board map.
    pub logical_motor: u8,
    /// Advanced timer output driving the pad.
    pub timer_channel: DshotTimerChannel,
    /// Package pin carrying the signal.
    pub pin: PinId,
    /// Fixed memory-to-peripheral DMA request route.
    pub dma: DmaRoute,
}

impl DshotLaneHardwareDeclaration {
    /// Creates one fully specified physical DShot lane.
    pub const fn new(
        physical_output: u8,
        logical_motor: u8,
        timer_channel: DshotTimerChannel,
        pin: PinId,
        dma: DmaRoute,
    ) -> Self {
        Self {
            physical_output,
            logical_motor,
            timer_channel,
            pin,
            dma,
        }
    }
}

/// One indivisible four-lane DShot hardware containment unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DshotBankHardwareDeclaration {
    /// Stable board-local hardware identifier.
    pub id: &'static str,
    /// Physical lanes in output order 1 through 4.
    pub lanes: [DshotLaneHardwareDeclaration; 4],
    /// Whether TIM1 is the frame master and TIM8 is started through ITR0.
    pub tim1_frame_master_with_tim8_itr0: bool,
}

impl DshotBankHardwareDeclaration {
    /// Creates a physical four-lane bank declaration.
    pub const fn new(id: &'static str, lanes: [DshotLaneHardwareDeclaration; 4]) -> Self {
        Self {
            id,
            lanes,
            tim1_frame_master_with_tim8_itr0: true,
        }
    }
}
