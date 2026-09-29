# FerroWasp Flight Logging Format

What the recorded flight log must carry, and where the move to ULog stands.

This left `CODEX_ACTIVE_WORK.md` on 2026-09-24 because it is a specification
rather than a next action: the handoff was spending a fifth of its budget on
it. The items are unchanged and still open. The handoff points here.

## Open work

- Record every fresh per-motor legacy-UART eRPM observation at its actual
  bounded rate, including time/age, identity, freshness, and transport health;
  never present repeated stale values as new control-rate data.
- Log timestamped body-frame accelerometer data, range/clipping, and bounded
  peaks for offline crash-detector development. Validate against landings,
  maneuvers, gusts, and impacts before allowing any safety-state effect.
- Make every recorded flight self-describing by storing the exact active
  configuration at its flight boundary and after any accepted runtime change.
  In the 2026-07-27 session, BB2 `PID/error` identifies flights 29-32 as P
  `1/1/2` and 33-34 as `2.5/2.5/2`; future readers must expose configuration
  directly and warn when it is absent.
- Adopt ULog as FerroWasp's standard persisted flight-log format so recorded
  data can use the existing PX4 logging, telemetry, visualization, and analysis
  ecosystem. Define stable FerroWasp message schemas and units, board/firmware
  metadata, parameters, timestamps, dropout reporting, and flight boundaries.
  Keep the hard real-time producer allocation-free and bounded: control tasks
  should publish fixed-size typed records to the existing bounded logging
  boundary, while a lower-priority owner performs ULog framing and flash I/O.
  Provide host tests with known-good ULog readers and a migration/conversion
  path for retained BB2/`.fwbb` evidence before replacing the current format.
- Migration paths now exist in both `tools/fwbb_to_ulog.py` and the packaged
  native FerroConfigurator. Both validate every `.fwbb` page, convert exactly
  one selected flight, and emit schema version 1 of the compact
  `ferrowasp_rate_control` topic. Synthetic format/timing/dropout tests pass;
  the native output is byte-identical to the Python reference for retained
  flight 27, and PyULog 1.2.3 previously accepted the reference converter's
  real Foxeer flight 11 output as uncorrupted. Native firmware ULog framing,
  parameters, richer metadata, and replacement of `.fwbb` remain open work.
- Onboard BB2 retrieval is now flight-aware. The USB host tool can catalog
  contiguous flight page ranges, download `--flight-id latest` or a numeric
  ID, and resume only after validating the selected flight ID, per-flight page
  sequence, and every page CRC. A backward-compatible record flag marks the
  first successfully assembled record of the first recorded flight after each
  MCU boot, allowing `flights` to group new captures by power-on session.
  Existing pages remain readable and are deliberately labeled `boot unknown`
  rather than grouped using ambiguous wrapping MCU timestamps.
