# FOXEER F405 V2 Hardware Map for FerroWasp

**Purpose:** Reverse-engineered board support reference for implementing the FOXEER F405 V2 flight controller in a Rust/RTIC FerroWasp target.

**Board target:** `FOXEERF405V2`  
**MCU:** STM32F405RGT6, Cortex-M4F, LQFP64, 1 MiB flash  
**External oscillator:** 8 MHz HSE  
**Recommended initial clock configuration:** 168 MHz SYSCLK with a valid 48 MHz USB clock

> This document is based primarily on the current Betaflight target definition and STM32F4 timer/DMA tables, cross-checked against the ArduPilot `FoxeerF405v2` hardware definition. Hardware revisions and component substitutions may exist, so sensor identity, orientation, output polarity, and power-control GPIOs must be verified on the physical board.

---

## 1. Board capabilities

The FOXEER F405 V2 exposes or contains:

- Eight motor-capable outputs
- One explicit servo pad
- One camera-control timer output
- One WS2812 LED-strip output
- Six hardware UARTs
- USB FS
- Three SPI buses
- One I2C bus
- SPI IMU with dedicated interrupt
- SPI blackbox flash
- SPI analog OSD
- Battery-voltage ADC
- Current-sensor ADC
- Analog RSSI ADC
- Buzzer output
- Camera-selection/PINIO output
- Two status LEDs sharing SWD pins

The board is well suited to a quadcopter-oriented FerroWasp target because the first four motor outputs are grouped on TIM1 and TIM8, while the IMU, flash, and OSD each have separate SPI buses.

---

## 2. MCU and clocks

| Property | Value |
|---|---|
| MCU family | STM32F4 |
| MCU device | STM32F405 |
| Package | LQFP64 |
| Internal flash | 1 MiB |
| HSE | 8 MHz |
| Maximum normal SYSCLK | 168 MHz |
| USB peripheral | USB OTG FS |
| FPU | Single-precision Cortex-M4F FPU |

A conventional clock tree is:

- HSE: 8 MHz
- PLL input: 1 MHz
- SYSCLK: 168 MHz
- APB1 peripheral clock: 42 MHz
- APB1 timer clock: 84 MHz
- APB2 peripheral clock: 84 MHz
- APB2 timer clock: 168 MHz
- USB clock: 48 MHz

Timer clocks therefore normally become:

| Timer group | Clock |
|---|---:|
| TIM1, TIM8, TIM9, TIM10, TIM11 | 168 MHz |
| TIM2, TIM3, TIM4, TIM5, TIM12, TIM13, TIM14 | 84 MHz |

---

## 3. Complete pin map

### 3.1 SPI1 — onboard IMU

| Signal | MCU pin | Alternate function | Notes |
|---|---:|---|---|
| SCK | PA5 | AF5, SPI1_SCK | |
| MISO | PA6 | AF5, SPI1_MISO | |
| MOSI | PA7 | AF5, SPI1_MOSI | |
| Chip select | PA4 | GPIO output | Active-low |
| Data-ready interrupt | PC4 | EXTI4 | Dedicated gyro/IMU interrupt |

Betaflight enables support for:

- MPU6000
- MPU6500
- ICM42688-P

The production board is commonly documented with an ICM42688-P, but firmware should probe or select the actual fitted sensor.

Recommended initial SPI settings:

- Mode 3
- Start at approximately 1 MHz during identification
- Increase only after reliable register access
- Use DMA for steady-state burst reads
- Use PC4/EXTI4 as the sampling trigger

---

### 3.2 SPI2 — blackbox flash

| Signal | MCU pin | Alternate function | Notes |
|---|---:|---|---|
| SCK | PB13 | AF5, SPI2_SCK | |
| MISO | PC2 | AF5, SPI2_MISO | |
| MOSI | PC3 | AF5, SPI2_MOSI | |
| Chip select | PB12 | GPIO output | Active-low |

Betaflight identifies the flash as an M25P16-compatible device class. ArduPilot describes the board as using SPI dataflash on SPI2.

Treat the exact flash density and JEDEC identity as runtime-detected hardware information.

---

### 3.3 SPI3 — analog OSD

| Signal | MCU pin | Alternate function | Notes |
|---|---:|---|---|
| SCK | PB3 | AF6, SPI3_SCK | Conflicts with SWO trace |
| MISO | PB4 | AF6, SPI3_MISO | Former JTAG pin |
| MOSI | PB5 | AF6, SPI3_MOSI | |
| Chip select | PC14 | GPIO output | Active-low |

