# Cross-Repo Sync

Last updated: 2026-07-14

## Role of this repo

`ferro-wasp` is the source of truth for the current FerroWasp prototype /
technology demonstrator.

This repo owns:

- firmware code and build configuration;
- current hardware assumptions for the active prototype;
- current motor map, gyro signs, RC mapping, gains, loop rates, and features;
- actuator-output, arming, failsafe, watchdog, and safety implementation docs;
- bench and flight evidence;
- firmware-specific tooling contracts such as BB2 logging format.

## Role of sibling repos

`..\ferro-debugger` owns Raspberry Pi probe, flashing, RTT/defmt streaming,
remote logging, link configuration, and Zero 2 W workflow implementation.

`..\ferro-planning` owns non-firmware plans, cross-repo task specs, and pruned
handoffs.

## Authority rule

Planning material can summarize or propose, but it does not override this repo's
current implementation state.

Before changing safety-relevant firmware behavior, verify this repo's current:

- `project_docs/CODEX_PROJECT_CONTEXT.md`
- `project_docs/CODEX_ACTIVE_WORK.md`
- `project_docs/testing/README.md` and the catalog-selected procedure
- relevant code and logs

Before changing remote probe or Pi logging behavior, verify the owning files in
`..\ferro-debugger`.

Before changing non-firmware planning or cross-project strategy, verify the
owning files in `..\ferro-planning`.

## Do not duplicate here

Keep these in `..\ferro-planning` unless they directly affect firmware work:

- non-firmware planning;
- general cross-repo roadmaps;
- raw ChatGPT/Codex handoff imports.

Keep these in `..\ferro-debugger`:

- Pi service installation details;
- probe server configuration;
- named link implementation;
- remote logging script implementation.

## Sync closeout

When firmware implementation facts change, update this repo first. Then update
`..\ferro-planning` only if the project-level plan or cross-repo decision also
changed.
