# SpeedyBee F405 V5 Flight App

This isolated RTIC 2 application targets the SpeedyBee F405 V5. It is based on
the validated FerroWasp task wiring but owns a separate board contract, Cargo
graph, linker configuration, and binary. The board's hardware facts live in
the merged data crate (`board/`, `ferrowasp-board-speedybee-f405-v5`); this app
is the HAL glue and RTIC composition on top.

Implemented board subset:

- 8 MHz HSE and 168 MHz system clock;
- SPI1 mode-3 IMU identity probe on PA4-PA7 with the ICM42688-P data-ready on
  PC4/EXTI4;
- runtime-selected MPU6500 `WHO_AM_I=0x70`, ICM42688-P `WHO_AM_I=0x47` or
  MPU-6000 `WHO_AM_I=0x68` configuration and DMA sampling; an MPU-6000 cannot
  arm until its orientation is verified;
- UART1 configurator command line on PA9/PA10;
- UART6 SBUS receiver path on PC7, the DJI O4 connector's RC line: the board
  has no inverter, so a receiver that emits SBUS non-inverted binds there;
- UART5 BLHeli legacy telemetry RX-only on PD2;
- ADC1 battery/current observation on PC1/PC5;
- four-lane DShot600 split across two timers: TIM3 CH4 on PB1 and CH3 on PB0
  (motors 1 and 2), TIM2 CH3 on PB10 and CH4 on PB11 (motors 3 and 4), all
  four DMA1 streams, with TIM3 leading TIM2 through the F405's `TIM2 ITR2`
  trigger so every lane starts on one edge (FerroHAL's two-timer waveform
  generator, proven on hardware by run R);
- standard USB CDC diagnostics on PA11/PA12;
- SPI3 NOR flash on PB3/PB4/PB5 with CS on PC12;
- SWD/RTT diagnostics with PA13/PA14 left untouched.

The board's other pads are not wired yet: the inverted buzzer on PB9 and
the gyro CLKIN PWM on PB15 (`TIM12_CH2`) await bench bring-up, as does the
on-board microSD-less blackbox path through the SPI3 flash. The SPL06
barometer on I2C1 PB6/PB7 is wired: its app probes the part at 0x76 and,
when no barometer answers, keeps probing the part's two strappings while
sweeping the bus, with the FWDBG1 line carrying `baro` (the chip ID it
answered with), `baro_part` (which driver measured it: 0 none, 1 the
SPL06-007's, 2 the DPS310 family's), `baro_addr` (the address it was
probed at), `baro_probe`
(how far the probe reached: 0 no bus, 1 measuring, 2 nothing at the
address, 3 the lines misbehaved, 4 a wrong chip ID, 5 coefficients, 6
oversampling configure) and `baro_scan` (how many addresses the sweep saw
answer, so a dead bus is told from a quiet one).

## Bench bring-up status

This app is **pre-verification**: every board-verification flag in the data
crate is false, so `FLIGHT_ARMING_ENABLED = false` and the ESC outputs stay
inhibited. Arming turns on only as bench evidence lands against stock
hardware:

- motor order and single-edge DShot start on real ESCs, starting from a
  stock-Betaflight configuration backup;
- the CW270 orientation derivation (`[-y, -x, -z]`, all three signs flipped in
  forward-right-down, matching the Foxeer's measured profile);
- ADC voltage/current scale against a known supply and load;
- buzzer polarity (the pad is inverted) and gyro CLKIN rate;
- SBUS reception on the O4 connector with a non-inverting receiver.

Build and flash as with the Foxeer app:

```
cargo build --release --locked --target thumbv7em-none-eabihf
```

or from the repository root, over SWD:

```
python tools/terminal_embed.py --board speedybee-f405-v5 --release --locked --probe-speed-khz 1800 --connect-under-reset
```
