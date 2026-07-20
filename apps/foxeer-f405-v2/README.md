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
- optional read-only USB CDC diagnostics on PA11/PA12;
- SWD/RTT diagnostics with PA13/PA14 left untouched.

M4 is the complementary `TIM1_CH3N` output. The BSP configures it explicitly;
it must be validated separately on the physical board.

Flight arming is intentionally inhibited until all of these are confirmed:

- fitted IMU identity;
- FerroWasp body-axis orientation and signs;
- ADC voltage/current calibration;
- logical-to-physical motor order;
- M4 complementary-output polarity and waveform.

An unsupported or failed IMU identity/configuration is nonfatal: firmware
logs the result once, disables periodic IMU transactions, and continues the
RTT/RC/OSD/ADC bring-up paths. MPU6000 is not yet implemented.

The RC PWM implementation uses a board-local TIM1/TIM8 owner because M4 is
the complementary `TIM1_CH3N` output. PWM/timer-DMA and DShot integration
remain deferred. M5-M8, SPI flash, analog OSD, I2C barometer, buzzer, camera
control, LED strip, and additional UARTs are not part of this application.

## Build

```powershell
cd apps/foxeer-f405-v2
cargo build --release --locked
```

See `flash-dfu.ps1` for the USB DFU build/flash sequence. Do not use the FCU3
binary on this board.

## USB Debug

The opt-in `usb_serial` feature exposes a CDC ACM device named
`FerroWasp Foxeer Debug`. It writes a header after enumeration and one bounded
ASCII status line with each roughly two-second firmware heartbeat:

```text
FWDBG1 ms=12345 imu=icm42688p ready=1 seq=9876 gyro=-17,4,-70 stale=0 ctl=4938 rc=1 armable=1 thr=1000 arm_sw=0 armed=0 vbat_dV=230 current_cA=-12
```

The fields report uptime, selected IMU and transport state, raw gyro and IMU
sequence, control sequence, RC qualification/throttle/arm switch, system arm
state, pack voltage in decivolts, and current in centiamps. The stream is
read-only: received USB bytes are drained and ignored, and the USB task owns
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

## USB DFU

The app-local Cargo configuration uses STM32CubeProgrammer to write the
Cargo-generated ELF directly. The runner searches `PATH`, the normal
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

Build and flash the normal image directly from Cargo:

```powershell
cargo run --release --locked
```

Build and flash with read-only USB diagnostics:

```powershell
cargo run --release --locked --features usb_serial
```

If multiple STM32 DFU devices are connected, select one before running Cargo:

```powershell
$env:STM32_DFU_PORT = "USB1"
```

The runner rejects non-ELF input and, when `arm-none-eabi-readelf` is
available, refuses an image without a load segment at `0x08000000`. It then
programs, verifies, and starts the firmware through CubeProgrammer. Since
Cargo's Windows executable has no `.elf` suffix, the runner creates a
temporary `.elf`-suffixed copy for CubeProgrammer and removes it after
programming. Use this no-flash end-to-end check to validate Cargo runner
discovery:

```powershell
$env:FERROWASP_DFU_DRY_RUN = "1"
cargo run --release --locked --features usb_serial
Remove-Item Env:FERROWASP_DFU_DRY_RUN
```

The existing helper remains available. It also creates a raw `.bin` artifact
before invoking the same CubeProgrammer runner:

```powershell
.\flash-dfu.ps1
.\flash-dfu.ps1 -UsbDebug
.\flash-dfu.ps1 -BuildOnly -UsbDebug
```

Direct flashing at `0x08000000` overwrites any installed Betaflight image but
does not overwrite the STM32 factory ROM bootloader. Keep motors and
propellers disconnected throughout bring-up.
