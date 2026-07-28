# Host Tooling Instructions

These instructions apply under `tools/` in addition to the repository root
rules. The RTIC app builder has additional instructions in
`rtic-app-builder/AGENTS.md`.

## Scope and safety

Host tools may build, flash, observe, configure allowed values, download logs,
and analyze evidence. They do not own firmware safety authority and must not
create a path around arming, actuator gating, or target health checks.

- Default to read-only observation where practical.
- Make flashing, erase, reset, configuration writes, and other state changes
  explicit in the CLI and user-visible output.
- Require confirmation for destructive log/storage actions and preserve
  resumable or recoverable behavior where available.
- Never infer a successful flash from an intermediate milestone; require the
  documented terminal success condition and nonzero failure exits.
- Select boards, apps, artifacts, features, links, and log identities
  explicitly. Do not silently fall back to another target or stale ELF.
- Treat matching firmware/ELF hashes, CRCs, sequence continuity, units, and
  schema versions as evidence boundaries.
- Preserve retained logs and input files. Use unique outputs or require
  explicit overwrite intent.

Read `../project_meta/CODEX_ACTIVE_WORK.md` before changing current logging,
download, analyzer, remote-debug, or configuration workflows. Read
`../project_meta/CROSS_REPO_SYNC.md` before changing ownership between
FerroWasp and FerroDebugger.

## Implementation rules

- Keep parsers bounded and reject malformed, truncated, mismatched, or
  unsupported input without partial success claims.
- Keep CLI behavior scriptable: stable exit codes, concise diagnostics, and
  deterministic output where practical.
- Maintain compatibility deliberately; do not change public wire/log formats
  or established CLI contracts without approval.
- Separate acquisition, validation, conversion, and analysis so conversion
  cannot make corrupt evidence appear valid.
- Do not add network, package-install, probe, or hardware assumptions to
  software-only tests.

## Verification

Run focused tests for the changed tool, then:

```text
python -m unittest discover -s tools/tests -p "test_*.py" -v
python tools/check_repository_context.py
```

When a tool invokes Cargo, probe-rs, remote hardware, or a sibling repository,
test its command construction independently where possible. Report unavailable
external tooling or hardware rather than weakening checks.
