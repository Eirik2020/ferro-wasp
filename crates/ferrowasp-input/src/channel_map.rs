//! Which receiver channel carries each stick and the arm switch: Command
//! Input's mapping from a receiver frame to the pilot's controls.
//!
//! Requirements: INPUT-006, INPUT-007, INPUT-013, INPUT-021
//! Safety-critical.

/// Lowest one-based channel the arm switch may use: channels 1-4 are sticks.
pub const RC_ARM_CHANNEL_MIN: u8 = 5;
/// Highest one-based channel SBUS and CRSF carry.
pub const RC_ARM_CHANNEL_MAX: u8 = 16;

/// Which receiver channel carries each stick and the arm switch.
///
/// Configuration names channels one-based, as radios do;
/// the stick order is one number whose digits are the channels for roll,
/// pitch, throttle and yaw, so AETR is 1234 and TAER is 2314. The order is
/// one value so a swap is a single change, never a transient duplicate.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RcChannelMap {
    /// Zero-based channels for roll, pitch, throttle and yaw.
    sticks: [u8; 4],
    /// Zero-based arm channel, never one of the stick channels.
    arm: u8,
}

/// Stick channels read from one receiver frame.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RcSticks {
    pub roll: u16,
    pub pitch: u16,
    pub throttle: u16,
    pub yaw: u16,
}

impl RcChannelMap {
    /// AETR with the arm switch on channel 9: the fixed mapping firmware
    /// used before the map was configurable, and every board's default. A
    /// configuration saved before then keeps it, so an update never moves a
    /// pilot's arm switch.
    pub const AETR_ARM_CH9: Self = Self {
        sticks: [0, 1, 2, 3],
        arm: 8,
    };

    /// AETR with the arm switch on channel 5, as EdgeTX and ExpressLRS radios
    /// send out of the box.
    pub const AETR_ARM_CH5: Self = Self {
        sticks: [0, 1, 2, 3],
        arm: 4,
    };

    /// Records `current` as the map in use, and says whether it replaced a
    /// different one. The first map seen is not a change.
    pub fn replaced(in_use: &mut Option<Self>, current: Self) -> bool {
        in_use
            .replace(current)
            .is_some_and(|previous| previous != current)
    }

    /// Build a map from its configuration values, or `None` when the stick
    /// order is not a permutation of channels 1-4 or the arm channel is
    /// outside 5-16.
    pub const fn from_config(stick_order: u16, arm_channel: u8) -> Option<Self> {
        if stick_order < 1000 || stick_order > 9999 {
            return None;
        }
        if arm_channel < RC_ARM_CHANNEL_MIN || arm_channel > RC_ARM_CHANNEL_MAX {
            return None;
        }
        let digits = [
            (stick_order / 1000) % 10,
            (stick_order / 100) % 10,
            (stick_order / 10) % 10,
            stick_order % 10,
        ];
        let mut sticks = [0_u8; 4];
        let mut seen = [false; 4];
        let mut index = 0;
        while index < 4 {
            let digit = digits[index];
            if digit < 1 || digit > 4 || seen[(digit - 1) as usize] {
                return None;
            }
            seen[(digit - 1) as usize] = true;
            sticks[index] = (digit - 1) as u8;
            index += 1;
        }
        Some(Self {
            sticks,
            arm: arm_channel - 1,
        })
    }

    /// The one-based stick order, digits for roll, pitch, throttle and yaw.
    pub const fn stick_order(self) -> u16 {
        (self.sticks[0] as u16 + 1) * 1000
            + (self.sticks[1] as u16 + 1) * 100
            + (self.sticks[2] as u16 + 1) * 10
            + (self.sticks[3] as u16 + 1)
    }

    /// The one-based arm channel.
    pub const fn arm_channel(self) -> u8 {
        self.arm + 1
    }

    pub const fn sticks(self, channels: &[u16; 16]) -> RcSticks {
        RcSticks {
            roll: channels[self.sticks[0] as usize],
            pitch: channels[self.sticks[1] as usize],
            throttle: channels[self.sticks[2] as usize],
            yaw: channels[self.sticks[3] as usize],
        }
    }

    pub const fn arm(self, channels: &[u16; 16]) -> u16 {
        channels[self.arm as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_edgetx_map_reads_aetr_with_the_arm_switch_on_channel_five() {
        let mut channels = [0_u16; 16];
        for (index, channel) in channels.iter_mut().enumerate() {
            *channel = index as u16 * 100;
        }
        let map = RcChannelMap::AETR_ARM_CH5;
        assert_eq!(map.stick_order(), 1234);
        assert_eq!(map.arm_channel(), 5);
        assert_eq!(
            map.sticks(&channels),
            RcSticks {
                roll: 0,
                pitch: 100,
                throttle: 200,
                yaw: 300,
            }
        );
        assert_eq!(map.arm(&channels), 400);
        assert_eq!(RcChannelMap::AETR_ARM_CH9.arm(&channels), 800);
    }

    #[test]
    fn a_channel_map_change_is_seen_once_and_the_first_map_is_not_one() {
        let mut in_use = None;
        assert!(!RcChannelMap::replaced(
            &mut in_use,
            RcChannelMap::AETR_ARM_CH5
        ));
        assert!(!RcChannelMap::replaced(
            &mut in_use,
            RcChannelMap::AETR_ARM_CH5
        ));
        assert!(RcChannelMap::replaced(
            &mut in_use,
            RcChannelMap::AETR_ARM_CH9
        ));
        assert!(!RcChannelMap::replaced(
            &mut in_use,
            RcChannelMap::AETR_ARM_CH9
        ));
        assert_eq!(in_use, Some(RcChannelMap::AETR_ARM_CH9));
    }

    #[test]
    fn channel_map_reads_taer_and_round_trips_its_config_values() {
        let mut channels = [0_u16; 16];
        channels[0] = 1;
        channels[1] = 2;
        channels[2] = 3;
        channels[3] = 4;
        channels[11] = 12;
        let map = RcChannelMap::from_config(2314, 12).unwrap();
        assert_eq!(
            map.sticks(&channels),
            RcSticks {
                roll: 2,
                pitch: 3,
                throttle: 1,
                yaw: 4,
            }
        );
        assert_eq!(map.arm(&channels), 12);
        assert_eq!(
            RcChannelMap::from_config(map.stick_order(), map.arm_channel()),
            Some(map)
        );
    }

    #[test]
    fn a_map_that_repeats_a_channel_or_misplaces_the_arm_switch_is_refused() {
        assert!(RcChannelMap::from_config(1234, 5).is_some());
        assert!(
            RcChannelMap::from_config(1123, 5).is_none(),
            "channel 1 twice"
        );
        assert!(
            RcChannelMap::from_config(1235, 6).is_none(),
            "channel 5 is not a stick"
        );
        assert!(RcChannelMap::from_config(123, 5).is_none(), "three sticks");
        assert!(RcChannelMap::from_config(12345, 5).is_none(), "five digits");
        assert!(
            RcChannelMap::from_config(1234, 4).is_none(),
            "arm on a stick channel"
        );
        assert!(
            RcChannelMap::from_config(1234, 17).is_none(),
            "beyond channel 16"
        );
    }
}
