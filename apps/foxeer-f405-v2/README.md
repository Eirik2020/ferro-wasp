# Foxeer F405 V2 Flight App

This isolated RTIC 2 application targets the Foxeer F405 V2. It is based on
the validated FerroWasp FCU3 task wiring but owns a separate board contract,
Cargo graph, linker configuration, and binary.

Implemented board subset:

- 8 MHz HSE and 168 MHz system clock;
- SPI1 mode-3 IMU identity probe on PA4-PA7;
- runtime-selected MPU6500 `WHO_AM_I=0x70` or ICM42688-P
  `WHO_AM_I=0x47` configuration and DMA sampling;
- USART2 SBUS receiver path on PA2/PA3;
- UART4 DJI MSP DisplayPort path on PA0/PA1;
- ADC1 battery/current observation on PC0/PC1;
- four conventional 400 Hz RC PWM outputs on PA8, PC9, PC8, and PB15;
- gated four-lane DShot600 on those same outputs;
- optional BLHeli legacy telemetry RX on PA10 / USART1;
- optional read-only USB CDC diagnostics on PA11/PA12;
- opt-in 16 MiB-class SPI2 NOR storage on PB12/PB13/PC2/PC3;
- SWD/RTT diagnostics with PA13/PA14 left untouched.

M4 is the complementary `TIM1_CH3N` output. The BSP configures it explicitly;
it must be validated separately on the physical board.

Flight arming is intentionally inhibited until all of these are confirmed:

- fitted IMU identity;
- FerroWasp body-axis orientation and signs;
- target validation of PC4 IMU data-ready polarity, rate, and EXTI4-driven
  sample triggering;
- ADC voltage/current calibration;
- logical-to-physical motor order;
- M4 complementary-output polarity and waveform.

### Props-off actuator validation

The compile-time `bench_actuator_validation` gate permits the board-readiness
checks that would otherwise be blocked by the flight-arming inhibit. It must be
combined with a capped equal-motor, physical-motor, or logical-motor bench
feature; it cannot build a normal PID/mixer flight image. For example:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features "bench_actuator_validation bench_motor1_only"
```

Use `bench_motor1_only` through `bench_motor4_only` for physical output
identification, `bench_logical_motor1_only` through
`bench_logical_motor4_only` to exercise the provisional logical remap, or
`bench_equal_motors` for a capped equal-output check. At most one physical or
logical motor selector may be enabled. The normal RC qualification, low-stick
arming guard, arm-high recovery latch, safety-owned actuator task, fresh motor
command requirement, RC-loss/disarm behavior, and 250-command bench cap remain
active.

This is a commissioning mode, not a flight-ready setting. It applies the
existing all-motor PWM idle stage during arming before the selected/capped
command begins, so every motor must be treated as potentially live. Remove
propellers and verify the selected feature string before powering ESCs.

### Prepared DShot600 commissioning image

The Foxeer DShot backend is opt-in and requires both commissioning gates:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features "dshot bench_actuator_validation bench_equal_motors"
```

It owns TIM1/TIM8 and DMA2 Streams 1, 7, 2, and 6 as one synchronized fault
domain. The 500 Hz actuator service continuously emits frames, enforces the
bounded nonzero-command lease, and requests disarm on lease expiry, DMA fault,
spurious completion, or frame timeout. The normal Foxeer build remains PWM.
Do not flash this image until the PWM test has established the four physical
outputs and M4 polarity.

