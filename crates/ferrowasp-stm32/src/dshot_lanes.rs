//! The four-lane DShot bank's plumbing, whatever the chip: the board's timer
//! set, its four lanes, and the `DshotLanes` the shared `DshotBank` drives.
//!
//! A chip backend supplies the hardware under these traits: `LaneTimer` for
//! each timer type it can drive, and `LaneDma` for its DMA lanes. Lanes find
//! their timer in the set by type, so a board names a lane once, as a timer,
//! a channel, and a stream, and the backend derives everything else.

use core::any::Any;
use core::marker::PhantomData;

use crate::dshot_bank::{DshotBank, DshotInitError, DshotLanes, DshotMotor};
use ferrowasp_waveform::dshot::{
    COMPARE_DMA_SLOTS, DshotPacket, DshotTiming, encode_compare_sequence,
};

/// Which outputs of one timer the bank drives: bit n is channel n + 1, and
/// a bit also set in `complementary` drives that channel's CHxN output
/// instead of CHx.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LaneOutputs {
    pub channels: u8,
    pub complementary: u8,
}

/// The register sequencing DShot needs from one timer.
pub trait LaneTimer: Any {
    /// Which clock the timer counts, as an identifier the backend chooses:
    /// timers of one bank must share it, because one bit timing encodes
    /// every lane.
    fn clock_domain(&self) -> u8;
    /// Count at the DShot bit rate with the outputs in PWM mode 1, preload
    /// on, enabled active high, and every compare at zero.
    fn configure(&self, timing: DshotTiming, outputs: LaneOutputs);
    /// Stop the counter with the requests closed, load each channel's first
    /// duty from `first[channel]`, stage the counter at ARR so its first
    /// update latches them, and open the requests.
    fn stage(&self, channels: u8, first: &[u16; 4], period_end: u16);
    fn start(&self);
    /// Stop with every output held low: all compares at zero.
    fn stop(&self, channels: u8);
    /// Stop, then hold the outputs inactive until the bank is rebuilt.
    fn force_inactive(&self, outputs: LaneOutputs);
}

/// The timers a bank drives, in slots. A lane finds its timer by type.
pub trait TimerSet: 'static {
    const LEN: usize;
    fn timer(&self, slot: usize) -> &dyn LaneTimer;
    fn find<T: 'static>(&self) -> Option<(usize, &T)>;
}

macro_rules! timer_sets {
    ($(($($T:ident: $slot:tt),+)),+) => {
        $(
            impl<$($T: LaneTimer),+> TimerSet for ($($T,)+) {
                const LEN: usize = [$($slot),+].len();

                fn timer(&self, slot: usize) -> &dyn LaneTimer {
                    match slot {
                        $($slot => &self.$slot,)+
                        _ => unreachable!("slot below LEN"),
                    }
                }

                fn find<T: 'static>(&self) -> Option<(usize, &T)> {
                    $(
                        if let Some(timer) = (&self.$slot as &dyn Any).downcast_ref::<T>() {
                            return Some(($slot, timer));
                        }
                    )+
                    None
                }
            }
        )+
    };
}

timer_sets!((A: 0), (A: 0, B: 1), (A: 0, B: 1, C: 2), (A: 0, B: 1, C: 2, D: 3));

/// How a bank's timers start each frame set.
pub trait StartMode<TS> {
    /// Wire the timers up; return the slot that is started by trigger
    /// rather than in software, if any.
    fn wire(&self, timers: &TS) -> Result<Option<usize>, DshotInitError>;
}

/// Start every timer in software, one after another.
pub struct SoftwareStart;

impl<TS: TimerSet> StartMode<TS> for SoftwareStart {
    fn wire(&self, _timers: &TS) -> Result<Option<usize>, DshotInitError> {
        Ok(None)
    }
}

/// A timer whose trigger output can start another: its TRGO follows its
/// counter enable.
pub trait TriggerMaster: LaneTimer {
    fn lead(&self);
}

