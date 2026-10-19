# Test-Run Evidence Records

This directory holds small, reviewable records of test runs. Raw logs,
firmware, plots, traces, CSV files and reports stay outside model context
under the ignored `logs/` tree; a record references them by path, size and
SHA-256.

A record is evidence for the requirement entries it names, at the fingerprints
it names. It does not waive a gate, authorize hardware operation, or clear an
aircraft for flight. Only `reqtool` writes records (`reqtool record`,
`reqtool test`, and the gate tools that call it); nobody edits one by hand.

## Records

Schema 3 records live at:

```text
runs/YYYY/MM/YYYYMMDDTHHMMSSZ__<method>__<board-or-host>__NN.json
```

A record is one session: one or more methods run together. It holds:

- the UTC time, the git revision, and whether tracked files had uncommitted
  changes;
- the board, for mcu, bench and flight evidence;
- one result per requirement entry that names a method that ran: the
  requirement, method, level, target, setup, the entry's fingerprint, and
  `pass` or `fail`;
- optionally, `supersedes` with the user's `ruling`, and a `note`;
- `details`: how the run was done (required for board evidence).

A fingerprint covers every source file of the path crates in the dependency
closure, each registry crate's checksum, the toolchain file, the
requirement's statement and verification entry, and a procedure's own
section; board evidence takes the board's whole firmware app. `reqtool`
derives it; nobody lists components.

## Details

`details` carries what each procedure's "Required evidence" asks for:

- `execution` — `performed_by` (`user`, `user-with-agent-assistance`,
  `agent`) and `user_confirmed`;
- `candidate` — the image's `firmware_sha256`, `features`, `target_triple`,
  `profile`, and relevant `configuration`;
- `conditions` — `propellers` and `actuator_power`, and the `equipment` and
  `environment`;
- `commands` with exit codes, never their output;
- `artifacts` — files under `logs/` by `id`, `kind` and `path`; `reqtool`
  hashes them;
- bounded `measurements`, `observations`, `limitations`, and any
  `stop_conditions_triggered`.

## Hardware Boundary

`reqtool` refuses, when recording and when reading records back:

- a pass that includes a triggered stop condition;
- mcu, bench or flight evidence that is agent-only, lacks the user's
  confirmation, lacks the image's SHA-256, or lacks a known power and
  propeller state;
- a props-off pass without propellers removed, and a flight pass without
  them installed.

Recording a failure, abort, or stop condition is valuable evidence. Never
change a result to make a gate appear complete.

## Artifact Rules

- Reference individual files under `logs/`, never directories.
- Do not embed logs, samples, command output, binary data, base64, plots, CSV
  rows, or report bodies in a record.
- Keep observations and measurements to what evaluating the run needs.
- Do not store credentials, tokens, private network details, operator names,
  precise flight locations, or other unnecessary personal data.

A referenced artifact may be absent, because a clean checkout has no ignored
logs. When it is present, its size and SHA-256 must still match.

## History

The schema 1 and 2 records of the earlier evidence system stay in `runs/`,
unchanged. Each has a schema 3 counterpart, `<run_id>__imported.json`,
re-fingerprinted at the record's own revision with its details carried over;
where that revision is outside this repository, or its crates or procedure
section did not exist yet, the fingerprint is unavailable and a pass reads
as stale. `../EVIDENCE_INDEX.md` locates evidence from before run records.