The board uses a MAX7456/AT7456E-compatible analog OSD device.

Recommended initial configuration:

- SPI mode 0
- Conservative clock during initialization
- Do not assume SWO trace remains available once PB3 is configured for SPI3

---

### 3.4 I2C1 — barometer and external compass

| Signal | MCU pin | Alternate function |
|---|---:|---|
| SCL | PB8 | AF4, I2C1_SCL |
| SDA | PB9 | AF4, I2C1_SDA |

Betaflight enables:

- DPS310
- BMP280

ArduPilot additionally probes SPL06-compatible devices.

The bus is also intended for an external compass. There is no known onboard magnetometer.

---

### 3.5 USB

| Signal | MCU pin | Alternate function |
|---|---:|---|
| USB D− | PA11 | AF10, OTG_FS_DM |
| USB D+ | PA12 | AF10, OTG_FS_DP |

PA11 can also expose a TIM1 complementary output in the MCU pin matrix, but USB should take precedence on this board.

---

## 4. UART map

| UART | TX pin | RX pin | AF | Typical board use |
|---|---:|---:|---:|---|
| USART1 | PA9 | PA10 | AF7 | ESC telemetry / general serial |
| USART2 | PA2 | PA3 | AF7 | Receiver input |
| USART3 | PC10 | PC11 | AF7 | Analog VTX control |
| UART4 | PA0 | PA1 | AF8 | DJI/MSP DisplayPort |
| UART5 | PC12 | PD2 | AF8 | GPS |
| USART6 | PC6 | PC7 | AF8 | General serial / HD connector |

### 4.1 Receiver inversion

The board exposes an inverted SBUS input associated with USART2 RX.

Use:

- Inverted SBUS pad for conventional inverted SBUS
- Direct USART2 RX pad for CRSF, ELRS, FPort, and other non-inverted protocols

The exact external inverter topology should be verified electrically before relying on bidirectional half-duplex behavior.

---

## 5. ADC inputs

All three analog inputs use ADC1.

| Function | MCU pin | ADC channel |
|---|---:|---|
| Battery voltage | PC0 | ADC1_IN10 |
| Current sensor | PC1 | ADC1_IN11 |
| Analog RSSI | PC5 | ADC1_IN15 |

Betaflight selects ADC1 DMA option 1:

- DMA2
- Stream 4
- Channel 0

ArduPilot uses approximate board-level scaling values:

- Battery divider scale: approximately 11.0
- Current scale: approximately 142.9 in ArduPilot's units

Betaflight uses a default current-meter scale of 70. These values use different conventions and must not be copied directly into FerroWasp without calibration.

Recommended approach:

1. Store raw ADC counts.
2. Define board-specific divider and current-transfer parameters.
3. Calibrate voltage against a trusted multimeter.
4. Calibrate current against a known load or calibrated power analyzer.
5. Keep raw, calibrated, and filtered values distinct.

---

## 6. GPIO functions

| Function | MCU pin | Recommended mode | Notes |
|---|---:|---|---|
| Buzzer | PC15 | Push-pull output | Verify active polarity |
| Camera control | PB0 | Timer output or GPIO | Shared with TIM3_CH3 |
| Camera selector / PINIO1 | PB14 | Push-pull output | Betaflight `PINIO1` |
| Status LED 0 | PA13 | GPIO output | Conflicts with SWDIO |
| Status LED 1 | PA14 | GPIO output | Conflicts with SWCLK |
| IMU CS | PA4 | Push-pull output | Default high |
| Flash CS | PB12 | Push-pull output | Default high |
| OSD CS | PC14 | Push-pull output | Default high |

### 6.1 Debug-port conflict

PA13 and PA14 are the SWD debug pins.

During FerroWasp development:

- Keep PA13 as SWDIO
- Keep PA14 as SWCLK
- Do not configure the onboard status LEDs
- Use RTT, defmt, or another UART for diagnostics

PB3 is the SWO pin, but it is required by SPI3 SCK. Therefore:

- SWD remains usable
- SWO trace is unavailable when SPI3 is active

### 6.2 Low-speed crystal conflict

PC14 and PC15 are normally capable of LSE oscillator use, but this board uses them for:

