# Implementing DShot on STM32F405 with RTIC2 and Embedded Rust

> Historical design note. This describes the original single-lane DShot plan,
> not the current four-lane FCU3 implementation or its live safety contract.
> See `mdbook/src/dshot.md` and `mdbook/src/current_support.md` for current
> behavior and evidence status.

## Recommended architecture

Use:

```text
TIM1_CH1 / PA8
    ↓
PWM mode 1
    ↓
DMA2 Stream1 Channel6
    ↓
TIM1_CCR1
```

The CPU should only:

1. Encode the 16-bit DShot frame.
2. Convert each bit into a timer compare value.
3. Start one DMA transaction.
4. Handle one DMA-complete interrupt.

Do **not** bit-bang DShot from an RTIC task. Timer plus DMA provides deterministic edges regardless of interrupt latency.

This design targets RTIC 2.2 and `stm32f4xx-hal` 0.23.0 with the `stm32f405` and `rtic2` features.

The HAL provides ownership-safe DMA transfers and compile-time stream/channel validation, but its existing timer-CCR DMA wrapper cannot conveniently be constructed outside the HAL because its fields are private. A small local DMA endpoint wrapper is therefore the most practical approach.

---

## 1. DShot waveform

A normal DShot frame is:

```text
[ 11-bit value ][ telemetry bit ][ 4-bit CRC ]
```

Values are interpreted as:

```text
0       stop/disarm
1..47   special commands
48..2047 throttle
```

The CRC is:

```rust
(payload ^ (payload >> 4) ^ (payload >> 8)) & 0x0f
```

For standard non-bidirectional DShot:

- A logical `0` is high for 37.5% of the bit period.
- A logical `1` is high for 75% of the bit period.

With TIM1 running at 168 MHz:

| Rate | Period ticks | ARR | `0` high | `1` high |
|---|---:|---:|---:|---:|
| DShot150 | 1120 | 1119 | 420 | 840 |
| DShot300 | 560 | 559 | 210 | 420 |
| DShot600 | 280 | 279 | 105 | 210 |
| DShot1200 | 140 | 139 | 53 | 105 |

DShot600 is a good initial choice. It gives good timing resolution and occupies approximately 26.7 µs for the 16 transmitted bits.

---

## 2. Why trigger DMA from the compare event

A naïve implementation triggers DMA on the timer update event:

```text
update event
    ↓
DMA writes CCR
    ↓
output pulse starts
```

That makes the rising-edge period sensitive to DMA arbitration latency.

A more robust implementation uses the TIM1_CH1 compare event:

```text
bit N starts high
    ↓
CCR1 match causes falling edge
    ↓
same compare event requests DMA
    ↓
DMA writes bit N+1 duty into CCR1 preload
    ↓
next update event atomically loads the new duty
```

DMA therefore performs its work during the low portion of the current bit. The next rising edge is entirely timer-generated.

TIM1_CH1 maps to DMA2 Stream1 Channel6 on the STM32F405.

---

## 3. Dependencies

```toml
[package]
name = "stm32f405-dshot"
version = "0.1.0"
edition = "2024"

[dependencies]
cortex-m = "0.7.7"
cortex-m-rt = "0.7.5"
panic-halt = "1.0"

rtic = { version = "2.2", features = ["thumbv7-backend"] }

stm32f4xx-hal = {
    version = "0.23",
    features = [
        "stm32f405",
        "rtic2",
    ]
}
```

---

## 4. Protocol encoder

Keep the protocol module hardware-independent so it can be unit-tested on the host.

```rust
// src/dshot/protocol.rs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    ValueOutOfRange,
    ThrottleOutOfRange,
}

pub const MAX_VALUE: u16 = 2047;
pub const MAX_THROTTLE: u16 = 1999;

pub const fn frame(value: u16, telemetry: bool) -> Result<u16, FrameError> {
    if value > MAX_VALUE {
        return Err(FrameError::ValueOutOfRange);
    }

    let payload = (value << 1) | telemetry as u16;
    let checksum = (payload ^ (payload >> 4) ^ (payload >> 8)) & 0x0f;

    Ok((payload << 4) | checksum)
}

pub const fn throttle_frame(
    throttle: u16,
    telemetry: bool,
) -> Result<u16, FrameError> {
    if throttle > MAX_THROTTLE {
        return Err(FrameError::ThrottleOutOfRange);
    }

    frame(throttle + 48, telemetry)
}

pub const fn stop_frame() -> u16 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_is_zero() {
        assert_eq!(frame(0, false), Ok(0x0000));
    }

    #[test]
    fn minimum_throttle_frame() {
        assert_eq!(frame(48, false), Ok(0x0606));
    }

    #[test]
    fn maximum_frame_fits_sixteen_bits() {
        assert!(frame(2047, true).is_ok());
    }

    #[test]
    fn rejects_invalid_value() {
        assert_eq!(
            frame(2048, false),
            Err(FrameError::ValueOutOfRange)
        );
    }
}
```

