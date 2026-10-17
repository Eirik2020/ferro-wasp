# Archived 2026-10-17: the test catalog, component registry and run-record template

Archived verbatim from their original paths:

- `project_meta/testing/TEST_CATALOG.json` → `TEST_CATALOG.json`
- `project_meta/testing/COMPONENTS.json` → `COMPONENTS.json`
- `project_meta/testing/evidence/RUN_RECORD_TEMPLATE.json` →
  `evidence/RUN_RECORD_TEMPLATE.json`

They belonged to the evidence system of ADR-0038, replaced by the
requirements system (ADR-0050 to ADR-0053, `tools/reqtool`).

What still binds was promoted before archiving:

- each test's procedure, stop conditions and required evidence stay in its
  Markdown procedure under `project_meta/testing/targets/`, which they
  duplicated;
- the prerequisite chain became the gates in
  `project_meta/requirements/gates.yaml`;
- who may run a procedure follows from its level under the Operating Rules
  in the root `AGENTS.md`;
- the hardware evidence rules of the run-record validator moved into
  `reqtool`, and a record's fields into the schema 3 `details` section;
- the component registry is replaced by fingerprints derived from the code.

The schema 1 and 2 run records stay where they were, under
`project_meta/testing/evidence/runs/`, unchanged; each has a schema 3
counterpart named `<run_id>__imported.json`.
