# App Builder Consolidation Instructions

These instructions apply with `../AGENTS.md` and the repository root rules.

The active checkpoint, evidence ledger, and cleanup protocol remain in
`../app-builder-v3/MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md`. Follow the
scoped continuation rules in `../app-builder-v3/AGENTS.md` while that plan is
active.

This directory is an isolated host workspace. Do not add it or generated
firmware to the root Cargo workspace. Repository-owned inputs are compiled
only through `src/input_catalog.rs`; do not include them in the handwritten
embedded app's module tree. Preserve output inhibition and retain both source
builders until the plan and user authorize cleanup.
