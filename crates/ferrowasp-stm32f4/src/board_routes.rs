#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DmaDirection {
    PeripheralToMemory,
    MemoryToPeripheral,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaRoute {
    pub controller: u8,
    pub stream: u8,
    pub channel: u8,
    pub direction: DmaDirection,
    pub owner: &'static str,
}

impl DmaRoute {
    pub fn matches_manifest_claim(self, claim: &str) -> bool {
        let Some(claim) = claim.strip_prefix("DMA") else {
            return false;
        };
        let Some((controller, claim)) = claim.split_once("_STREAM") else {
            return false;
        };
        let (stream, channel) = match claim.split_once("_CH") {
            Some((stream, channel)) => (stream, Some(channel)),
            None => (claim, None),
        };

        parse_u8(controller) == Some(self.controller)
            && parse_u8(stream) == Some(self.stream)
            && match channel {
                Some(channel) => parse_u8(channel) == Some(self.channel),
                None => self.channel == 0,
            }
    }
}

fn parse_u8(value: &str) -> Option<u8> {
    value.parse().ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiRoute {
    pub peripheral: &'static str,
    pub sck_pin: &'static str,
    pub miso_pin: &'static str,
    pub mosi_pin: &'static str,
    pub cs_pin: &'static str,
    pub mode: u8,
    pub rx_dma: &'static str,
    pub tx_dma: &'static str,
    pub device: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmOutputKind {
    Main,
    Complementary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticPwmRoute {
    pub motor: u8,
    pub pin: &'static str,
    pub timer_channel: &'static str,
    pub output_kind: PwmOutputKind,
    pub logical_lane: &'static str,
}

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub fn assert_dma_route<StreamT, PeripheralT, const CHANNEL: u8, Direction>()
where
    StreamT: stm32f4xx_hal::dma::traits::Stream,
    stm32f4xx_hal::dma::ChannelX<CHANNEL>: stm32f4xx_hal::dma::traits::Channel,
    PeripheralT: stm32f4xx_hal::dma::traits::DMASet<StreamT, CHANNEL, Direction>,
{
}

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub fn assert_timer_instance<TimerT>()
where
    TimerT: stm32f4xx_hal::timer::Instance,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: DmaRoute = DmaRoute {
        controller: 2,
        stream: 5,
        channel: 4,
        direction: DmaDirection::PeripheralToMemory,
        owner: "test",
    };

    #[test]
    fn dma_route_matches_full_manifest_claim() {
        assert!(ROUTE.matches_manifest_claim("DMA2_STREAM5_CH4"));
        assert!(!ROUTE.matches_manifest_claim("DMA1_STREAM5_CH4"));
        assert!(!ROUTE.matches_manifest_claim("DMA2_STREAM4_CH4"));
        assert!(!ROUTE.matches_manifest_claim("DMA2_STREAM5_CH3"));
        assert!(!ROUTE.matches_manifest_claim("DMA2_STREAM5"));
        assert!(!ROUTE.matches_manifest_claim("not-a-route"));
    }

    #[test]
    fn channel_zero_route_accepts_legacy_claim_without_channel_suffix() {
        let route = DmaRoute {
            channel: 0,
            ..ROUTE
        };

        assert!(route.matches_manifest_claim("DMA2_STREAM5"));
        assert!(route.matches_manifest_claim("DMA2_STREAM5_CH0"));
    }
}