---

## 5. Timer timing and DMA buffer encoding

The first duty cycle is written directly to CCR1. DMA then contains:

```text
bits 14 down to 0, followed by zero
```

The final zero ensures the PWM output becomes low after the last bit.

```rust
// src/dshot/timing.rs

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub arr: u16,
    pub zero_high: u16,
    pub one_high: u16,
}

impl Timing {
    pub const fn from_clocks(
        timer_hz: u32,
        bitrate: u32,
    ) -> Self {
        let period = div_round(timer_hz, bitrate);

        assert!(period >= 4);
        assert!(period <= u16::MAX as u32 + 1);

        let zero_high = div_round(period * 3, 8);
        let one_high = div_round(period * 3, 4);

        assert!(zero_high > 0);
        assert!(one_high < period);

        Self {
            arr: (period - 1) as u16,
            zero_high: zero_high as u16,
            one_high: one_high as u16,
        }
    }

    #[inline]
    pub const fn duty_for_bit(self, set: bool) -> u16 {
        if set {
            self.one_high
        } else {
            self.zero_high
        }
    }
}

const fn div_round(numerator: u32, denominator: u32) -> u32 {
    (numerator + denominator / 2) / denominator
}

pub const DSHOT600_168MHZ: Timing =
    Timing::from_clocks(168_000_000, 600_000);

pub fn encode_compare_sequence(
    frame: u16,
    timing: Timing,
    dma_buffer: &mut [u16; 16],
) -> u16 {
    let first = timing.duty_for_bit(frame & 0x8000 != 0);

    for (index, destination) in dma_buffer[..15].iter_mut().enumerate() {
        let bit_number = 14 - index;
        let set = frame & (1 << bit_number) != 0;
        *destination = timing.duty_for_bit(set);
    }

    dma_buffer[15] = 0;
    first
}
```

---

## 6. Minimal unsafe DMA endpoint

The HAL's `Transfer` owns the active DMA buffer and inserts the required compiler fences. Its safe `next_transfer` API exchanges ownership of complete buffers rather than exposing memory being read by DMA.

The local unsafe contract only states:

1. The peripheral address is TIM1_CCR1.
2. Its transfer width is 16 bits.
3. DMA2 Stream1 Channel6 is a valid memory-to-TIM1_CH1 route.

```rust
// src/dshot/stm32f405.rs

use stm32f4xx_hal::{
    dma::{
        traits::{DMASet, PeriAddress},
        MemoryToPeripheral,
        Stream1,
    },
    pac,
};

pub struct Tim1Ch1Dma {
    tim: pac::TIM1,
}

impl Tim1Ch1Dma {
    pub fn new(tim: pac::TIM1, arr: u16) -> Self {
        let mut this = Self { tim };
        this.configure(arr);
        this
    }

    fn configure(&mut self, arr: u16) {
        self.tim.cr1().modify(|_, w| w.cen().clear_bit());
        self.tim.dier().modify(|_, w| w.ccde(0).clear_bit());

        unsafe {
            self.tim.psc().write(|w| w.bits(0));
            self.tim.arr().write(|w| w.bits(arr as u32));
            self.tim.cnt().write(|w| w.bits(0));
            self.tim.ccr(0).write(|w| w.bits(0));
        }

        self.tim.ccmr1_output().modify(|_, w| {
            w.ocpe(0)
                .set_bit()
                .ocm(0)
                .set(6)
        });

        self.tim.ccer().modify(|_, w| {
            w.cce(0)
                .set_bit()
                .ccp(0)
                .clear_bit()
        });

        self.tim.cr1().modify(|_, w| w.arpe().set_bit());
        self.tim.bdtr().modify(|_, w| w.moe().set_bit());

        self.force_update();
        self.clear_timer_flags();
    }

    fn write_ccr1(&mut self, duty: u16) {
        unsafe {
            self.tim.ccr(0).write(|w| w.bits(duty as u32));
        }
    }

    fn force_update(&mut self) {
        self.tim.egr().write(|w| w.ug().set_bit());
    }

    fn clear_timer_flags(&mut self) {
        unsafe {
            self.tim.sr().write(|w| w.bits(0));
        }
    }

    pub fn start_frame(&mut self, first_duty: u16) {
        self.tim.cr1().modify(|_, w| w.cen().clear_bit());
        self.tim.dier().modify(|_, w| w.ccde(0).clear_bit());

        self.write_ccr1(first_duty);

        unsafe {
            self.tim.cnt().write(|w| w.bits(0));
        }

        self.force_update();
        self.clear_timer_flags();

        self.tim.dier().modify(|_, w| w.ccde(0).set_bit());
        self.tim.cr1().modify(|_, w| w.cen().set_bit());
    }

    pub fn stop_frame(&mut self) {
        self.tim.dier().modify(|_, w| w.ccde(0).clear_bit());
        self.tim.cr1().modify(|_, w| w.cen().clear_bit());

        self.write_ccr1(0);
        self.force_update();
        self.clear_timer_flags();
    }
}

unsafe impl PeriAddress for Tim1Ch1Dma {
    type MemSize = u16;

    #[inline]
    fn address(&self) -> u32 {
        self.tim.ccr(0).as_ptr() as u32
    }
}

unsafe impl DMASet<
    Stream1<pac::DMA2>,
    6,
    MemoryToPeripheral,
> for Tim1Ch1Dma
{
}
```

