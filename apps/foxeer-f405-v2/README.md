# Foxeer F405 V2 Flight App

This isolated RTIC 2 application targets the Foxeer F405 V2. It is based on
the validated FerroWasp FCU3 task wiring but owns a separate board contract,
Cargo graph, linker configuration, and binary.

Normal users should begin with the repository
[Foxeer USB Quick Start](../../docs/FOXEER_F405_V2_QUICK_START.md). The
published Windows package includes a checked release image and does not
require a Rust toolchain, Python, STM32CubeProgrammer, or an SWD probe.

Implemented board subset:

- 8 MHz HSE and 168 MHz system clock;
- SPI1 mode-3 IMU identity probe on PA4-PA7;
- runtime-selected MPU6500 `WHO_AM_I=0x70` or ICM42688-P
  `WHO_AM_I=0x47` configuration and DMA sampling;
- USART2 SBUS receiver path on PA2/PA3;
- UART4 DJI MSP DisplayPort path on PA0/PA1;
- ADC1 battery/current observation on PC0/PC1;
- standard four-lane DShot600 on PA8, PC9, PC8, and PB15;
- standard BLHeli legacy telemetry RX on PA10 / USART1;
- standard USB CDC diagnostics on PA11/PA12;
- standard 16 MiB-class SPI2 NOR blackbox/configuration storage on PB12/PB13/PC2/PC3;
- SWD/RTT diagnostics with PA13/PA14 left untouched.

M4 is the complementary `TIM1_CH3N` output. The board support configures it explicitly;
powered props-off PWM selection has functionally validated the chosen polarity.
No electrical waveform measurement has been taken.

The fitted IMU identity, body-axis map, PC4/EXTI4 runtime cadence, physical
motor order/direction, functional M4 polarity, DShot/PA10 eRPM qualification,
and onboard blackbox path have been confirmed on the target. Exact electrical
waveforms remain unmeasured because the logic-analyzer checkpoint was skipped.
The board support now permits the normal flight path using the documented Betaflight
voltage baseline and Foxeer current scale. The ADC/OSD path uses the upstream
target values directly: VBAT scale 110, current scale 70, and current offset
zero. Cell count is detected from Betaflight's 4.30 V maximum-cell threshold
and latched until battery removal. Fine PC0/PC1 calibration remains a TODO.

The first prop-on departure on 2026-07-22 attempted an immediate forward flip.
Onboard records established that the physically correct Foxeer pitch rate was
being fed into the FCU3-compatible controller with the wrong sign, creating
positive pitch feedback. The physical IMU map remains unchanged; rate control
now uses an explicit pitch-polarity compatibility transform. Both corrective
props-off checks below passed on 2026-07-22. The result permits preparation of
a clean logged candidate, which was subsequently programmed and boot-verified
as `B85DB4F43897EF628EFF0C368CF0670F34FEEDB3FC59C895F21ECFB91D3E6FC4`.
This does not itself validate flight. Do not fly the
pre-fix image with SHA-256
`E4BAE2A6229D1B340E4DF72BF0727D00506989FE9A1DCDE3B71935B4D6BC9758`.

The 2026-07-22 individual motor-output test also exposed an arm-high reset defect. The
shared RC-link fix ignores arm-low transients received before link
qualification completes. Its target repeat held arm high across flashing and
remained disarmed after RC qualification for the full observation window. A
valid arm-low observation followed by a later high transition is required.

The current fresh-storage Foxeer tuning baseline is P-only:
roll/pitch/yaw P `2.5 / 2.5 / 2.0`, with every I and D gain set to zero.
Persisted configuration remains authoritative across firmware updates; these
defaults apply only when no valid stored configuration exists or the operator
explicitly restores defaults.

### Props-off actuator validation

The compile-time `bench_actuator_validation` gate selects capped commissioning
behavior instead of the normal mixer. It must be
combined with a capped equal-motor or logical-motor bench feature; it cannot build a normal PID/mixer flight image. For example:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --no-default-features --features "board-foxeer-f405-v2 bench_actuator_validation bench_logical_motor1_only"
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
existing all-motor DShot idle stage during arming before the selected/capped
command begins, so every motor must be treated as potentially live. Remove
propellers and verify the selected feature string before powering ESCs.

### Default DShot600 flight candidate

