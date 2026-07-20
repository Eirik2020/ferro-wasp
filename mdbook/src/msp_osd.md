# MSPv1 DJI O4 OSD

## Current Prototype

The DJI O4 Air unit is connected on UART4:

| FCU pin | Signal |
|---|---|
| PA0 / UART4_TX | DJI O4 UART_RX |
| PA1 / UART4_RX | DJI O4 UART_TX |

The runtime OSD path uses MSPv1 at 115200 8N1. It is a low-priority display/telemetry path and must not own actuator resources, arming authority, watchdog resources, or safety state.

The current implementation responds to the O4 unit's MSP polling and periodically sends MSP DisplayPort frames:

- `MSP_FC_VERSION`
- `MSP_NAME`
- `MSP_STATUS`
- `MSP_STATUS_EX`
- `MSP_RC`
- `MSP_ANALOG`
- `MSP_RC_TUNING`
- `MSP_PID`
- `MSP_BATTERY_STATE`
- `MSP_OSD_VIDEO_STATUS` / DisplayPort clear, write string, draw, heartbeat

## Text Layout Model

The DJI O4 OSD is driven as a character grid, not as pixels. Each text item is written at a row and column. Row `0`, column `0` is the upper-left corner of the visible canvas.

The first proven FerroWasp layout is:

```text
row 1, col 2: FERROWASP
row 2, col 2: DISARMED / ARMED
row 3, col 2: VBAT 0.0
row 4, col 2: CELL ---
row 5, col 2: CUR 0.0A
row 6, col 2: THR 1000
```

This currently appears in the upper-left corner of the goggles.

## Display Update Sequence

Treat one screen update as a small transaction:

1. Send DisplayPort heartbeat.
2. Clear the screen.
3. Write each text line.
4. Draw the screen.

The draw step matters. It tells the display device to present the frame after the text has been staged. Writing strings without a final draw may not visibly update the goggles.

For example, one frame is conceptually:

```text
heartbeat
clear_screen
write_string(row=1, col=2, attr=0, "FERROWASP")
write_string(row=2, col=2, attr=0, "DISARMED")
write_string(row=3, col=2, attr=0, "VBAT 0.0")
write_string(row=4, col=2, attr=0, "CELL ---")
write_string(row=5, col=2, attr=0, "CUR 0.0A")
write_string(row=6, col=2, attr=0, "THR 1000")
draw_screen
```

## Write String Payload

A DisplayPort write-string operation carries:

```text
subcommand = 3
row
column
attribute
text bytes
```

The current implementation builds this through:

```rust,ignore
write_string(row, col, attr, text, output)
```

Important layout rules:

- Keep text short. Betaflight-style DisplayPort strings are limited to about 30 characters.
- Keep values fixed-width where possible so updates do not leave visual leftovers.
- If a value can shrink, clear the screen before redrawing or pad the string with spaces.
- Prefer stable positions for safety/status items, so the pilot can scan the overlay quickly.
- Do not show stale sensor values as if they are valid.

## Attribute Byte

The attribute byte controls font selection and blink behavior.

For basic text, use:

```text
attr = 0
```

Attribute bits:

| Bits | Meaning |
|---|---|
| 0-1 | Font number |
| 2-5 | Reserved, keep zero |
| 6 | Blink |
| 7 | Version, keep zero |

Use blink sparingly. It is suitable for warnings such as failsafe, RC loss, low voltage, or sensor invalid. Normal telemetry should not blink.

## Data Formatting

OSD text should be formatted before calling `write_string`.

Current examples:

| Field | Example | Source |
|---|---|---|
| Armed state | `ARMED` / `DISARMED` | Safety arm reader |
| Battery voltage | `VBAT 16.4` | ADC-derived pack voltage from `ADC_VBAT` on PC0 |
| Cell voltage | `CELL 4.10` | Pack voltage divided by the configured battery cell count |
| Current | `CUR 12.3A` | ADC-derived current sensor voltage from `ADC_CURR` on PC1 using the Foxeer/Betaflight scale value |
| Throttle | `THR 0` | Internal FerroWasp throttle command, `0` to `2000` |

Recommended formatting conventions:

- Voltage: `VBAT 16.4`
- Cell voltage: `CELL 4.10`
- Current: `CUR 12.3A`
- mAh: `MAH 0420`
- RSSI/LQ: `LQ 99`
- Altitude: `ALT 012m`
- Warnings: short, high-priority words like `LOW BAT`, `STALE RC`, `STALE IMU`

Do not duplicate warnings already owned by the DJI system, such as the goggles' RC/VTX link strength and link-loss display. FerroWasp OSD warnings should come from FerroWasp's own safety state and sensor validity model.

## Layout Guidelines

Start with a sparse layout. Add fields only after the value is real and freshness-checked.

Suggested early layout:

```text
row 1, col 2: FERROWASP
row 2, col 2: ARMED / DISARMED
row 3, col 2: VBAT xx.x
row 4, col 2: CELL x.xx
row 5, col 2: CUR xx.xA
row 6, col 2: THR xxxx
row 7, col 2: LQ xx
row 8, col 2: WARN ...
```

Keep high-priority warnings near the top-left until the complete OSD layout is designed. Avoid placing experimental/debug values where they can be mistaken for safety-critical status.

## Freshness Rules

Every displayed data source should eventually have a freshness or validity flag.

If a value is stale:

- Blank it,
- replace it with `---`,
- or show an explicit warning.

Do not leave old values on the OSD after sensor loss, RC loss, estimator invalidity, or battery ADC failure.

## Sensor TODOs

The OSD currently has enough placeholder/live data to prove the UART/MSPv1/display path. Add these sensor readings before treating the overlay as useful flight telemetry:

- Validate battery voltage ADC scaling on hardware. The `75k` / `10k` divider means the ADC node sees about `1 / 8.5` of pack voltage, so firmware multiplies the measured ADC voltage by `8.5`.
- Move the hard-coded 6S battery cell count into board/profile configuration when configuration support exists.
- Validate current-sense scaling on hardware. The Foxeer Reaper 55A ESC lists Betaflight current scale `70`; the prototype conversion currently treats the ADC reading as `ADC_mV / 70` amps after bench observation showed the first pass was 10x high.
- Accumulated mAh from current integration.
- Low-battery warning driven by the safety master, using validated cell voltage thresholds and freshness checks.
- Stale-data warning driven by the safety master, covering at least stale battery ADC, stale RC command input, and stale IMU/estimator data.
- RSSI or link quality from the active RC protocol.
- Altitude from barometer or other height source.
- Ground speed and position from GNSS.
- Flight mode and failsafe state from the safety kernel.
- IMU health/staleness indicator.
- Craft attitude from the estimator rather than raw gyro integration only.
- VTX temperature/status if the DJI side exposes it reliably.

OSD data freshness should be tracked. Stale or unavailable values should be blanked or marked invalid instead of being displayed as valid telemetry.
