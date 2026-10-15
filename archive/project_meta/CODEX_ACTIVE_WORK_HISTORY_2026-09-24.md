# FerroWasp Active Work History - 2026-09-24

Archived 2026-09-29 from `project_meta/CODEX_ACTIVE_WORK.md`, section
`Current State - 2026-09-24`, subsection `FerroForge adoption candidate -
flown, every gate passed`. It holds the flown campaign's result and reasoning,
the motor-identity note and the storage-CLI bug as they stood that day,
verbatim. The flown record, the equivalence result and the two open bugs still
bind and remain in the active handoff; the storage-CLI bug was since fixed
(`cbc892b`). Use this file for provenance only.

## Current State - 2026-09-24

### FerroForge adoption candidate - flown, every gate passed

Foxeer runs on `ferroforge::app!`: every task except `usb_fs` and
`flash_manager_task` is an instance of a `ferrowasp-stm32f4-tasks` definition,
including the whole safety and actuator path. Bodies moved verbatim;
priorities and bindings are unchanged. It has flown.

Every gate passed on candidate `a921ffe` (binary `16f6e8ed`): SW-COMMON-001,
BUILD-FOX-001, BENCH-COMMON-001, BENCH-FOX-USB-001, BENCH-FOX-001, then
PREFLIGHT-FOX-001 and FLIGHT-FOX-001 flown 2026-09-24 and accepted by operator
decision. Records under `testing/evidence/runs/2026/09/`; BENCH-FOX-001 keeps
both a `fail` and the `pass` that supersedes it, `__01` for the reasoning.

**The adoption question is answered.** Measured pre- against post-conversion
over ~133k armed samples: identical control law, identical loop timing, and
zero sequence gaps, repeats or CRC failures in ~10 MB of logs - including
through flight 22's 1592 deg/s cartwheel with the mixer saturated. No
oscillation in either era. Detail is in the run records; logs are under
`logs/ferroforge-flights/` and `logs/preconversion-flights/`, gitignored.
Command tracking was never evaluated, because every flight with stick input
ended in deliberate ground contact; one ordinary hop would close it.

FerroForge 0.3.0 is published and `main` pushed, so the path override is gone
and this repo builds against the registry. That changes the image, so tag
`foxeer-candidate-16f6e8ed` is historical and any future flight re-gates.
Left: convert `usb_fs` and `flash_manager_task` shaped by Foxeer alone, and
retire `tools/rtic-app-builder`, whose phase 6 entry condition was this
flight. All of it is on branch `ferroforge-adoption`.

Two open bugs carried forward. Neither can stop a running motor - current
sense drives only OSD and MSP, and `EscManager::is_faulted` has one consumer,
a log line - and neither is accepted behaviour.

1. Battery current reads a constant `0.1 A` with four motors at 6300..7700
   eRPM. `centiamps = adc_mv * 10000 / 70 / 10` puts raw PC1 near `1 mV`, the
   noise floor, so the fault is upstream of the scale-70 change this candidate
   adopted, which could never have fixed it. Next: read `adc_current_mv` from
   the USB debug status under load - still ~1 mV means the sense input, not
   the math.
2. The ESC telemetry manager latched faulted after a single response timeout
   for logical M4 (one miss in 10132 requests, zero CRC failures) during an
   armed RC-loss stop, and both later arm attempts then correctly aborted on
   idle telemetry qualification. `esc_manager.rs` and `blheli_telemetry.rs`
   are byte-identical to pre-conversion; the moved UART plumbing in
   `ferrowasp-stm32f4-tasks/src/esc.rs` is new, so a conversion-induced
   dropped response is not excluded by code identity alone.

A latch also costs per-motor eRPM logging for the rest of that power cycle.
The bench run does **not** independently reproduce the ESC-only power-cycle
bug below - that latch was already set 100 s before the battery was
reconnected - but avoid ESC-only power cycles with USB attached regardless.

Motor identity is confirmed unchanged by the conversion, in code and by the
operator's roll and pitch differential response; see the `BENCH-FOX-001`
record.

Third open bug, pre-existing and cosmetic: the first storage-CLI command after
each USB port open is rejected once with `ERR invalid command`, then succeeds
on retry. `CommandParser` in `crates/ferrowasp-tasks/src/flash_storage.rs`
accumulates a line with no reset across port open, so a stray byte corrupts
the first line and the parse error clears the buffer.

Closed: the `UART4 RX free-buffer pool exhausted on IDLE` warning was noise on
an unterminated line, absent throughout the powered run with the VTX
connected.