The normal Foxeer image now defaults to DShot600 and its bundled PA10 legacy
telemetry/eRPM qualification path:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --probe-speed-khz 1800 --connect-under-reset
```

It owns TIM1/TIM8 and DMA2 Streams 1, 7, 2, and 6 as one synchronized fault
domain. The 500 Hz actuator service continuously emits frames, enforces the
bounded nonzero-command lease, and requests disarm on lease expiry, DMA fault,
spurious completion, or frame timeout.

DShot arming is protocol-specific. It keeps continuous stop frames selected
through a guarded 100 ms pre-arm dwell, then applies idle under a temporary
actuator permit while the system remains logically disarmed. The actuator
owner requires three consecutive fresh observations from every physical ESC
output in the 3,000-10,000 eRPM window after a 250 ms spin-up grace. Missing,
zero, stale, or overspeed evidence aborts arming and selects four stop values;
only successful four-motor qualification can notify the safety master. The
PWM-only image separately retains its legacy 2.5-second low plus 500 ms idle
ESC preparation.

Run the capped positive qualification candidate with:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features "bench_actuator_validation bench_equal_motors" --probe-speed-khz 1800 --connect-under-reset
```

The pre-bench positive candidate produced ELF SHA-256
`28C5E4BDC4C9B51998385A434983D83035B5FAE8136E8437E8B07ABF9A8A2B70`.

The negative candidate forces only the observed eRPM for physical output 1 /
logical M1 rear-right to zero; it does not stop that physical motor directly:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features "bench_actuator_validation bench_equal_motors bench_dshot_idle_output1_not_running" --probe-speed-khz 1800 --connect-under-reset
```

Its pre-bench ELF SHA-256 is
`9A7A14751747969CDE80265AD2ADA7AB0464A423A1D4A64DC1DE6CED9E8A9073`.

The bounded ESC-manager task rotates requests across physical outputs M1-M4,
parses the 115,200-baud BLHeli frames received on PA10, and prints eRPM plus
parser/request statistics to RTT. Only the actuator-owned DShot service can
set a frame's telemetry-request bit; it returns a sequenced acknowledgement to
the manager after the request is actually emitted. The manager additionally
publishes bounded, timestamped updates to the actuator owner for idle
qualification. A request/response-association timeout latches telemetry off
until reboot and therefore causes later arm attempts to fail closed.
RTT identifies both physical output and logical motor, and reports request,
acknowledgement, response, mismatch, unsolicited-frame, CRC, and discarded-byte
counters so the checkpoint can distinguish wiring faults from association
faults.

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
5. After the base DShot transport and motor behavior pass, validate the bundled
   PA10 ESC telemetry association. Power the FC and ESCs together so telemetry
   is available when the manager's five-second boot delay ends. Confirm each
   M1-M4 eRPM rises from zero at idle and follows throttle. Also confirm
   CRC/discard counts remain stable and telemetry returns to zero after disarm.
6. Wait at least five seconds after boot, then run the positive and injected
   idle-qualification images. The positive image must reach `[3, 3, 3, 3]`
   before `SYSTEM ARMED`. The injected image must identify physical output 1 /
   logical M1, stop all outputs at the 1.2-second deadline, and never report
   `SYSTEM ARMED`.

Steps 1-3 have target evidence as of 2026-07-22. Individual PWM selection
confirmed M1 rear-right CW, M2 front-right CCW, M3 rear-left CCW, and M4
front-left CW. The equal-motor image then idled and followed throttle on all
four motors and stopped them on explicit disarm. The boot-high and
reconnect-high arming interlocks also passed after the shared RC-latch fix.
The unpowered half of step 4 also passed: 25,000 synchronized DShot frame starts
completed with four zero values and no busy, expiry, timeout, DMA-fault, or
spurious-interrupt report. The first powered attempt then revealed that the
Foxeer app shell still selected the PWM preparation sequence around the shared
DShot backend. That defect is corrected: DShot now uses the FCU3-style guarded
100 ms stop-frame preparation and remains stopped until `SYSTEM ARMED`; PWM
retains its original low/idle holds. The receiver and ESC could not be powered
separately, so a corrected-image stop soak was followed by a powered props-off
test. The corrected sequence passed: idle began only after `SYSTEM ARMED`, all
four motors followed throttle, explicit disarm and RC loss stopped them, link
recovery and boot with arm high did not rearm, and all DShot counters remained
clean. Base step 4 is therefore complete.

Step 5 passed on 2026-07-22 with corrected image SHA-256
`CCD4B30A5CA900E28BF1C07D75E63E59353206EE870427A32A93787D74DBD16F`.
All four associated outputs reported zero eRPM stopped, approximately
6,600-7,200 eRPM at idle, and approximately 10,100-10,700 eRPM under throttle.
Disarming while throttle was raised selected four stop values and returned all
four readings to zero. Request, response, parser, and DShot fault counters
remained clean through 2,450 associated samples. Telemetry remains
available after arming for observation; loss after the armed transition does
not currently trigger an in-flight disarm.

Step 6 and the normal uncapped-mixer props-off flight handoff passed on
2026-07-22. The final run confirmed the IMU pre-arm gate, live OSD voltage,
DShot/eRPM health, motor response, explicit disarm, RC-loss stop, and the
arm-low-before-rearm latch. The exact evidence and hashes are routed through
`../../project_docs/testing/EVIDENCE_INDEX.md`.

To repeat the deterministic IMU integration fault test:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features bench_prearm_imu_stale --probe-speed-khz 1800 --connect-under-reset
```