/// A timer that `M`'s trigger output can start, through the internal
/// trigger input the reference manual wires to `M`.
pub trait TriggerSlave<M>: LaneTimer {
    fn follow(&self);
}

/// Start `M` in software and `S` from `M`'s trigger output, so both begin
/// on the same edge. Available only where the backend declares `S` a
/// trigger slave of `M`.
pub struct TriggeredStart<M, S>(PhantomData<(M, S)>);

impl<M, S> TriggeredStart<M, S> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<M, S> Default for TriggeredStart<M, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<TS, M, S> StartMode<TS> for TriggeredStart<M, S>
where
    TS: TimerSet,
    M: TriggerMaster,
    S: TriggerSlave<M>,
{
    fn wire(&self, timers: &TS) -> Result<Option<usize>, DshotInitError> {
        let (_, master) = timers.find::<M>().ok_or(DshotInitError::LaneTimerMissing)?;
        let (slave_slot, slave) = timers.find::<S>().ok_or(DshotInitError::LaneTimerMissing)?;
        master.lead();
        slave.follow();
        Ok(Some(slave_slot))
    }
}

/// A lane's DMA buffer of compare words, in the word size its timers'
/// compare registers take.
pub trait LaneBuffer: 'static {
    /// Encode `packet`, returning the first bit's compare duty.
    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut Self) -> u16;
}

impl LaneBuffer for [u16; COMPARE_DMA_SLOTS] {
    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut Self) -> u16 {
        encode_compare_sequence(packet, timing, buffer)
    }
}

impl LaneBuffer for [u32; COMPARE_DMA_SLOTS] {
    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut Self) -> u16 {
        let mut compares = [0; COMPARE_DMA_SLOTS];
        let first = encode_compare_sequence(packet, timing, &mut compares);
        for (word, compare) in buffer.iter_mut().zip(compares) {
            *word = u32::from(compare);
        }
        first
    }
}

/// One built lane, whatever its stream, timer, and channel.
pub trait LaneDma {
    type Buffer: LaneBuffer;
    fn slot(&self) -> usize;
    fn channel(&self) -> usize;
    fn complementary(&self) -> bool;
    #[allow(clippy::type_complexity)]
    fn exchange(
        &mut self,
        next: &'static mut Self::Buffer,
    ) -> Result<&'static mut Self::Buffer, &'static mut Self::Buffer>;
    fn start(&mut self);
    /// (transfer complete, transfer or direct-mode error).
    fn status(&self) -> (bool, bool);
    fn pause_and_clear(&mut self);
}

/// Static storage for two DMA buffers per motor lane.
pub struct DshotDmaStorage<B> {
    buffers: [B; 8],
}

impl DshotDmaStorage<[u16; COMPARE_DMA_SLOTS]> {
    pub const fn new() -> Self {
        Self {
            buffers: [[0; COMPARE_DMA_SLOTS]; 8],
        }
    }
}

impl Default for DshotDmaStorage<[u16; COMPARE_DMA_SLOTS]> {
    fn default() -> Self {
        Self::new()
    }
}

impl DshotDmaStorage<[u32; COMPARE_DMA_SLOTS]> {
    pub const fn new() -> Self {
        Self {
            buffers: [[0; COMPARE_DMA_SLOTS]; 8],
        }
    }
}

impl Default for DshotDmaStorage<[u32; COMPARE_DMA_SLOTS]> {
    fn default() -> Self {
        Self::new()
    }
}

/// A lane the board named, to be built against the bank's timers.
pub trait IntoLane<TS> {
    type Lane: LaneDma;
    fn into_lane(
        self,
        timers: &TS,
        buffer: &'static mut <Self::Lane as LaneDma>::Buffer,
    ) -> Result<Self::Lane, DshotInitError>;
}

/// The timers and four lanes under a `DshotBank`.
pub struct Lanes<TS, A, B, C, D> {
    timers: TS,
    /// Each timer slot's lane outputs.
    outputs: [LaneOutputs; 4],
    /// The slot started by another timer's trigger, if any.
    triggered_slot: Option<usize>,
    period_end: u16,
    motor1: A,
    motor2: B,
    motor3: C,
    motor4: D,
}