- PC14: OSD chip select
- PC15: buzzer

Do not expect an external 32.768 kHz LSE crystal to be available.

---

## 7. Timer and output map

### 7.1 Motor outputs

| Output | Pin | Timer channel | GPIO AF | Betaflight DMA selection |
|---|---:|---|---:|---|
| M1 | PA8 | TIM1_CH1 | AF1 | DMA2 Stream1 Channel6 |
| M2 | PC9 | TIM8_CH4 | AF3 | DMA2 Stream7 Channel7 |
| M3 | PC8 | TIM8_CH3 | AF3 | DMA2 Stream2 Channel0 |
| M4 | PB15 | TIM1_CH3N | AF1 | DMA2 Stream6 Channel6 |
| M5 | PB6 | TIM4_CH1 | AF2 | DMA1 Stream0 Channel2 |
| M6 | PA15 | TIM2_CH1 | AF1 | DMA1 Stream5 Channel3 |
| M7 | PB11 | TIM2_CH4 | AF1 | DMA1 Stream7 Channel3 |
| M8 | PB10 | TIM2_CH3 | AF1 | DMA1 Stream1 Channel3 |

### 7.2 Auxiliary timer outputs

| Function | Pin | Timer channel | GPIO AF | DMA |
|---|---:|---|---:|---|
| Servo 1 | PB1 | TIM3_CH4 | AF2 | DMA1 Stream2 Channel5 |
| Camera control | PB0 | TIM3_CH3 | AF2 | DMA1 Stream7 Channel5 |
| LED strip | PB7 | TIM4_CH2 | AF2 | DMA1 Stream3 Channel2 |

---

## 8. Natural timer groups

Outputs sharing the same timer also share:

- Timer counter
- Prescaler
- Auto-reload period
- Update event
- Timer base frequency
- Some DMA/update behavior

The FOXEER F405 V2 groups are:

| Timer | Outputs |
|---|---|
| TIM1 | M1, M4 |
| TIM8 | M2, M3 |
| TIM4 | M5, LED strip |
| TIM2 | M6, M7, M8 |
| TIM3 | Servo 1, camera control |

This grouping matters when mixing protocols.

Examples:

- M1 and M4 should normally use the same motor protocol and update rate.
- M2 and M3 should normally use the same motor protocol and update rate.
- LED-strip waveform generation on TIM4 may conflict with running M5 from the same timer.
- Servo timing on TIM3 may conflict with timer-driven camera-control waveforms.

---

## 9. Betaflight timer-table interpretation

Betaflight's target entries use:

```c
TIMER_PIN_MAP(index, pin, occurrence, dma_option)
```

The `occurrence` field selects the nth matching timer function for that pin in Betaflight's complete STM32F4 timer table.

Examples:

```c
TIMER_PIN_MAP(0, PA8,  1, 1)
TIMER_PIN_MAP(1, PC9,  2, 0)
TIMER_PIN_MAP(3, PB15, 1, 1)
```

resolve to:

- PA8, first timer occurrence → TIM1_CH1
- PC9, second timer occurrence → TIM8_CH4
- PB15, first timer occurrence → TIM1_CH3N

The final number chooses one of the valid DMA mappings for the resolved timer channel.

---

## 10. Important discrepancy: M8 is absent from Betaflight's timer map

Betaflight declares:

```c
#define MOTOR8_PIN PB10
```

but PB10 is absent from the target's `TIMER_PIN_MAPPING` list.

The STM32F405 alternate-function table and ArduPilot hardware definition both identify:

```text
PB10 = TIM2_CH3
```

Therefore, FerroWasp should explicitly define M8 as:

| Property | Value |
|---|---|
| Pin | PB10 |
| Timer | TIM2 |
| Channel | CH3 |
| Alternate function | AF1 |
| DMA | DMA1 Stream1 Channel3 |

This appears to be a Betaflight target omission or limitation, not a physical board limitation.

Do not assume that every pad declared as `MOTORx_PIN` is also present in Betaflight's timer-management table.

---

## 11. M4 complementary-output hazard

M4 is not a normal positive timer output.

```text
PB15 = TIM1_CH3N
```

`CH3N` is the complementary output associated with TIM1 channel 3.

A driver must:

