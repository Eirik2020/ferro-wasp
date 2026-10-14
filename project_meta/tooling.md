# Repository Tooling

The maintainer tools that check the repository or route an agent's reading.
None of them changes firmware, operates hardware, or decides that a test passed.
The bench gate runners belong to their procedures in
[testing](testing/README.md), and evidence records to
[run evidence](testing/evidence/README.md).

## App Context Router

`tools/app_context.py` is a read-only map of a flight app's RTIC shell. For a
topic it reports the relevant task and helper line ranges, priorities,
interrupt bindings, feature gates, resource blocks, companion files, and the
test chain, without printing the whole source. Output is capped at 8 KiB.

```text
python tools/app_context.py --list-topics
python tools/app_context.py --board foxeer-f405-v2 --topic arming
python tools/app_context.py --validate
```

`--validate` fails on a missing symbol, companion path, or test ID, an
unassigned RTIC task, or a route over budget. The test chain is routing, not a
statement of which tests apply or passed.

## Checks

- `tools/check_repository_context.py` validates the document registry and byte
  budgets, the always-loaded line limit, the internal book's summary, the ADR
  index and records, the immutable archive hashes, the test catalog, run
  evidence records, and the app context routes.
- `tools/test_evidence.py` runs the evidence-record checks alone.
- `tools/check_rtic_boundaries.py` enforces the thin-app rules mechanically:
  RTIC imports only through the app facade, only RTIC declarations in
  `main.rs`, shared board data models, no timer-register sequencing in board
  support, and no source copied verbatim between boards.
- `tools/release/check_release.py` checks the release metadata, and
  `check_public_history.py` rejects oversized or prohibited files reachable from
  Git refs.

All of them run in CI.