After the ordinary startup bias-calibration message, request arm at zero
throttle. Require `Arming aborted: IMU sample is stale`, no temporary motor
idle, and no `SYSTEM ARMED`. Then reflash the normal candidate;
never use the fault-injection image for flight.

An unsupported or failed IMU identity/configuration is nonfatal: firmware
logs the result once, disables periodic IMU transactions, and continues the
RTT/RC/OSD/ADC bring-up paths. MPU6000 is not yet implemented.

Foxeer IMU sampling is now driven by PC4/EXTI4 rather than the TIM4 poll
trigger. Both supported drivers configure an active-high, push-pull data-ready
pulse; the EXTI handler timestamps the edge, clears it, and defers one bounded
SPI DMA request without doing blocking bus work. Routine builds omit periodic
IMU raw/DRDY RTT reports. Feature `imu_transport_rtt` restores the totals,
two-second deltas, and rejected-trigger counts; `--foxeer-smoke` enables it
automatically. The existing transaction
deadline, stale-sample detection, and timer-driven 400 Hz control cadence remain
intact. Functional EXTI cadence and zero rejected triggers have target evidence;
the skipped electrical pulse-shape measurement remains documented separately.

For the physical sensor-axis check, use the observational RTT-only diagnostic:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features imu_orientation_rtt --probe-speed-khz 1800 --connect-under-reset
```

Every two seconds it adds one sensor-frame line containing acceleration in mg,
gyro in tenths of a degree per second, and temperature in tenths of a degree
Celsius, followed by a physical body-frame line containing mapped gravity and
angular rates and a separate rate-controller line. Physical body pitch follows
the right-hand rule; the established FCU3 controller convention intentionally
inverts only pitch. Keep ESC power disconnected. Capture stationary level,
then slowly move nose-up, right-side-down, and clockwise in yaw for at least
four seconds per motion so a two-second snapshot lands during each movement;
hold and pause between motions. The feature does not alter the USB
status protocol, IMU scheduling, control rate, safety state, or actuator gate.
The `sensor` line remains uncorrected input evidence; the `body` line validates
the board support's measured signed-axis mapping, while the `control` line exposes the
rates actually passed to the rate PID.

### Corrective pitch-opposition gate

After any pitch-axis or mixer change, remove all propellers. Disconnect ESC
power and run the combined physical/controller diagnostic and onboard logger
from the repository root:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features "imu_orientation_rtt" --probe-speed-khz 1800 --connect-under-reset
```

Keep ARM low and slowly pitch the airframe in both directions. Nose-up must be
positive on `IMU ORIENT body` and negative on `IMU ORIENT control`; nose-down
must show the inverse. Roll and yaw must keep the same sign on both lines.

Without reflashing, return the craft to level, connect ESC power, and hold the
airframe securely. After stationary gyro bias calibration, arm at zero
throttle, allow all four ESCs to qualify, and use only modest throttle.
Slowly pitch nose-down: the controller pitch rate must be positive, pitch PID
negative, and front M2/M4 higher than rear M1/M3. Slowly pitch nose-up:
controller pitch must be negative, pitch PID positive, and rear M1/M3 higher
than front M2/M4. Confirm roll and yaw still oppose motion, then disarm and stop
the run. Leave the system disarmed for at least two seconds so the final flash
page is committed. Retain the RTT and onboard evidence for review. Do not
reinstall propellers or attempt another hop until both checks pass.