1. Write the waveform timing to `TIM1.CCR3`.
2. Enable `CC3NE`, not `CC3E`.
3. Configure complementary-output polarity.
4. Set the advanced-timer main-output-enable bit, `BDTR.MOE`.
5. Configure idle and break behavior.
6. Guarantee a safe GPIO state before switching PB15 to the timer alternate function.
7. Disable the complementary output explicitly during disarm and fault handling.

This is likely the most important special case for a generic FerroWasp motor-output abstraction.

A Rust HAL API that only exposes ordinary `CH3` PWM may not automatically support `CH3N`. The target implementation may need a specialized safe wrapper around TIM1's complementary channel functionality.

---

## 12. STM32F405 timer DMA map used by this board

### TIM1

| Channel | DMA options |
|---|---|
| TIM1_CH1 | DMA2 S6 C0; DMA2 S1 C6; DMA2 S3 C6 |
| TIM1_CH2 | DMA2 S6 C0; DMA2 S2 C6 |
| TIM1_CH3 | DMA2 S6 C0; DMA2 S6 C6 |
| TIM1_CH4 | DMA2 S4 C6 |

Selected by this board:

- M1 TIM1_CH1 → option 1 → DMA2 S1 C6
- M4 TIM1_CH3/CH3N → option 1 → DMA2 S6 C6

### TIM8

| Channel | DMA options |
|---|---|
| TIM8_CH1 | DMA2 S2 C0; DMA2 S2 C7 |
| TIM8_CH2 | DMA2 S2 C0; DMA2 S3 C7 |
| TIM8_CH3 | DMA2 S2 C0; DMA2 S4 C7 |
| TIM8_CH4 | DMA2 S7 C7 |

Selected by this board:

- M3 TIM8_CH3 → option 0 → DMA2 S2 C0
- M2 TIM8_CH4 → option 0 → DMA2 S7 C7

### TIM2

| Channel | DMA options |
|---|---|
| TIM2_CH1 | DMA1 S5 C3 |
| TIM2_CH2 | DMA1 S6 C3 |
| TIM2_CH3 | DMA1 S1 C3 |
| TIM2_CH4 | DMA1 S7 C3; DMA1 S6 C3 |

### TIM3

| Channel | DMA options |
|---|---|
| TIM3_CH1 | DMA1 S4 C5 |
| TIM3_CH2 | DMA1 S5 C5 |
| TIM3_CH3 | DMA1 S7 C5 |
| TIM3_CH4 | DMA1 S2 C5 |

### TIM4

| Channel | DMA options |
|---|---|
| TIM4_CH1 | DMA1 S0 C2 |
| TIM4_CH2 | DMA1 S3 C2 |
| TIM4_CH3 | DMA1 S7 C2 |

---

## 13. Peripheral DMA map

### 13.1 SPI

| Peripheral | Direction | DMA options |
|---|---|---|
| SPI1 | TX | DMA2 S3 C3; DMA2 S5 C3 |
| SPI1 | RX | DMA2 S0 C3; DMA2 S2 C3 |
| SPI2 | TX | DMA1 S4 C0 |
| SPI2 | RX | DMA1 S3 C0 |
| SPI3 | TX | DMA1 S5 C0; DMA1 S7 C0 |
| SPI3 | RX | DMA1 S0 C0; DMA1 S2 C0 |

### 13.2 ADC

| Peripheral | DMA options |
|---|---|
| ADC1 | DMA2 S0 C0; DMA2 S4 C0 |

The Betaflight target selects option 1:

```text
ADC1 → DMA2 Stream4 Channel0
```

### 13.3 UART

| UART | TX DMA | RX DMA |
|---|---|---|
| USART1 | DMA2 S7 C4 | DMA2 S5 C4 or DMA2 S2 C4 |
| USART2 | DMA1 S6 C4 | DMA1 S5 C4 |
| USART3 | DMA1 S3 C4 | DMA1 S1 C4 |
| UART4 | DMA1 S4 C4 | DMA1 S2 C4 |
| UART5 | DMA1 S7 C4 | DMA1 S0 C4 |
| USART6 | DMA2 S6 C5 or DMA2 S7 C5 | DMA2 S1 C5 or DMA2 S2 C5 |

---

## 14. Recommended DMA allocation for an initial quad

A practical M1–M4 allocation is:

