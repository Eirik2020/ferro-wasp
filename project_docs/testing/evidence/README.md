# Test-Run Evidence Records

Use this directory for small, reviewable metadata records describing new test
runs. Test definitions remain in `../TEST_CATALOG.json`; raw logs, firmware,
plots, traces, CSV files, and reports remain outside model context under the
ignored `logs/` tree.

A run record is candidate-specific evidence. It does not change a catalog
entry's status, waive a prerequisite, authorize hardware operation, or clear
an aircraft for flight.

## Storage Layout

Copy `RUN_RECORD_TEMPLATE.json` and save the completed record as:

```text
runs/YYYY/MM/YYYYMMDDTHHMMSSZ__TEST-ID__target__NN.json
```

The filename timestamp is the UTC completion time. `NN` is a two-digit
sequence used when more than one record could otherwise share the same name.
The JSON `run_id` must equal the filename stem.

Examples of `target` are `common`, `fcu3`, and `foxeer-f405-v2`. For a common
hardware gate, record the actual board target rather than `common`.

Do not create a cumulative results file or append run results to current
procedures. Locate records by year, test ID, target, or result using filenames
and targeted search.

## Required Identity

Every record captures:

- the test ID, target, validation tier, UTC start/completion times, and result;
- the SHA-256 of the catalog and selected procedure, plus the procedure path
  and heading;
- repository revision or dirty-working-tree identity, feature set, target
  triple, firmware SHA-256 where applicable, and relevant configuration;
- who performed the work and whether the user confirmed a hardware run;
- propeller, actuator-power, equipment, and environment state;
- commands and exit codes without inline command output;
- bounded measurements and observations;
- limitations and any triggered stop conditions;
- artifact identities as repository-relative `logs/...` paths, byte sizes, and
  SHA-256 values.

Use result `pass`, `fail`, `aborted`, or `inconclusive`. A prior pass applies
only to its recorded candidate and does not remove a future test requirement.

## Artifact Rules

Raw artifacts stay ignored and are not committed as run-record content.

- Reference individual files under `logs/`; do not reference directories.
- Record each artifact's byte size and SHA-256.
- Do not embed logs, samples, command output, binary data, base64, plots, CSV
  rows, or report bodies in JSON.
- Use concise observations and measurements that are necessary to evaluate
  the catalog requirement.
- Do not store credentials, tokens, private network details, operator names,
  precise flight locations, or other unnecessary personal data.

The validator permits a referenced local artifact to be absent because a clean
checkout does not contain ignored logs. When the file is present, its declared
size and SHA-256 must match.

## Hardware Boundary

For bench, preflight, and flight records:

- the user must perform or directly participate in execution;
- `user_confirmed` must be true;
- firmware identity and explicit propeller/actuator-power states are required;
- a `pass` cannot contain an unknown power/propeller state or a triggered stop
  condition.

Recording a failure, abort, or stop condition is valuable evidence. Never
change a result to make a gate appear complete.

## Workflow

1. Select the test through `../TEST_CATALOG.json` and run its prerequisites.
2. Copy `RUN_RECORD_TEMPLATE.json` to the required dated path.
3. Replace every placeholder and remove unused example command/artifact rows.
4. Calculate file hashes without modifying the artifacts. On PowerShell:

   ```powershell
   Get-FileHash -Algorithm SHA256 -LiteralPath <path>
   ```

5. Run:

   ```text
   python tools/check_repository_context.py
   python -m unittest tools.tests.test_test_evidence -v
   ```

6. Review the record and referenced evidence before making any operational
   decision.

The system is forward-looking. Do not backfill retained history merely to
populate this directory; use `../EVIDENCE_INDEX.md` for pre-rollout evidence.
