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

- `tools/run_tests.py` runs every test suite, locally and in CI, and counts the
  tests each ran; `--list` shows what each suite runs and why it is separate.
- `tools/check_repository_context.py` validates the document registry and byte
  budgets, the always-loaded line limit, the internal book's summary, the
  immutable archive hashes, and the app context routes.
- `tools/reqtool` checks the requirements, gates and evidence records:
  `reqtool validate`, `reqtool check`, `reqtool gate`.
- `tools/check_rtic_boundaries.py` enforces the thin-app rules mechanically:
  RTIC imports only through the app facade, only RTIC declarations in
  `main.rs`, shared board data models, no timer-register sequencing in board
  support, and no source copied verbatim between boards. It also enforces
  section 8's direction rule from the manifests: a crate marked
  `[package.metadata.ferrowasp] critical = true` depends, including for tests
  and builds, only on other safety-critical crates and on external crates
  pinned to one version (`=x.y.z`, or a git `rev`), and a `-critical` name
  carries the marker.
- `tools/release/check_release.py` checks the release metadata, and
  `check_public_history.py` rejects oversized or prohibited files reachable from
  Git refs.

All of them run in CI. A change that touches only `project_meta/`, `mdbook/`,
`tools/reqtool/`, `AGENTS.md` or `CLAUDE.md` skips the firmware,
FerroConfigurator and RTIC app builder jobs and the root workspace's Rust tests
and lints, since none of those paths builds into them; the Python tools, the
document and boundary checks, `reqtool`, the books and the publication scan
still run. Tags and manual runs run everything.