| Function | DMA resource |
|---|---|
| SPI1 IMU RX | DMA2 Stream0 Channel3 |
| M1 DShot | DMA2 Stream1 Channel6 |
| M3 DShot | DMA2 Stream2 Channel0 |
| SPI1 IMU TX | DMA2 Stream3 Channel3 |
| ADC1 scan | DMA2 Stream4 Channel0 |
| USART1 RX / ESC telemetry | DMA2 Stream5 Channel4 |
| M4 DShot | DMA2 Stream6 Channel6 |
| M2 DShot | DMA2 Stream7 Channel7 |

This consumes every DMA2 stream.

### Consequences

- USART6 DMA becomes unavailable while all four motor streams are active.
- USART1 TX conflicts with M2.
- USART1 RX can still use DMA2 Stream5.
- USART2 RX remains available on DMA1 Stream5 until M6 is enabled.
- SPI2 blackbox can use DMA1 Streams3 and 4.
- UART4 TX conflicts with SPI2 TX on DMA1 Stream4.
- UART3 RX conflicts with M8 on DMA1 Stream1.
- UART5 RX conflicts with M5 on DMA1 Stream0.
- UART5 TX conflicts with M7 on DMA1 Stream7.

For a quadcopter this is manageable. For eight motors, DMA assignment becomes a board-level scheduling problem.

---

## 15. Recommended initial FerroWasp feature allocation

### Core flight functions

| Function | Peripheral |
|---|---|
| IMU | SPI1 + EXTI4 |
| Motors 1–4 | TIM1 + TIM8, four DMA2 streams |
| Receiver | USART2 RX, preferably DMA1 Stream5 |
| Battery/current/RSSI | ADC1 + DMA2 Stream4 |
| Debug | SWD + RTT |
| USB | OTG FS |

### Secondary functions

| Function | Peripheral |
|---|---|
| Blackbox flash | SPI2 |
| Analog OSD | SPI3 |
| Barometer | I2C1 |
| GPS | UART5 |
| DJI DisplayPort | UART4 |
| ESC telemetry | USART1 RX |
| Additional general serial | USART3 or USART6 |

Do not attempt to enable every peripheral simultaneously during the first board bring-up.

---

## 16. Recommended Rust board-manifest representation

A board target should describe physical truth without embedding application policy.

Conceptually:

```rust
pub struct FoxeerF405V2;

pub const CLOCKS: ClockManifest = ClockManifest {
    hse_hz: 8_000_000,
    sysclk_hz: 168_000_000,
    usb_required: true,
};

pub const IMU: SpiDeviceManifest = SpiDeviceManifest {
    bus: SpiBusId::Spi1,
    sck: Pin::PA5,
    miso: Pin::PA6,
    mosi: Pin::PA7,
    cs: Pin::PA4,
    mode: SpiMode::Mode3,
    interrupt: Some(ExtiPin::PC4),
};

pub const MOTOR_OUTPUTS: &[MotorOutputManifest] = &[
    MotorOutputManifest::normal(
        MotorId::M1,
        Pin::PA8,
        Timer::Tim1,
        TimerChannel::Ch1,
        Dma::new(DmaController::Dma2, 1, 6),
    ),
    MotorOutputManifest::normal(
        MotorId::M2,
        Pin::PC9,
        Timer::Tim8,
        TimerChannel::Ch4,
        Dma::new(DmaController::Dma2, 7, 7),
    ),
    MotorOutputManifest::normal(
        MotorId::M3,
        Pin::PC8,
        Timer::Tim8,
        TimerChannel::Ch3,
        Dma::new(DmaController::Dma2, 2, 0),
    ),
    MotorOutputManifest::complementary(
        MotorId::M4,
        Pin::PB15,
        Timer::Tim1,
        TimerChannel::Ch3,
        ComplementaryChannel::Ch3N,
        Dma::new(DmaController::Dma2, 6, 6),
    ),
];
```

The exact Rust types will depend on the FerroWasp board-generation architecture, but the manifest should explicitly represent:

- Pin
- Alternate function
- Timer
- Timer channel
- Complementary versus normal output
- DMA controller
- DMA stream
- DMA channel
- Shared-timer group
- Safe inactive state
- Peripheral conflicts
- Whether a feature is fitted or merely supported by a target driver

---

## 17. RTIC resource recommendations

### High-priority hardware tasks

Potential structure:

