//! A beeper over an `embedded-hal` output pin: patterns of on and off
//! segments, driven at their boundaries, for `no_std` firmware.
//!
//! The pin is the whole part: a passive buzzer, or the gate of a driven
//! one. A pattern is a `&'static` slice of segments, each an on-time and
//! an off-time in microseconds, played a fixed number of times; the
//! durations and the repeat count are the caller's, so a pattern's total
//! work is known the moment it is asked for. One [`Buzzer::update`] at a
//! time the caller reads drives every segment boundary passed since the
//! last one, then stops: the work in a call is bounded by the pattern's
//! whole length, and a finished pattern leaves the pin low.
//!
//! The crate never panics on any input, never allocates, and has no
//! unsafe code.
//!
//! Requirements: BUZZER-001, BUZZER-002, BUZZER-003, BUZZER-004

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::digital::{ErrorKind, ErrorType, OutputPin};

    /// A pin that records every level it was driven to.
    #[derive(Default)]
    struct Pin {
        high: bool,
        history: std::vec::Vec<bool>,
        fail: bool,
    }

    impl Pin {
        fn record(&mut self, high: bool) -> Result<(), ErrorKind> {
            if self.fail {
                return Err(ErrorKind::Other);
            }
            self.high = high;
            self.history.push(high);
            Ok(())
        }
    }

    impl ErrorType for Pin {
        type Error = ErrorKind;
    }

    impl OutputPin for Pin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.record(false)
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.record(true)
        }
    }

    /// One beep: on 100 ms, off 100 ms.
    const BEEP: Segment = Segment {
        on_us: 100_000,
        off_us: 100_000,
    };

    #[test]
    fn a_pattern_drives_the_pin_at_its_boundaries() {
        let mut buzzer = Buzzer::new(Pin::default());
        assert!(!buzzer.is_playing());
        buzzer.play(&[BEEP], 1);
        assert!(buzzer.is_playing());

        // The first half-segment begins at the update that starts it.
        buzzer.update(1_000).unwrap();
        assert_eq!(buzzer.pin.history, [true]);
        buzzer.update(50_000).unwrap();
        assert_eq!(buzzer.pin.history, [true]);

        // On-time over: the off-half begins.
        buzzer.update(101_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false]);
        assert!(buzzer.is_playing());

        // Off-half over: the pattern is over, the pin low.
        buzzer.update(201_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, false]);
        assert!(!buzzer.is_playing());

        // A finished pattern drives nothing further.
        buzzer.update(500_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, false]);
        assert_eq!(buzzer.update(1_000_000), Ok(()));
    }

    #[test]
    fn two_repeats_play_through_twice() {
        let mut buzzer = Buzzer::new(Pin::default());
        buzzer.play(&[BEEP], 2);

        // First repeat: on, then off.
        buzzer.update(0).unwrap();
        assert_eq!(buzzer.pin.history, [true]);
        buzzer.update(100_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false]);
        // Second repeat: on, then off.
        buzzer.update(200_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, true]);
        buzzer.update(300_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, true, false]);
        // The second off-half ends the pattern.
        buzzer.update(400_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, true, false, false]);
        assert!(!buzzer.is_playing());
    }

    #[test]
    fn a_late_update_processes_every_boundary_and_is_bounded() {
        let mut buzzer = Buzzer::new(Pin::default());
        // Three segments played once: 300 ms of on and off boundaries.
        buzzer.play(&[BEEP, BEEP, BEEP], 1);

        // The caller was away for the whole pattern: one update passes every
        // boundary, and the pattern is over, its work bounded by the
        // pattern's whole length.
        buzzer.update(0).unwrap();
        buzzer.update(1_000_000).unwrap();
        assert!(!buzzer.is_playing());
        // The boundaries were processed to the pattern's end, and the pin
        // ends low.
        assert_eq!(buzzer.pin.history.last(), Some(&false));
        assert!(buzzer.pin.history.len() <= 8);
    }

    #[test]
    fn a_zero_length_segment_is_skipped_without_waiting() {
        let mut buzzer = Buzzer::new(Pin::default());
        // On, then no off-time: the pattern ends the moment it begins.
        buzzer.play(
            &[Segment {
                on_us: 100_000,
                off_us: 0,
            }],
            1,
        );
        buzzer.update(0).unwrap();
        assert_eq!(buzzer.pin.history, [true]);
        buzzer.update(100_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false]);
        assert!(!buzzer.is_playing());
    }

    #[test]
    fn stop_ends_the_pattern_with_the_pin_low() {
        let mut buzzer = Buzzer::new(Pin::default());
        buzzer.play(&[BEEP], 10);
        buzzer.update(0).unwrap();
        assert_eq!(buzzer.pin.history, [true]);
        buzzer.stop().unwrap();
        assert!(!buzzer.is_playing());
        assert_eq!(buzzer.pin.history, [true, false]);
        // A stopped buzzer drives nothing further.
        buzzer.update(10_000_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false]);

        // Stop on an already-stopped buzzer drives the pin low once more.
        buzzer.stop().unwrap();
        assert_eq!(buzzer.pin.history, [true, false, false]);
        // The pin comes back with the buzzer it drove.
        assert_eq!(buzzer.into_inner().history, [true, false, false]);
    }

    #[test]
    fn a_new_pattern_replaces_the_playing_one() {
        let mut buzzer = Buzzer::new(Pin::default());
        buzzer.play(&[BEEP], 10);
        buzzer.update(0).unwrap();
        buzzer.update(100_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false]);

        // The new pattern begins from its first segment, replacing the old.
        static LONG: [Segment; 1] = [Segment {
            on_us: 1_000_000,
            off_us: 1_000_000,
        }];
        buzzer.play(&LONG, 1);
        buzzer.update(100_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, true]);
        buzzer.update(1_100_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, true, false]);
        assert!(buzzer.is_playing());
        buzzer.update(2_100_000).unwrap();
        assert_eq!(buzzer.pin.history, [true, false, true, false, false]);
        assert!(!buzzer.is_playing());
    }

    #[test]
    fn a_failing_pin_fails_the_call() {
        let mut buzzer = Buzzer::new(Pin::default());
        buzzer.pin.fail = true;
        buzzer.play(&[BEEP], 1);
        assert!(buzzer.update(0).is_err());
        assert!(buzzer.stop().is_err());
    }

    #[test]
    fn an_empty_pattern_ends_immediately_and_drives_nothing() {
        let mut buzzer = Buzzer::new(Pin::default());
        buzzer.play(&[], 5);
        buzzer.update(0).unwrap();
        // An empty pattern is a pattern of no boundaries: nothing driven, and
        // not playing.
        assert!(!buzzer.is_playing());
        assert_eq!(buzzer.pin.history, [false]);
    }

    /// One segment of a pattern: the pin driven high for `on_us`, then low
    /// for `off_us`.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Segment {
        /// How long the pin stays high, in microseconds.
        pub on_us: u32,
        /// How long the pin stays low after it, in microseconds.
        pub off_us: u32,
    }

    /// A pattern being played: its segments and how many times through.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Playing {
        segments: &'static [Segment],
        repeats_left: u32,
        /// The segment the pattern is inside.
        step: usize,
        /// Whether the pin is high inside the segment.
        on: bool,
        /// When the current half-segment ends, in microseconds by the
        /// caller's clock. `None` before the first update: the pattern has
        /// not begun, so the first half-segment is taken at the first
        /// update, however late it comes.
        ends_at_us: Option<u64>,
    }

    /// A beeper over one output pin, driven by a caller-read clock.
    pub struct Buzzer<P> {
        pin: P,
        playing: Option<Playing>,
    }

    impl<P> Buzzer<P> {
        pub const fn new(pin: P) -> Self {
            Self { pin, playing: None }
        }

        pub fn into_inner(self) -> P {
            self.pin
        }

        /// Whether a pattern is still playing.
        pub fn is_playing(&self) -> bool {
            self.playing.is_some()
        }
    }

    impl<P: OutputPin> Buzzer<P> {
        /// Play `segments` `repeats` times, starting at the next
        /// [`Buzzer::update`]. A pattern played over a playing one replaces
        /// it, from its first segment.
        pub fn play(&mut self, segments: &'static [Segment], repeats: u32) {
            self.playing = Some(Playing {
                segments,
                repeats_left: repeats,
                step: 0,
                on: false,
                ends_at_us: None,
            });
        }

        /// End the pattern and drive the pin low. A buzzer that is not
        /// playing only drives the pin low once more.
        pub fn stop(&mut self) -> Result<(), P::Error> {
            self.playing = None;
            self.pin.set_low()
        }

        /// Drive every segment boundary passed since the last update: each
        /// flips the pin, and the work in one call is bounded by the
        /// pattern's whole length, because a finished pattern ends itself
        /// with the pin low. `now_us` is the caller's clock, which must not
        /// run backwards between calls.
        pub fn update(&mut self, now_us: u64) -> Result<(), P::Error> {
            let Some(playing) = &mut self.playing else {
                return Ok(());
            };
            // Every boundary passed is processed, at most the pattern's whole
            // length, then the pattern is over.
            loop {
                if playing.step >= playing.segments.len() || playing.repeats_left == 0 {
                    // A pattern of no segments, or of no repeats, is over
                    // before it begins: the pin ends low.
                    self.playing = None;
                    return self.pin.set_low();
                }
                let began = match playing.ends_at_us {
                    Some(ends_at_us) => {
                        if now_us < ends_at_us {
                            return Ok(());
                        }
                        ends_at_us
                    }
                    // The pattern has not begun: the first half-segment
                    // starts now, however late this update comes.
                    None => now_us,
                };
                if playing.ends_at_us.is_some() && !playing.on {
                    // The off-half of a segment ends: the next one begins, or
                    // a repeat does, or the pattern is over.
                    playing.step += 1;
                    if playing.step >= playing.segments.len() {
                        playing.step = 0;
                        if playing.repeats_left > 1 {
                            playing.repeats_left -= 1;
                        } else {
                            self.playing = None;
                            return self.pin.set_low();
                        }
                    }
                }
                playing.on = !playing.on;
                let segment = playing.segments[playing.step];
                let half = if playing.on {
                    segment.on_us
                } else {
                    segment.off_us
                };
                playing.ends_at_us = Some(began.saturating_add(u64::from(half)));
                if half == 0 {
                    // A zero-length half-segment cannot be waited for; drop
                    // it and let the loop take the next one.
                    continue;
                }
                if playing.on {
                    self.pin.set_high()?;
                } else {
                    self.pin.set_low()?;
                }
            }
        }
    }
}
