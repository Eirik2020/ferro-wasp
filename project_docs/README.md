# Project Documentation Map

This directory contains live project state, durable decisions, targeted
workflows and evidence, and historical engineering handoffs. Do not load every
document for every task.

`DOCUMENT_REGISTRY.json` is the machine-readable inventory. Its lifecycle and
context fields are enforced by `tools/check_repository_context.py`.

## Authority Order

When repository information conflicts, use this order:

1. the user's latest explicit decision and safety instruction;
2. current code, manifests, features, and tests for the selected target;
3. `CODEX_ACTIVE_WORK.md` and `../mdbook/src/current_support.md` for live state;
4. accepted, non-superseded ADRs linked from `ARCHITECTURE_DECISIONS.md`;
5. `CODEX_PROJECT_CONTEXT.md` for durable project direction;
6. targeted test plans and evidence;
7. historical handoffs and implementation plans.

Historical evidence can remain technically useful without describing current
defaults.

## Task Routing

| Task | Read |
|---|---|
| Software, target, bench, preflight, or flight testing | `testing/README.md`, then only the catalog-selected procedures |
| Localized reusable-crate work | Selected crate code and tests; use `CODEX_PROJECT_CONTEXT.md` only when architecture is affected |
| Flight app, motor output, logging, or bench workflow | `CODEX_ACTIVE_WORK.md`, selected app README/code, and the catalog-selected procedure |
| Board pins, timers, DMA, orientation, or hardware policy | `ARCHITECTURE_DECISIONS.md`, then only the relevant linked ADRs, selected app's `src/board/` support, and current target evidence |
| Cross-repository movement or ownership | `CROSS_REPO_SYNC.md` |
| Publication work | `PUBLICATION_CHECKLIST.md` and current public documentation |
| Historical provenance | The specifically relevant file under `archive/` or another registry entry marked `historical` |

Files marked `targeted` should be read only when the task requires them. Files
marked `exclude` are never default context and must be opened only to answer a
specific provenance question.

## Current And Historical Work

`CODEX_ACTIVE_WORK.md` contains only the live handoff. Completed checkpoints
and superseded state belong under `archive/`; do not append another dated
`Current State` section to the live file.

Moving material to the archive does not weaken its evidence status. Preserve
artifact identities, hashes, dates, feature sets, and limitations when
archiving.

Use `testing/EVIDENCE_INDEX.md` to locate historical test evidence without
loading the complete immutable baselines.

## Context Guard

Run:

```text
python tools/check_repository_context.py
```

The check verifies document registration, lifecycle/context values, live-file
structure, and reviewed byte budgets. It does not interpret prose, delete
history, or authorize firmware behavior.