Both checks passed on 2026-07-22. The unpowered orientation image was
`C639C8BD3476E8415632644E970D4BAB3B42417FD9C624428B6D5D343D37F7FA`;
the powered normal-mixer image was
`FF6606EFACC55C9C88CCDE5EC044C0CB3B3881A5229319C26820621654DD136F`.
The powered capture produced the correct PID and motor-pair polarity for every
selected pitch, roll, and yaw motion sample. See
`../../project_docs/testing/EVIDENCE_INDEX.md` for the retained logs and exact
counts.

Continue the already completed archive so only pages added by this test cross
USB, then select the flight ID that `list` reported as `next flight` before
arming:

```powershell
python tools\ferrowasp_storage.py --port COM7 list
python tools\ferrowasp_storage.py --port COM7 --timeout 10 read --resume --output logs\foxeer-hop-front-flip.fwbb
python tools\blackbox_analyzer.py logs\foxeer-hop-front-flip.fwbb --flight-id 23 --mode swing --csv logs\foxeer-pitch-fix-flight23.csv
```

Replace the COM port and flight ID with the values observed on the target.

The standard DShot implementation uses board-declared TIM1/TIM8 timer and DMA
routes, including the complementary `TIM1_CH3N` output for M4. Motor actuation
remains safety-owned; only the capped bench selectors are commissioning-gated.
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

The board support permanently reserves PA13 for SWDIO and PA14 for SWCLK. It does not
claim the board LEDs that share those MCU signals. Connect the debugger's
SWDIO, SWCLK, target-reference voltage, and ground; connect NRST as well when
the retrofit exposes it. The debugger must use the board voltage only as a
logic-level reference and must not back-power an otherwise unpowered flight
controller unless the debugger and wiring are explicitly designed for that.

From the repository root, build, flash, reset, and stream decoded RTT with:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
```

For the first connection, use the bounded automatic smoke test:

```powershell
python tools\terminal_embed.py --foxeer-smoke
```

It builds the default DShot release image with the dedicated
`smoke_actuator_inhibit` lockout, programs it over SWD,
collects RTT for 14 seconds after firmware boot, and exits with PASS only after observing successful
Foxeer initialization, the RTT hello, the smoke actuator lockout, a supported IMU
identity, and at least two approximately 1 kHz PC4/EXTI4 data-ready intervals.
The full transcript and firmware hash are retained under
`logs/terminal_embed`. Keep propellers removed and ESC power disconnected.
The preset uses the target-validated 1.8 MHz ceiling of the attached ST-Link
V2. If a future cable or probe is marginal, add `--probe-speed-khz 100`.
If normal attach fails after wiring NRST, retry with
`python tools\terminal_embed.py --foxeer-smoke --connect-under-reset`.

This selects the Foxeer app and `FerroWaspFoxeerF405V2` ELF explicitly. Keep
the flight battery and ESC power disconnected for the first SWD session. USB
DFU remains available as a recovery path.

From this app directory, `cargo run --release --locked` also uses
`probe-rs run` over SWD. It does not invoke the DFU runner.

## USB Debug And Onboard Storage

The board-mandatory USB FS route exposes a CDC ACM device named
`FerroWasp Foxeer Debug`. It writes a header after enumeration and one bounded
ASCII status line with each roughly two-second firmware heartbeat:

```text
FWDBG1 ms=12345 imu=icm42688p ready=1 seq=9876 gyro=-17,4,-70 stale=0 ctl=4938 rc=1 armable=1 thr=1000 arm_sw=0 armed=0 vbat_dV=230 current_cA=-12 adc_v_mV=2091 adc_i_mV=1234
```

The fields report uptime, selected IMU and transport state, raw gyro and IMU
sequence, control sequence, RC qualification/throttle/arm switch, system arm
state, pack voltage in decivolts, and current in centiamps. `current_cA` uses
the Foxeer/Betaflight scale 70 and zero offset. The stream is read-only.
`adc_v_mV` and `adc_i_mV` are the pre-scale ADC observations retained for
future fine calibration. Received USB bytes are drained and ignored, and the
USB task owns
no safety or actuator handle.

Build the diagnostic image without flashing:

```powershell
.\flash-dfu.ps1 -BuildOnly
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

The standard onboard-storage service extends the same CDC endpoint with bounded
ASCII commands. It probes JEDEC identity, records fixed-size CRC-protected
control snapshots while armed, flushes the final partial page on disarm, and
permits disarmed-only configuration saves, confirmed log erase, and a dedicated
scratch-sector erase/program/readback self-test.