```rust
#[task(
    binds = EXTI4,
    priority = 15,
    local = [imu_exti],
    shared = [imu_wakeup]
)]
fn imu_data_ready(cx: imu_data_ready::Context) {
    // Clear EXTI and schedule/start the IMU DMA transfer.
}

#[task(
    binds = DMA2_STREAM0,
    priority = 14,
    local = [imu_rx_dma],
    shared = [imu_samples]
)]
fn imu_dma_complete(cx: imu_dma_complete::Context) {
    // Finalize sample and hand it to the control pipeline.
}

#[task(
    binds = DMA2_STREAM1,
    priority = 13,
    local = [motor_1_dma]
)]
fn motor_1_dma_complete(cx: motor_1_dma_complete::Context) {
    // Mark output buffer reusable.
}
```

The exact interrupt names and architecture depend on the HAL and whether the motor implementation uses:

- Per-channel DMA
- Timer-update DMA burst
- Multi-channel timer DMA
- DShot bit-banging
- Conventional PWM without DMA

### Recommended ownership boundaries

- SPI1 and IMU DMA owned by the IMU service
- TIM1 and TIM8 owned by the actuator-output service
- ADC1 and its DMA owned by the analog-sampling service
- USART2 RX owned by the receiver-input service
- Flash owned by the logging service
- Only the actuator-output service may alter timer outputs connected to motors

---

## 18. Safe motor initialization

The board target should initialize motor pins in a defined safe sequence.

Recommended sequence:

1. Enable GPIO clocks.
2. Configure all motor pins as ordinary push-pull outputs.
3. Drive all motor pins low.
4. Configure timer base registers while outputs remain disconnected.
5. Configure DMA buffers with all-low/disarmed waveforms.
6. Configure TIM1/TIM8 output polarity and idle states.
7. Configure TIM1 complementary-channel behavior for M4.
8. Set advanced-timer break/dead-time registers explicitly.
9. Switch GPIO pins to timer alternate functions.
10. Enable timer outputs only after the safety state machine permits actuator operation.

For TIM1, do not set `BDTR.MOE` until every channel is configured safely.

---

## 19. Sensor orientation

Betaflight specifies:

```text
GYRO_1_ALIGN = CW270_DEG
```

ArduPilot uses:

```text
ROTATION_PITCH_180_YAW_90
```

These names use different coordinate and sensor-driver conventions.

Do not convert them by name alone.

Required bench tests:

1. Stationary board:
   - Gravity must point along FerroWasp's expected down axis.

2. Nose raised:
   - Pitch sign must match the controller convention.

3. Right side lowered:
   - Roll sign must match the controller convention.

4. Clockwise yaw:
   - Gyro Z sign must match the controller convention.

5. Compare the board silkscreen arrow with the firmware body-frame definition.

Motor actuation must remain disabled until these signs are verified.

---

## 20. Recommended bring-up sequence

### Phase 1 — board identity and debug

- Preserve SWD pins
- Verify MCU identification
- Verify internal flash size
- Start from HSI
- Bring up 8 MHz HSE
- Configure 168 MHz SYSCLK
- Verify a stable monotonic timer
- Verify USB clock if USB is enabled
- Establish RTT/defmt logging

### Phase 2 — GPIO safety

- Set all chip selects high
- Set all motor outputs low
- Set buzzer inactive
- Set camera-control outputs inactive
- Leave status LEDs unconfigured while debugging

### Phase 3 — IMU

- Probe SPI1
- Read `WHO_AM_I`
- Identify fitted IMU
- Configure conservative SPI speed
- Verify PC4 EXTI
- Log raw samples
- Verify sensor orientation
- Measure interrupt timing and jitter

### Phase 4 — conventional motor PWM

- Configure TIM1 and TIM8
- Validate PA8, PC8, and PC9 first
- Validate PB15/TIM1_CH3N separately
- Use an oscilloscope or logic analyzer
- Verify disarmed low state
- Verify no pulses during reset or panic

### Phase 5 — DShot

- Add DMA waveforms
- Verify each stream independently
- Verify synchronized update timing
- Add explicit M4 complementary-output support
- Test disarm packets before throttle packets
- Measure waveform timing at the physical ESC connector

### Phase 6 — receiver and ADC

- Bring up direct USART2 RX
- Add CRSF or SBUS as required
- Configure ADC1 scan mode
- Add voltage/current calibration
- Verify RSSI input range

### Phase 7 — secondary devices