/// A bank over the timer set `TS` and lanes `A`-`D`, for motors 1-4.
pub type DshotMotorBank<TS, A, B, C, D> = DshotBank<Lanes<TS, A, B, C, D>>;

impl<TS, A, B, C, D> Lanes<TS, A, B, C, D>
where
    TS: TimerSet,
    A: LaneDma,
    B: LaneDma<Buffer = A::Buffer>,
    C: LaneDma<Buffer = A::Buffer>,
    D: LaneDma<Buffer = A::Buffer>,
{
    fn lane(&self, motor: DshotMotor) -> &dyn LaneDma<Buffer = A::Buffer> {
        match motor {
            DshotMotor::Motor1 => &self.motor1,
            DshotMotor::Motor2 => &self.motor2,
            DshotMotor::Motor3 => &self.motor3,
            DshotMotor::Motor4 => &self.motor4,
        }
    }

    fn lane_mut(&mut self, motor: DshotMotor) -> &mut dyn LaneDma<Buffer = A::Buffer> {
        match motor {
            DshotMotor::Motor1 => &mut self.motor1,
            DshotMotor::Motor2 => &mut self.motor2,
            DshotMotor::Motor3 => &mut self.motor3,
            DshotMotor::Motor4 => &mut self.motor4,
        }
    }

    /// The slots that carry at least one lane, each with its outputs.
    fn used_slots(&self) -> impl Iterator<Item = (usize, LaneOutputs)> + '_ {
        (0..TS::LEN).filter_map(|slot| {
            let outputs = self.outputs[slot];
            (outputs.channels != 0).then_some((slot, outputs))
        })
    }
}

impl<TS, A, B, C, D> DshotLanes for Lanes<TS, A, B, C, D>
where
    TS: TimerSet,
    A: LaneDma,
    B: LaneDma<Buffer = A::Buffer>,
    C: LaneDma<Buffer = A::Buffer>,
    D: LaneDma<Buffer = A::Buffer>,
{
    type Buffer = A::Buffer;

    fn encode(packet: DshotPacket, timing: DshotTiming, buffer: &mut A::Buffer) -> u16 {
        A::Buffer::encode(packet, timing, buffer)
    }

    fn exchange_buffer(
        &mut self,
        motor: DshotMotor,
        next: &'static mut A::Buffer,
    ) -> Result<&'static mut A::Buffer, &'static mut A::Buffer> {
        self.lane_mut(motor).exchange(next)
    }

    fn start_dma(&mut self, motor: DshotMotor) {
        self.lane_mut(motor).start();
    }

    fn start_frame(&mut self, first_duties: [u16; 4]) {
        // Each timer slot's first duties, by channel.
        let mut first = [[0u16; 4]; 4];
        for (motor, duty) in DshotMotor::ALL.into_iter().zip(first_duties) {
            let lane = self.lane(motor);
            first[lane.slot()][lane.channel()] = duty;
        }
        for (slot, outputs) in self.used_slots() {
            self.timers
                .timer(slot)
                .stage(outputs.channels, &first[slot], self.period_end);
        }
        // A triggered timer starts from its master's trigger output.
        for (slot, _) in self.used_slots() {
            if Some(slot) != self.triggered_slot {
                self.timers.timer(slot).start();
            }
        }
    }

    fn stop_frame(&mut self) {
        for (slot, outputs) in self.used_slots() {
            self.timers.timer(slot).stop(outputs.channels);
        }
    }

    fn force_outputs_low(&mut self) {
        for (slot, outputs) in self.used_slots() {
            self.timers.timer(slot).force_inactive(outputs);
        }
    }

    fn dma_status(&self, motor: DshotMotor) -> (bool, bool) {
        self.lane(motor).status()
    }

    fn pause_and_clear(&mut self, motor: DshotMotor) {
        self.lane_mut(motor).pause_and_clear();
    }
}