---

## 7. Safe ownership wrapper

```rust
use stm32f4xx_hal::{
    dma::{
        config::DmaConfig,
        traits::StreamISR,
        DMAError,
        MemoryToPeripheral,
        Priority,
        Stream1,
        Transfer,
    },
    gpio::{Alternate, PA8},
    pac,
    ClearFlags,
    ReadFlags,
};

use super::{
    protocol,
    stm32f405::Tim1Ch1Dma,
    timing::{encode_compare_sequence, Timing},
};

type Buffer = &'static mut [u16; 16];

type DmaTransfer = Transfer<
    Stream1<pac::DMA2>,
    6,
    Tim1Ch1Dma,
    MemoryToPeripheral,
    Buffer,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendError {
    Busy,
    Faulted,
    InvalidThrottle,
    InvalidValue,
    Dma,
}

pub struct DshotTx {
    _pin: PA8<Alternate<1>>,
    transfer: DmaTransfer,
    spare: Option<Buffer>,
    timing: Timing,
    busy: bool,
    faulted: bool,
}

impl DshotTx {
    pub fn new(
        pin: PA8<Alternate<1>>,
        stream: Stream1<pac::DMA2>,
        peripheral: Tim1Ch1Dma,
        active: Buffer,
        spare: Buffer,
        timing: Timing,
    ) -> Self {
        active.fill(0);
        spare.fill(0);

        let transfer = Transfer::init_memory_to_peripheral(
            stream,
            peripheral,
            active,
            None,
            DmaConfig::default()
                .priority(Priority::VeryHigh)
                .memory_increment(true)
                .peripheral_increment(false)
                .transfer_complete_interrupt(true)
                .transfer_error_interrupt(true)
                .direct_mode_error_interrupt(true)
                .fifo_error_interrupt(true),
        );

        Self {
            _pin: pin,
            transfer,
            spare: Some(spare),
            timing,
            busy: false,
            faulted: false,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn is_faulted(&self) -> bool {
        self.faulted
    }

    pub fn send_stop(&mut self) -> Result<(), SendError> {
        self.send_encoded(protocol::stop_frame())
    }

    pub fn send_throttle(
        &mut self,
        throttle: u16,
        telemetry: bool,
    ) -> Result<(), SendError> {
        let frame = protocol::throttle_frame(throttle, telemetry)
            .map_err(|_| SendError::InvalidThrottle)?;

        self.send_encoded(frame)
    }

    pub fn send_raw_value(
        &mut self,
        value: u16,
        telemetry: bool,
    ) -> Result<(), SendError> {
        let frame = protocol::frame(value, telemetry)
            .map_err(|_| SendError::InvalidValue)?;

        self.send_encoded(frame)
    }

    fn send_encoded(&mut self, frame: u16) -> Result<(), SendError> {
        if self.faulted {
            return Err(SendError::Faulted);
        }

        if self.busy {
            return Err(SendError::Busy);
        }

        let mut next = self.spare.take().ok_or(SendError::Faulted)?;

        let first_duty = encode_compare_sequence(
            frame,
            self.timing,
            &mut *next,
        );

        let old = match self.transfer.next_transfer(next) {
            Ok((old, _current_buffer)) => old,

            Err(DMAError::NotReady(buffer))
            | Err(DMAError::SmallBuffer(buffer))
            | Err(DMAError::Overrun(buffer)) => {
                self.spare = Some(buffer);
                self.faulted = true;
                return Err(SendError::Dma);
            }
        };

        self.spare = Some(old);
        self.busy = true;

        self.transfer
            .start(|tim| tim.start_frame(first_duty));

        Ok(())
    }

    pub fn on_dma_interrupt(&mut self) {
        let error =
            self.transfer.is_transfer_error()
                || self.transfer.is_direct_mode_error()
                || self.transfer.is_fifo_error();

        self.transfer.pause(Tim1Ch1Dma::stop_frame);
        self.transfer.clear_all_flags();

        self.busy = false;
        self.faulted |= error;
    }
}
```