- I2C barometer
- SPI2 flash
- SPI3 OSD
- GPS
- ESC telemetry
- DJI DisplayPort

---

## 21. Minimum viable FerroWasp support

A useful first target does not need every board feature.

Recommended MVP:

- STM32F405 clock initialization
- SWD/RTT diagnostics
- SPI1 IMU
- PC4 IMU interrupt
- M1–M4 PWM
- M1–M4 DShot
- USART2 receiver
- ADC battery voltage
- ADC current sensing
- USB or UART configuration interface
- Basic fault-safe disarm behavior

Defer initially:

- M5–M8
- Analog OSD
- Camera control
- Camera switching
- LED strip
- Buzzer patterns
- Analog RSSI
- Full blackbox logging
- Every UART simultaneously

---

## 22. Verification checklist

### Electrical

- [ ] Confirm MCU marking
- [ ] Confirm HSE frequency
- [ ] Confirm fitted IMU
- [ ] Confirm flash JEDEC ID
- [ ] Confirm barometer identity
- [ ] Confirm OSD identity
- [ ] Confirm buzzer polarity
- [ ] Confirm camera-selection polarity
- [ ] Confirm SBUS inversion topology
- [ ] Confirm voltage-divider ratio
- [ ] Confirm current-sensor transfer ratio

### Pin mux

- [ ] PA5/PA6/PA7 operate as SPI1
- [ ] PC4 generates EXTI4
- [ ] PB13/PC2/PC3 operate as SPI2
- [ ] PB12 remains high when idle and selects only the onboard flash
- [ ] PB3/PB4/PB5 operate as SPI3
- [ ] PB8/PB9 operate as I2C1
- [ ] PA11/PA12 operate as USB FS
- [ ] SWD remains functional

### Motor outputs

- [ ] M1 PA8 produces expected waveform
- [ ] M2 PC9 produces expected waveform
- [ ] M3 PC8 produces expected waveform
- [ ] M4 PB15 produces expected waveform
- [ ] M4 uses CH3N correctly
- [ ] Motors remain inactive during reset
- [ ] Motors remain inactive during panic
- [ ] Motors remain inactive before arming
- [ ] DShot timing is within tolerance

### DMA

- [ ] No two enabled peripherals own the same DMA stream
- [ ] DMA interrupt priorities are explicit
- [ ] Error flags are handled
- [ ] Buffers are not mutated while owned by DMA
- [ ] Stream disable/clear ordering follows STM32F4 requirements
- [ ] ADC and motor DMA coexist as intended
- [ ] Receiver DMA does not conflict with enabled outputs

---

## 23. Source references

### Betaflight target

- Repository: `betaflight/config`
- File: `configs/FOXEERF405V2/config.h`
- Target name: `FOXEERF405V2`

### Betaflight STM32F4 timer database

- Repository: `betaflight/betaflight`
- File: `src/platform/STM32/timer_stm32f4xx.c`

### Betaflight timer-map interpretation

- Repository: `betaflight/betaflight`
- File: `src/main/pg/timerio.c`

### Betaflight DMA request map

- Repository: `betaflight/betaflight`
- File: `src/platform/STM32/dma_reqmap_mcu.c`

### ArduPilot cross-check

- Repository: `ArduPilot/ardupilot`
- File: `libraries/AP_HAL_ChibiOS/hwdef/FoxeerF405v2/hwdef.dat`

---

## 24. Summary

The FOXEER F405 V2 is a strong candidate for FerroWasp support.

Its main strengths are:

- Dedicated SPI bus and interrupt for the IMU
- Separate SPI buses for flash and OSD
- Six UARTs
- Four primary motor outputs distributed across TIM1 and TIM8
- Onboard voltage/current sensing
- Existing Betaflight and ArduPilot hardware definitions for cross-checking

Its main implementation hazards are:

1. M4 uses the complementary TIM1_CH3N output.
2. Betaflight omits M8/PB10 from its timer-management table.
3. The STM32F405 DMA matrix is highly constrained.
4. PA13 and PA14 status LEDs conflict with SWD.
5. PB3 SPI3 SCK conflicts with SWO trace.
6. Sensor-orientation names differ between firmware projects.
7. Octocopter support requires explicit global DMA allocation.

The recommended first implementation is a safe M1–M4 quad target using SPI1 IMU DMA, USART2 receiver input, ADC1 monitoring, and a strictly isolated actuator-output service.
