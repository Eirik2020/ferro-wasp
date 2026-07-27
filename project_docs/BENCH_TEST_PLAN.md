# Bench Test Plan

This legacy path is now a bounded router. It no longer owns current test
selection, current flight state, or dated run evidence.

## Current Testing

Start at `testing/README.md`, then use `testing/TEST_CATALOG.json` to select
only the procedures relevant to the exact target and validation tier.

Current operator procedures:

- FCU3: `testing/targets/fcu3.md`
- Foxeer F405 V2: `testing/targets/foxeer-f405-v2.md`

For current bench/debug work, motor-output behavior, logging changes, and open
flight work, read `CODEX_ACTIVE_WORK.md`. Current code, manifests, and tests
outrank narrative documentation.

## Historical Evidence

Use `testing/EVIDENCE_INDEX.md` to locate the relevant evidence without loading
the full history.

The complete former bench plan is retained byte-for-byte at
`archive/BENCH_TEST_PLAN_FULL_BASELINE_2026-07-27.md`. It includes completed
tests, superseded tuning snapshots, historical failures, open-risk snapshots,
and earlier proposed procedures. Those records remain useful provenance but
must not be treated as a current procedure or flight clearance.

Do not append new run results to the immutable archive. Record evidence
separately from test definitions using `testing/evidence/README.md`, preserving
the exact candidate identity, date, target, features, equipment, result, logs,
and limitations.