/// Build a bank over `timers` with `lanes` for motors 1-4, started as
/// `start` says. `timing_for` turns the lane timers' shared clock domain
/// into the bit timing; the backend knows what each domain counts. Each
/// timer that carries a lane is configured with its outputs parked low.
#[allow(clippy::type_complexity)]
pub fn build_bank<TS, LA, LB, LC, LD>(
    timers: TS,
    lanes: (LA, LB, LC, LD),
    start: impl StartMode<TS>,
    timing_for: impl FnOnce(u8) -> Result<DshotTiming, DshotInitError>,
    storage: &'static mut DshotDmaStorage<<LA::Lane as LaneDma>::Buffer>,
) -> Result<DshotMotorBank<TS, LA::Lane, LB::Lane, LC::Lane, LD::Lane>, DshotInitError>
where
    TS: TimerSet,
    LA: IntoLane<TS>,
    LB: IntoLane<TS>,
    LC: IntoLane<TS>,
    LD: IntoLane<TS>,
    LB::Lane: LaneDma<Buffer = <LA::Lane as LaneDma>::Buffer>,
    LC::Lane: LaneDma<Buffer = <LA::Lane as LaneDma>::Buffer>,
    LD::Lane: LaneDma<Buffer = <LA::Lane as LaneDma>::Buffer>,
{
    let [
        motor1_active,
        motor1_spare,
        motor2_active,
        motor2_spare,
        motor3_active,
        motor3_spare,
        motor4_active,
        motor4_spare,
    ] = storage.buffers.each_mut();
    let motor1 = lanes.0.into_lane(&timers, motor1_active)?;
    let motor2 = lanes.1.into_lane(&timers, motor2_active)?;
    let motor3 = lanes.2.into_lane(&timers, motor3_active)?;
    let motor4 = lanes.3.into_lane(&timers, motor4_active)?;

    let mut outputs = [LaneOutputs::default(); 4];
    let lane_slots = [
        (motor1.slot(), motor1.channel(), motor1.complementary()),
        (motor2.slot(), motor2.channel(), motor2.complementary()),
        (motor3.slot(), motor3.channel(), motor3.complementary()),
        (motor4.slot(), motor4.channel(), motor4.complementary()),
    ];
    for (slot, channel, complementary) in lane_slots {
        let bit = 1 << channel;
        let slot_outputs = &mut outputs[slot];
        if slot_outputs.channels & bit != 0 {
            return Err(DshotInitError::LaneChannelReused);
        }
        slot_outputs.channels |= bit;
        if complementary {
            slot_outputs.complementary |= bit;
        }
    }

    // One bit timing serves every lane, so every lane timer must count the
    // same clock.
    let mut clock_domain = None;
    for (slot, slot_outputs) in outputs.iter().enumerate().take(TS::LEN) {
        if slot_outputs.channels == 0 {
            continue;
        }
        let domain = timers.timer(slot).clock_domain();
        match clock_domain {
            None => clock_domain = Some(domain),
            Some(first) if first != domain => return Err(DshotInitError::LaneClocksDiffer),
            Some(_) => {}
        }
    }
    let timing = timing_for(clock_domain.ok_or(DshotInitError::LaneTimerMissing)?)?;

    for (slot, slot_outputs) in outputs.iter().enumerate().take(TS::LEN) {
        if slot_outputs.channels != 0 {
            timers.timer(slot).configure(timing, *slot_outputs);
        }
    }
    let triggered_slot = start.wire(&timers)?;

    let lanes = Lanes {
        timers,
        outputs,
        triggered_slot,
        period_end: timing.arr,
        motor1,
        motor2,
        motor3,
        motor4,
    };
    Ok(DshotBank::from_lanes(
        lanes,
        [motor1_spare, motor2_spare, motor3_spare, motor4_spare],
        timing,
    ))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::cell::RefCell;
    use ferrowasp_waveform::dshot::DSHOT600_BITRATE_HZ;
    use std::{boxed::Box, vec::Vec};

    /// A timer that records each call, in a clock domain of the test's
    /// choosing.
    struct FakeTimer<const ID: u8> {
        domain: u8,
        calls: RefCell<Vec<(u8, &'static str, u8)>>,
    }

    impl<const ID: u8> FakeTimer<ID> {
        fn new(domain: u8) -> Self {
            Self {
                domain,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn record(&self, call: &'static str, channels: u8) {
            self.calls.borrow_mut().push((ID, call, channels));
        }
    }

    impl<const ID: u8> LaneTimer for FakeTimer<ID> {
        fn clock_domain(&self) -> u8 {
            self.domain
        }
        fn configure(&self, _timing: DshotTiming, outputs: LaneOutputs) {
            self.record("configure", outputs.channels);
        }
        fn stage(&self, channels: u8, _first: &[u16; 4], _period_end: u16) {
            self.record("stage", channels);
        }
        fn start(&self) {
            self.record("start", 0);
        }
        fn stop(&self, channels: u8) {
            self.record("stop", channels);
        }
        fn force_inactive(&self, outputs: LaneOutputs) {
            self.record("force_inactive", outputs.channels);
        }
    }

    impl TriggerMaster for FakeTimer<1> {
        fn lead(&self) {
            self.record("lead", 0);
        }
    }

    impl TriggerSlave<FakeTimer<1>> for FakeTimer<2> {
        fn follow(&self) {
            self.record("follow", 0);
        }
    }

    type Buffer = [u16; COMPARE_DMA_SLOTS];

    struct FakeLane {
        slot: usize,
        channel: usize,
    }

    impl LaneDma for FakeLane {
        type Buffer = Buffer;
        fn slot(&self) -> usize {
            self.slot
        }
        fn channel(&self) -> usize {
            self.channel
        }
        fn complementary(&self) -> bool {
            false
        }
        fn exchange(
            &mut self,
            next: &'static mut Buffer,
        ) -> Result<&'static mut Buffer, &'static mut Buffer> {
            Ok(next)
        }
        fn start(&mut self) {}
        fn status(&self) -> (bool, bool) {
            (false, false)
        }
        fn pause_and_clear(&mut self) {}
    }

    /// A lane on channel `channel` of the timer of type `T`.
    struct LaneOn<T> {
        channel: usize,
        _timer: PhantomData<T>,
    }

    fn lane<T>(channel: usize) -> LaneOn<T> {
        LaneOn {
            channel,
            _timer: PhantomData,
        }
    }

    impl<TS: TimerSet, T: 'static> IntoLane<TS> for LaneOn<T> {
        type Lane = FakeLane;
        fn into_lane(
            self,
            timers: &TS,
            _buffer: &'static mut Buffer,
        ) -> Result<FakeLane, DshotInitError> {
            let (slot, _) = timers.find::<T>().ok_or(DshotInitError::LaneTimerMissing)?;
            Ok(FakeLane {
                slot,
                channel: self.channel,
            })
        }
    }

    fn storage() -> &'static mut DshotDmaStorage<Buffer> {
        Box::leak(Box::new(DshotDmaStorage::<Buffer>::new()))
    }

    fn timing(_domain: u8) -> Result<DshotTiming, DshotInitError> {
        DshotTiming::from_clocks(168_000_000, DSHOT600_BITRATE_HZ)
            .map_err(DshotInitError::InvalidTiming)
    }

    type Pair = (FakeTimer<1>, FakeTimer<2>);

    fn pair(domains: (u8, u8)) -> Pair {
        (FakeTimer::new(domains.0), FakeTimer::new(domains.1))
    }

    fn calls(
        bank: &DshotMotorBank<Pair, FakeLane, FakeLane, FakeLane, FakeLane>,
    ) -> Vec<(u8, &'static str, u8)> {
        let timers = &bank.lanes().timers;
        let mut all = timers.0.calls.borrow().clone();
        all.extend(timers.1.calls.borrow().iter().copied());
        all
    }

    #[test]
    fn configures_each_timer_for_its_lane_channels_and_wires_the_trigger() {
        let bank = build_bank(
            pair((2, 2)),
            (
                lane::<FakeTimer<1>>(0),
                lane::<FakeTimer<2>>(3),
                lane::<FakeTimer<2>>(2),
                lane::<FakeTimer<1>>(2),
            ),
            TriggeredStart::<FakeTimer<1>, FakeTimer<2>>::new(),
            timing,
            storage(),
        )
        .unwrap_or_else(|_| panic!("bank builds"));
        let calls = calls(&bank);
        assert!(calls.contains(&(1, "configure", 0b0101)));
        assert!(calls.contains(&(2, "configure", 0b1100)));
        assert!(calls.contains(&(1, "lead", 0)));
        assert!(calls.contains(&(2, "follow", 0)));
    }

    #[test]
    fn a_frame_stages_every_used_timer_and_starts_all_but_the_triggered_one() {
        let mut bank = build_bank(
            pair((2, 2)),
            (
                lane::<FakeTimer<1>>(0),
                lane::<FakeTimer<2>>(3),
                lane::<FakeTimer<2>>(2),
                lane::<FakeTimer<1>>(2),
            ),
            TriggeredStart::<FakeTimer<1>, FakeTimer<2>>::new(),
            timing,
            storage(),
        )
        .unwrap_or_else(|_| panic!("bank builds"));
        bank.lanes_mut().start_frame([0; 4]);
        let calls = calls(&bank);
        assert!(calls.contains(&(1, "stage", 0b0101)));
        assert!(calls.contains(&(2, "stage", 0b1100)));
        assert!(calls.contains(&(1, "start", 0)));
        assert!(!calls.contains(&(2, "start", 0)));
    }

    #[test]
    fn software_start_starts_every_used_timer() {
        let mut bank = build_bank(
            pair((1, 1)),
            (
                lane::<FakeTimer<1>>(0),
                lane::<FakeTimer<1>>(1),
                lane::<FakeTimer<2>>(0),
                lane::<FakeTimer<2>>(1),
            ),
            SoftwareStart,
            timing,
            storage(),
        )
        .unwrap_or_else(|_| panic!("bank builds"));
        bank.lanes_mut().start_frame([0; 4]);
        let calls = calls(&bank);
        assert!(calls.contains(&(1, "start", 0)));
        assert!(calls.contains(&(2, "start", 0)));
    }

    #[test]
    fn refuses_two_lanes_on_one_channel() {
        let result = build_bank(
            pair((1, 1)),
            (
                lane::<FakeTimer<1>>(0),
                lane::<FakeTimer<1>>(0),
                lane::<FakeTimer<2>>(0),
                lane::<FakeTimer<2>>(1),
            ),
            SoftwareStart,
            timing,
            storage(),
        );
        assert!(matches!(result, Err(DshotInitError::LaneChannelReused)));
    }

    #[test]
    fn refuses_lane_timers_on_different_clocks() {
        let result = build_bank(
            pair((1, 2)),
            (
                lane::<FakeTimer<1>>(0),
                lane::<FakeTimer<1>>(1),
                lane::<FakeTimer<2>>(0),
                lane::<FakeTimer<2>>(1),
            ),
            SoftwareStart,
            timing,
            storage(),
        );
        assert!(matches!(result, Err(DshotInitError::LaneClocksDiffer)));
    }

    #[test]
    fn refuses_a_lane_on_a_timer_the_bank_was_not_given() {
        let result = build_bank(
            (FakeTimer::<1>::new(1),),
            (
                lane::<FakeTimer<1>>(0),
                lane::<FakeTimer<1>>(1),
                lane::<FakeTimer<1>>(2),
                lane::<FakeTimer<2>>(3),
            ),
            SoftwareStart,
            timing,
            storage(),
        );
        assert!(matches!(result, Err(DshotInitError::LaneTimerMissing)));
    }
}
