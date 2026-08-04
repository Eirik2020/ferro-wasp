/// Latest bounded radio-control sample published by a serial consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RcInputSnapshot {
    /// Sixteen decoded protocol channel values.
    pub channels: [u16; 16],
    /// First SBUS digital channel flag.
    pub digital_channel_1: bool,
    /// Second SBUS digital channel flag.
    pub digital_channel_2: bool,
    /// Whether the latest SBUS packet reported a lost frame.
    pub frame_lost: bool,
    /// Whether the latest SBUS packet reported receiver failsafe.
    pub failsafe: bool,
    /// Number of successfully decoded frames, with saturating arithmetic.
    pub valid_frames: u32,
    /// Number of malformed frames, with saturating arithmetic.
    pub parse_errors: u32,
    /// Whether at least one valid frame has been decoded.
    pub has_valid_frame: bool,
}

impl RcInputSnapshot {
    /// Creates an empty snapshot for a port that has not received RC data.
    pub const fn new() -> Self {
        Self {
            channels: [0; 16],
            digital_channel_1: false,
            digital_channel_2: false,
            frame_lost: false,
            failsafe: false,
            valid_frames: 0,
            parse_errors: 0,
            has_valid_frame: false,
        }
    }

    /// Replaces the sample fields and records one successfully decoded frame.
    pub fn record_frame(
        &mut self,
        channels: [u16; 16],
        digital_channel_1: bool,
        digital_channel_2: bool,
        frame_lost: bool,
        failsafe: bool,
    ) {
        self.channels = channels;
        self.digital_channel_1 = digital_channel_1;
        self.digital_channel_2 = digital_channel_2;
        self.frame_lost = frame_lost;
        self.failsafe = failsafe;
        self.valid_frames = self.valid_frames.saturating_add(1);
        self.has_valid_frame = true;
    }

    /// Records one malformed protocol frame without replacing the last sample.
    pub fn record_parse_error(&mut self) {
        self.parse_errors = self.parse_errors.saturating_add(1);
    }
}

impl Default for RcInputSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_frames_and_errors_without_losing_the_last_sample() {
        let mut snapshot = RcInputSnapshot::new();
        snapshot.record_parse_error();
        snapshot.record_frame([42; 16], true, false, true, false);

        assert_eq!(snapshot.channels, [42; 16]);
        assert!(snapshot.digital_channel_1);
        assert!(snapshot.frame_lost);
        assert_eq!(snapshot.valid_frames, 1);
        assert_eq!(snapshot.parse_errors, 1);
        assert!(snapshot.has_valid_frame);
    }
}
