# App Builder V3 Instructions

These instructions apply with `../AGENTS.md` and the repository root rules.

## Current migration plan

`MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md` owns the checkpoint order,
completion criteria, evidence ledger, and the boundary between making the
builder canonical and replacing the handwritten Foxeer firmware.

For an ordinary continuation, read only:

1. the plan's **Execution status** section;
2. the complete section for the active checkpoint named there;
3. the files explicitly required by that checkpoint and the scoped repository
   instructions.

Locate checkpoint headings with an anchored search such as
`rg -n '^## Checkpoint ' MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md` instead of
reading every intervening completed section. Read `README.md` when first
entering this workspace or when the current checkpoint changes commands or
source-of-truth boundaries.

Read the complete migration plan when:

- beginning migration work without a reliable handoff that identifies the
  current plan-spec revision;
- transitioning from one checkpoint to the next;
- the user changes migration scope, architecture, or a safety boundary; or
- the execution status, checkboxes, evidence, and repository state disagree.

Progress and evidence edits do not by themselves require another full-plan
read. The plan-spec revision changes only when its requirements or ordering
change.

Use the existing `../rtic-app-builder/` manifests, contracts, fixtures, and
checkpoint workflow as preserved inputs to the migration. Do not interpret
the V3 candidate's more complete Foxeer graph as permission to discard those
capabilities or bypass their checks.

## Working the checkpoints

- Work only on the active checkpoint unless the user explicitly changes the
  order or scope.
- Update the plan's execution status, checkboxes, and bounded evidence entry
  when work is actually demonstrated. Reference retained logs and artifacts;
  do not paste large command output into the plan.
- Do not reopen completed checkpoints or repeat unaffected checks when their
  recorded inputs are unchanged. A checkpoint requiring a fresh run or exact
  image always overrides this reuse rule.
- Batch independent read-only inspection and verification where practical.
  Load only the targeted design note, procedure, or archive section required
  by the active work.
- At a checkpoint transition, review the full plan once, compact the completed
  evidence to stable references, and update **Execution status** to the next
  checkpoint before continuing.
- A generated file or successful compile is not target, electrical, actuator,
  or flight evidence.
- Ask before changing the canonical builder path, crate/workspace layout,
  public CLI or manifest formats, controlled hardware assignments, safety
  states, or actuator-output gates.
- Keep generated output disposable and output-inhibited until the applicable
  plan checkpoint and repository target procedure explicitly allow otherwise.
- Do not add either isolated builder or its generated applications to the root
  Cargo workspace without explicit architectural approval.
- Do not run flash, probe, powered-actuator, or flight steps without the
  required user intent and the repository test/evidence procedure.

## Completion and plan removal

Do not delete `MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md` merely because its
technical checkboxes are complete. At the final checkpoint, present the
completed implementation and evidence to the user and explicitly ask the user
to review it. Delete the completed plan only after the user confirms that the
review is complete and explicitly authorizes deletion. When deletion is
authorized, remove or revise this routing section in the same change so that
`AGENTS.md` does not retain a dangling plan reference.