The active buffer is never modified by application code. `Transfer` owns it until `next_transfer` returns it.

Avoid reaching into the DMA stream through `Transfer::stream()`, because that can invalidate the HAL's safety assumptions.

---

## 8. RTIC2 integration

```rust
#![no_std]
#![no_main]

use panic_halt as _;

#[rtic::app(
    device = stm32f4xx_hal::pac,
    dispatchers = [EXTI0]
)]
mod app {
    use cortex_m::singleton;

    use stm32f4xx_hal::{
        dma::StreamsTuple,
        gpio::Speed,
        pac,
        prelude::*,
        rcc::Config,
        timer::Timer,
    };

    use crate::dshot::{
        DshotTx,
        stm32f405::Tim1Ch1Dma,
        timing::DSHOT600_168MHZ,
    };

    #[shared]
    struct Shared {
        dshot: DshotTx,
    }

    #[local]
    struct Local {}

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let device = cx.device;

        let mut rcc = device.RCC.freeze(
            Config::hse(8.MHz())
                .sysclk(168.MHz())
                .hclk(168.MHz())
                .pclk1(42.MHz())
                .pclk2(84.MHz()),
        );

        let gpioa = device.GPIOA.split(&mut rcc);

        let dshot_pin = gpioa
            .pa8
            .into_alternate::<1>()
            .speed(Speed::VeryHigh);

        let tim1 = Timer::new(device.TIM1, &mut rcc).release();

        let dma2 = StreamsTuple::new(device.DMA2, &mut rcc);
        let dma_stream = dma2.1;

        let buffer_a = singleton!(: [u16; 16] = [0; 16])
            .expect("DSHOT DMA buffer A allocated twice");

        let buffer_b = singleton!(: [u16; 16] = [0; 16])
            .expect("DSHOT DMA buffer B allocated twice");

        let peripheral =
            Tim1Ch1Dma::new(tim1, DSHOT600_168MHZ.arr);

        let dshot = DshotTx::new(
            dshot_pin,
            dma_stream,
            peripheral,
            buffer_a,
            buffer_b,
            DSHOT600_168MHZ,
        );

        (Shared { dshot }, Local {})
    }

    #[task(priority = 2, shared = [dshot])]
    fn set_motor(mut cx: set_motor::Context, throttle: u16) {
        cx.shared.dshot.lock(|dshot| {
            let _result = dshot.send_throttle(throttle, false);
        });
    }

    #[task(
        binds = DMA2_STREAM1,
        priority = 5,
        shared = [dshot]
    )]
    fn dshot_dma_complete(
        mut cx: dshot_dma_complete::Context,
    ) {
        cx.shared.dshot.lock(|dshot| {
            dshot.on_dma_interrupt();
        });
    }
}
```

The DMA interrupt should have a higher RTIC priority than the task that submits new frames.

---

## 9. Actuator-layer integration

Do not let the flight controller directly convert zero throttle into DShot stop. Keep explicit actuator states:

```rust
pub enum MotorCommand {
    Disarmed,
    Idle,
    Throttle([u16; 4]),
}
```

For one motor:

```rust
match command {
    MotorCommand::Disarmed => {
        dshot.send_stop()?;
    }

    MotorCommand::Idle => {
        dshot.send_throttle(IDLE_THROTTLE, false)?;
    }

    MotorCommand::Throttle(values) => {
        dshot.send_throttle(values[0], false)?;
    }
}
```

During boot and disarmed operation, continuously send stop frames at the normal actuator update rate.