The separate `mspv2_configurator` gate replaces the ASCII/status stream on the
CDC endpoint with bounded native MSPv2 frames. It uses the same standard storage owner and
implements the common read-only `MSP_API_VERSION`, `MSP_FC_VARIANT` (`FWSP`),
`MSP_FC_VERSION`, `MSP_BOARD_INFO`, `MSP_BUILD_INFO`, `MSP_STATUS`, and
`MSP_UID` commands. FerroWasp-native requests use function `0x7A00` and a
version-1 postcard envelope. Supported native operations are hello/capability
discovery, whole-config read/stage/CRC-checked commit, defaults restore,
bounded blackbox listing/info, and CRC-protected reads of at most 512 bytes.
The endpoint advertises config-write and reset capabilities; firmware still rejects mutation while armed. Individual-log erase and software reboot are
not advertised or implemented.

MSPv2 config staging and persistence are rejected while armed. Blackbox access
is disarmed-only, active logs cannot be downloaded, the host may have only one
outstanding request, and all parser, queue, response, and chunk sizes are
fixed. The USB task still owns no safety or actuator handle.

SPI2 runs in mode 0 at 10 MHz using short CPU-driven transfers. This avoids
the fixed DMA1 Stream 4 collision between SPI2 TX and the validated UART4 OSD
TX route. The priority-1 flash manager uses bounded queues, never owns motor
hardware, rejects destructive commands while armed, and aborts maintenance if
the system arms.

The first two 4 KiB sectors are copy-on-write configuration slots, the third
is reserved for the destructive self-test, and logs begin at `0x3000`.
Configuration input is limited to the firmware-owned whitelist: PID gains,
IMU LPF alpha, log-rate divisor, RC deadband, and per-axis Actual Rates
center/max/expo values. Firmware-owned ranges and cross-field constraints are
enforced before a disarmed-only atomic save. A foreign/non-FerroWasp log region
stays read-only until an explicit confirmed erase.

Build the standard image from the repository root:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features mspv2_configurator
```

The normal-mixer flight image includes DShot and flash blackbox recording:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --probe-speed-khz 1800 --connect-under-reset
```

The first blackbox run requires one explicit log-partition erase because the
existing non-FerroWasp contents are intentionally protected as
`writable: False`. This erases the log partition only, not the two
configuration slots or the dedicated scratch-test sector. With the enumerated
COM port in a second terminal:

```powershell
python tools\ferrowasp_storage.py --port COM7 info
python tools\ferrowasp_storage.py --port COM7 list
python tools\ferrowasp_storage.py --port COM7 erase --confirm
python tools\ferrowasp_storage.py --port COM7 list
```

Require `used pages: 0` and `writable: True`, then reboot or reflash the exact
same image before recording. The reboot clears records intentionally dropped
while the long erase monopolized flash maintenance, giving the capture clean
runtime counters. Wait at least five seconds after reboot, then perform
props-off disarmed, armed-idle, modest throttle, and explicit-disarm phases.
Leave it disarmed for at least two seconds so the final partial page flushes.
Then download and analyze:

```powershell
python tools\ferrowasp_storage.py --port COM7 list
python tools\ferrowasp_storage.py --port COM7 read --output logs\foxeer-bench.fwbb
python tools\blackbox_analyzer.py logs\foxeer-bench.fwbb --mode auto --csv logs\foxeer-bench.csv
```

Require nonzero used pages, zero RTT-reported record drops and write faults,
CRC-valid downloaded pages, one flight transition, plausible IMU/control
sequences and motor values, and a final partial page. Do not issue storage
commands while armed during this first capture.

The analyzer validates every `.fwbb` page strictly. A bad magic, version,
record count, CRC, or truncated page fails the command rather than being
silently skipped. Its flash summary also reports flight IDs, page-sequence
endpoints, total records, partial pages, and the final-page record count.

The self-test alters only the reserved scratch sector. `erase --confirm`
erases the whole FerroWasp log region and may take several minutes. These
paths have basic target evidence, including a CRC-valid 551-page armed capture.
Interrupted configuration-save recovery and armed maintenance rejection remain
hardening work; neither USB nor storage has an actuator-authority handle.

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
.\flash-dfu.ps1
Remove-Item Env:FERROWASP_DFU_DRY_RUN
```

The helper also creates a raw `.bin` artifact before invoking the
CubeProgrammer runner. Use `.\flash-dfu.ps1 -BuildOnly` to create both
artifacts without flashing.

Direct flashing at `0x08000000` overwrites any installed Betaflight image but
does not overwrite the STM32 factory ROM bootloader. Keep motors and
propellers disconnected throughout bring-up.