After the basic DShot image has passed, enable observational legacy telemetry:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features "dshot esc_telemetry bench_actuator_validation bench_equal_motors"
```

The bounded ESC-manager task rotates requests across physical outputs M1-M4,
parses the 115,200-baud BLHeli frames received on PA10, and prints eRPM plus
parser/request statistics to RTT. Only the actuator-owned DShot service can
set a frame's telemetry-request bit; it returns a sequenced acknowledgement to
the manager after the request is actually emitted. For initial commissioning,
telemetry is deliberately observational and does not qualify arming. This
keeps a missing/miswired telemetry lead from obscuring the independent DShot
waveform and motor-order tests. A request/response-association timeout latches
telemetry off until reboot without bypassing actuator safety.

### Required bench order

Keep propellers removed throughout this sequence and begin each powered test
with the arm control low:

1. Run the default image with ESC power disconnected. Confirm IMU identity,
   RC qualification, ADC raw readings, heartbeat, and no repeated transport
   faults.
2. Power the ESCs and build each `bench_motorN_only` PWM image in turn. Confirm
   physical outputs 1-4, especially M4's complementary polarity, then verify
   immediate stop on disarm and RC loss.
3. Run `bench_equal_motors` with PWM and confirm all four start and track the
   capped command evenly.
4. Run the gated DShot command above first unpowered, then with ESC power.
   Confirm synchronized lane counters, zero DMA/frame faults, the same motor
   order, and immediate stop behavior.
5. Only after DShot passes, add `esc_telemetry`. Power the FC and ESCs together
   so telemetry is available before the manager's five-second boot delay ends.
   Confirm each M1-M4 eRPM rises from zero at idle and follows throttle. Also
   confirm CRC/discard counts remain stable and telemetry returns to zero after
   disarm.

Do not remove the Foxeer flight-arming inhibit from the BSP profile merely
because these commissioning images run. Record the observed IMU orientation,
ADC scale values, motor order/direction, M4 waveform, DShot timing/fault counts,
and telemetry identity first; those measurements are the input to the separate
flight-readiness change.

An unsupported or failed IMU identity/configuration is nonfatal: firmware
logs the result once, disables periodic IMU transactions, and continues the
RTT/RC/OSD/ADC bring-up paths. MPU6000 is not yet implemented.

Foxeer IMU sampling is now driven by PC4/EXTI4 rather than the TIM4 poll
trigger. Both supported drivers configure an active-high, push-pull data-ready
pulse; the EXTI handler timestamps the edge, clears it, and defers one bounded
SPI DMA request without doing blocking bus work. The heartbeat reports total
and two-second-delta IRQ and rejected-trigger counts. The existing transaction
deadline, stale-sample detection, and timer-driven 400 Hz control cadence remain
intact. Interrupt polarity/rate and zero-or-bounded rejection behavior still
require target validation before Foxeer flight arming is enabled.

The RC PWM implementation uses a board-local TIM1/TIM8 owner because M4 is
the complementary `TIM1_CH3N` output. The DShot implementation uses the same
pins through board-declared timer/DMA routes and remains commissioning-gated.
M5-M8, analog OSD, I2C barometer, buzzer, camera
control, LED strip, and additional UARTs are not part of this application.

## Build

```powershell
cd apps/foxeer-f405-v2
cargo build --release --locked
```

See `flash-dfu.ps1` for the USB DFU build/flash sequence. Do not use the FCU3
binary on this board.

## SWD and RTT

The BSP permanently reserves PA13 for SWDIO and PA14 for SWCLK. It does not
claim the board LEDs that share those MCU signals. Connect the debugger's
SWDIO, SWCLK, target-reference voltage, and ground; connect NRST as well when
the retrofit exposes it. The debugger must use the board voltage only as a
logic-level reference and must not back-power an otherwise unpowered flight
controller unless the debugger and wiring are explicitly designed for that.

From the repository root, build, flash, reset, and stream decoded RTT with:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
```

This selects the Foxeer app and `FerroWaspFoxeerF405V2` ELF explicitly. Keep
the flight battery and ESC power disconnected for the first SWD session. USB
DFU remains available as a recovery path.

From this app directory, `cargo run --release --locked` also uses
`probe-rs run` over SWD. It does not invoke the DFU runner.

## USB Debug And Onboard Storage

The opt-in `usb_serial` feature exposes a CDC ACM device named
`FerroWasp Foxeer Debug`. It writes a header after enumeration and one bounded
ASCII status line with each roughly two-second firmware heartbeat:

```text
FWDBG1 ms=12345 imu=icm42688p ready=1 seq=9876 gyro=-17,4,-70 stale=0 ctl=4938 rc=1 armable=1 thr=1000 arm_sw=0 armed=0 vbat_dV=230 current_cA=-12 adc_v_mV=2091 adc_i_mV=1234
```

The fields report uptime, selected IMU and transport state, raw gyro and IMU
sequence, control sequence, RC qualification/throttle/arm switch, system arm
state, pack voltage in decivolts, and current in centiamps. The stream is
read-only. `adc_v_mV` and `adc_i_mV` are the pre-scale ADC observations used
for Foxeer voltage/current calibration. Received USB bytes are drained and
ignored, and the USB task owns
no safety or actuator handle.

Build the diagnostic image without flashing:

```powershell
.\flash-dfu.ps1 -BuildOnly -UsbDebug
```

After flashing and normal boot, find the new Windows COM port and read it:

```powershell
.\read-usb-debug.ps1 -Port COM7
```

For a bounded five-minute capture that closes the port automatically:

```powershell
.\read-usb-debug.ps1 -Port COM7 -DurationSeconds 300
```

The nominal 115200 baud value is USB CDC line coding; USB transfer timing does
not depend on a physical UART baud clock.

The staged onboard-storage features extend the same CDC endpoint with bounded
ASCII commands:

- `flash_storage` probes JEDEC identity and permits read-only log/config access;
- `flash_writes` adds disarmed-only configuration saves, log erase, and a
  dedicated scratch-sector erase/program/readback self-test;
- `flash_blackbox` records fixed-size CRC-protected control snapshots while
  armed and flushes the final partial page on disarm.

SPI2 runs in mode 0 at 10 MHz using short CPU-driven transfers. This avoids
the fixed DMA1 Stream 4 collision between SPI2 TX and the validated UART4 OSD
TX route. The priority-1 flash manager uses bounded queues, never owns motor
hardware, rejects destructive commands while armed, and aborts maintenance if
the system arms.

The first two 4 KiB sectors are copy-on-write configuration slots, the third
is reserved for the destructive self-test, and logs begin at `0x3000`.
Configuration input is limited to the PID gains, IMU LPF alpha, and log-rate
divisor already constrained by firmware ranges. A foreign/non-FerroWasp log
region stays read-only until an explicit confirmed erase.

Build the three stages from the repository root:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features flash_storage
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features flash_writes
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features flash_blackbox
```

Normal Foxeer flight arming remains inhibited, so the initial props-off armed
log capture must use the existing capped PWM commissioning gate:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features "flash_blackbox bench_actuator_validation bench_equal_motors"
```

With the enumerated COM port in a second terminal:

```powershell
python tools\ferrowasp_storage.py --port COM7 info
python tools\ferrowasp_storage.py --port COM7 test --confirm
python tools\ferrowasp_storage.py --port COM7 list
python tools\ferrowasp_storage.py --port COM7 read --output logs\foxeer-bench.fwbb
python tools\blackbox_analyzer.py logs\foxeer-bench.fwbb --mode rest
```

The self-test alters only the reserved scratch sector. `erase --confirm`
erases the whole FerroWasp log region and may take several minutes. These
paths are implemented but not yet target-validated; keep flight arming
inhibited and follow the storage checklist before relying on them.

## USB DFU

The dedicated DFU scripts use STM32CubeProgrammer to write the Cargo-generated
ELF directly. The runner searches `PATH`, the normal
STM32CubeProgrammer installation directories, and STM32CubeCLT under `C:\ST`.
Override discovery when needed with:

```powershell
$env:STM32_PROGRAMMER_CLI = "C:\path\to\STM32_Programmer_CLI.exe"
```

Enter the STM32F405 factory ROM bootloader:

1. Remove propellers and disconnect the flight battery.
2. Disconnect USB.
3. Hold the board's BOOT button.
4. Connect USB, wait one second, and release BOOT.

Confirm that CubeProgrammer sees a device such as `USB1`:

```powershell
.\tools\dfu-runner.ps1 -ListOnly
```

Build and flash the normal image through the explicit DFU helper:

```powershell
.\flash-dfu.ps1
```

Build and flash with read-only USB diagnostics:

```powershell
.\flash-dfu.ps1 -UsbDebug
```

If multiple STM32 DFU devices are connected, select one before running Cargo:

```powershell
$env:STM32_DFU_PORT = "USB1"
```

The DFU runner rejects non-ELF input and, when `arm-none-eabi-readelf` is
available, refuses an image without a load segment at `0x08000000`. It then
programs, verifies, and starts the firmware through CubeProgrammer. Since
Cargo's Windows executable has no `.elf` suffix, the runner creates a
temporary `.elf`-suffixed copy for CubeProgrammer and removes it after
programming. Use this no-flash end-to-end check to validate Cargo runner
discovery:

```powershell
$env:FERROWASP_DFU_DRY_RUN = "1"
.\flash-dfu.ps1 -UsbDebug
Remove-Item Env:FERROWASP_DFU_DRY_RUN
```

The helper also creates a raw `.bin` artifact before invoking the
CubeProgrammer runner:

```powershell
.\flash-dfu.ps1
.\flash-dfu.ps1 -UsbDebug
.\flash-dfu.ps1 -BuildOnly -UsbDebug
```

Direct flashing at `0x08000000` overwrites any installed Betaflight image but
does not overwrite the STM32 factory ROM bootloader. Keep motors and
propellers disconnected throughout bring-up.
