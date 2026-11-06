//! DShot on FerroHAL's waveform generator: `DshotBank` holds the command lease,
//! telemetry, completion and fault policy, and this encodes each frame as samples and
//! plays them. FerroHAL knows nothing of DShot.
//!
//! One bank plays plain DShot. FerroHAL's generator can also read a lane's pin, which
//! bidirectional DShot will need, and this doesn't use it yet.
//!
//! Requirements: HW-DSHOT-001, HW-DSHOT-002, HW-DSHOT-003, HW-DSHOT-004, HW-DSHOT-005, HW-DSHOT-006
//! Safety-critical.

use core::cell::RefCell;

pub use crate::dshot_bank::channel;
pub use crate::dshot_bank::*;
use ferrohal_stm32::timer::{Awg, ChannelTimer, Error, Lanes};
use ferrowasp_output::dshot::{DshotPacket, DshotTiming, encode_compare_sequence};

pub type DshotDmaBuffer = [u16; COMPARE_DMA_SLOTS];
type DshotBuffer = &'static mut DshotDmaBuffer;

/// Static storage for two buffers per motor.
pub struct DshotDmaStorage {
    buffers: [DshotDmaBuffer; 8],
}

impl DshotDmaStorage {
    pub const fn new() -> Self {
        Self {
            buffers: [[0; COMPARE_DMA_SLOTS]; 8],
        }
    }
}

impl Default for DshotDmaStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Four motors on a waveform generator's lanes. `lanes[motor]` is that motor's lane.
pub struct AwgLanes<A: ChannelTimer, B: ChannelTimer, LA, LB> {
    awg: RefCell<Awg<A, B, LA, LB>>,
    lane_of: [usize; 4],
    buffers: [Option<DshotBuffer>; 4],
    /// Each lane's end, once seen, until the bank has cleared it.
    ended: RefCell<[Option<Result<(), Error>>; 4]>,
}

impl<A, B, LA, LB> DshotLanes for AwgLanes<A, B, LA, LB>
where
    A: ChannelTimer,
    B: ChannelTimer,
    LA: Lanes<A>,
    LB: Lanes<B>,
{
    type Buffer = DshotDmaBuffer;

    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut Self::Buffer) -> u16 {
        encode_compare_sequence(packet, timing, buffer)
    }

    fn exchange_buffer(
        &mut self,
        motor: DshotMotor,
        next: &'static mut Self::Buffer,
    ) -> Result<&'static mut Self::Buffer, &'static mut Self::Buffer> {
        let slot = &mut self.buffers[motor.index()];
        match slot.take() {
            Some(old) => {
                *slot = Some(next);
                Ok(old)
            }
            None => Err(next),
        }
    }

    /// The generator enables a lane's stream as it loads it, in `start_frame`.
    fn start_dma(&mut self, _motor: DshotMotor) {}

    fn start_frame(&mut self, first_duties: [u16; 4]) {
        let awg = self.awg.get_mut();
        let ended = self.ended.get_mut();
        *ended = [None; 4];
        for motor in DshotMotor::ALL {
            let index = motor.index();
            let mut samples = [0u32; COMPARE_DMA_SLOTS];
            samples[0] = u32::from(first_duties[index]);
            if let Some(buffer) = self.buffers[index].as_deref() {
                // The first bit is `first_duties`; the buffer holds the others and
                // a closing zero, which the lane adds itself.
                for (sample, duty) in samples[1..].iter_mut().zip(buffer.iter()) {
                    *sample = u32::from(*duty);
                }
            }
            if awg.load(self.lane_of[index], &samples).is_err() {
                ended[index] = Some(Err(Error::Dma));
            }
        }
        if awg.play().is_err() {
            *ended = [Some(Err(Error::Dma)); 4];
        }
    }

    fn stop_frame(&mut self) {
        self.awg.get_mut().abort();
    }

    fn force_outputs_low(&mut self) {
        let awg = self.awg.get_mut();
        awg.abort();
        awg.set_outputs(false);
    }

    fn dma_status(&self, motor: DshotMotor) -> (bool, bool) {
        let index = motor.index();
        let mut ended = self.ended.borrow_mut();
        if ended[index].is_none() {
            ended[index] = self.awg.borrow_mut().on_interrupt(self.lane_of[index]);
        }
        match ended[index] {
            Some(Ok(())) => (true, false),
            Some(Err(_)) => (false, true),
            None => (false, false),
        }
    }

    /// The generator clears a lane's stream as it ends. What's left is this side's
    /// record of it.
    fn pause_and_clear(&mut self, _motor: DshotMotor) {}
}

pub type DshotMotorBank<A, B, LA, LB> = DshotBank<AwgLanes<A, B, LA, LB>>;

/// Builds the bank over `awg`, which has to run at the DShot600 bit rate from a timer
/// clock of `timer_clock_hz`. Its outputs go on at their inactive level.
pub fn init_dshot_motor_bank<A, B, LA, LB>(
    mut awg: Awg<A, B, LA, LB>,
    lane_of: [usize; 4],
    timer_clock_hz: u32,
    storage: &'static mut DshotDmaStorage,
) -> Result<DshotMotorBank<A, B, LA, LB>, DshotInitError>
where
    A: ChannelTimer,
    B: ChannelTimer,
    LA: Lanes<A>,
    LB: Lanes<B>,
{
    let timing = DshotTiming::from_clocks(timer_clock_hz, DSHOT600_BITRATE_HZ)
        .map_err(DshotInitError::InvalidTiming)?;
    if u32::from(timing.arr) + 1 != awg.period() {
        return Err(DshotInitError::LaneTimerMissing);
    }
    awg.set_outputs(true);
    let [m1, s1, m2, s2, m3, s3, m4, s4] = storage.buffers.each_mut();
    Ok(DshotBank::from_lanes(
        AwgLanes {
            awg: RefCell::new(awg),
            lane_of,
            buffers: [Some(m1), Some(m2), Some(m3), Some(m4)],
            ended: RefCell::new([None; 4]),
        },
        [s1, s2, s3, s4],
        timing,
    ))
}