Some ESC firmware expects a sequence of stop frames before accepting throttle. The exact arming duration is ESC-dependent.

---

## 10. Supporting four motors

There are two reasonable designs.

### 10.1 Independent streams

Use one timer channel and one DMA stream per motor:

```text
TIM1_CH1 → motor 1
TIM1_CH2 → motor 2
TIM1_CH3 → motor 3
TIM1_CH4 → motor 4
```

Advantages:

- Straightforward ownership.
- Each motor has an independent DMA transaction.
- Easy fault isolation.
- Easy to extend toward bidirectional DShot.

Disadvantages:

- Four DMA streams.
- Four completion interrupts unless synchronized.

### 10.2 Timer DMA burst

Use the timer's DMAR/DCR burst mechanism to update CCR1 through CCR4 from an interleaved buffer:

```text
bit 0: CCR1, CCR2, CCR3, CCR4
bit 1: CCR1, CCR2, CCR3, CCR4
...
```

Advantages:

- One DMA stream.
- All motor channels update synchronously.
- Lower DMA-stream consumption.

Disadvantages:

- More difficult register sequencing.
- More tightly coupled to STM32 timer internals.
- The HAL abstraction is less ergonomic.
- Harder to prove and review.

For the first safe implementation, use four independent channel drivers. Introduce a dedicated four-channel timer-DMA-burst backend only after the single-channel implementation has been validated with a logic analyser.

---

## 11. Timing validation

Perform these tests with propellers removed.

### 11.1 Idle state

Before the first transmission and after DMA completion:

```text
PA8 = low
```

There must be no periodic PWM output while no frame is active.

### 11.2 DShot600 timing

Expected:

```text
bit period:  1.667 µs
logical 0:   0.625 µs high
logical 1:   1.250 µs high
```

### 11.3 Known test frame

Transmit raw DShot value 48 without telemetry:

```text
frame = 0x0606
bits  = 0000 0110 0000 0110
```

Verify all 16 bits on the logic analyser.

### 11.4 DMA completion

Check that:

```text
NDTR starts at 16
NDTR reaches 0
DMA2_STREAM1 interrupt occurs once
timer stops
line remains low
```

### 11.5 Jitter

Trigger repeatedly and measure pulse-width distributions.

Pulse widths should remain timer-quantized and independent of other RTIC task execution.

---

## 12. Unsafe-code assessment

This design contains three categories of `unsafe`:

| Unsafe use | Reason | Risk control |
|---|---|---|
| `PeriAddress` implementation | HAL needs proof of the CCR1 address and width | Own TIM1; fixed CCR1 address; `u16` width |
| `DMASet` implementation | HAL needs proof of the stream/channel route | Fixed STM32F405 DMA2 Stream1 Channel6 mapping |
| PAC `bits()` writes | Some generated register writers expose raw numeric values | Values are bounded `u16` timer fields or zero |

It does **not** require:

- Raw mutable static buffers.
- Aliasing a DMA-active buffer.
- Manual DMA memory addresses.
- Direct DMA stream enable or disable.
- Global `static mut`.
- Unsafe access from RTIC tasks.
- Interrupt masking around buffer mutation.

This is a small and auditable unsafe boundary: two hardware-contract trait implementations and a few bounded register writes, while DMA buffer ownership remains delegated to the HAL.

---

## 13. Recommended project structure

```text
src/
├── main.rs
└── dshot/
    ├── mod.rs
    ├── protocol.rs
    ├── timing.rs
    ├── stm32f405.rs
    └── tx.rs
```

Suggested responsibilities:

| Module | Responsibility |
|---|---|
| `protocol.rs` | DShot frame construction and CRC |
| `timing.rs` | Timer timing and compare-sequence generation |
| `stm32f405.rs` | STM32F405 TIM1/DMA endpoint |
| `tx.rs` | Safe transfer ownership and runtime state |
| `main.rs` | RTIC task and board-resource integration |

---

## 14. Recommended implementation sequence

1. Unit-test DShot frame encoding on the host.
2. Configure one TIM1 channel without DMA and verify fixed PWM timing.
3. Add the local CCR1 DMA endpoint.
4. Transmit known frames into a logic analyser.
5. Verify low output after every frame.
6. Add RTIC DMA completion handling.
7. Add explicit disarmed, idle, and throttle actuator states.
8. Extend to four independent motor channels.
9. Validate simultaneous updates and worst-case RTIC interrupt loading.
10. Consider timer DMA burst only after the independent implementation is stable.
