# The Internal Book

This is FerroWasp's internal book, for maintainers and agents. It is never
published. Users and contributors read the public book, `mdbook/`, instead,
and no public chapter links here.

The root `AGENTS.md` defines the authority order and maps tasks to chapters.
Below it and [the governing requirements](governing-requirements.md), current
code, manifests, and tests outrank every narrative document here.

- [Governing requirements](governing-requirements.md): W1-W9.
- [Remaining work](remaining-work.md): what is left to build.
- [Architecture decisions](ARCHITECTURE_DECISIONS.md): the ADR index.
- [Testing](testing/README.md): test selection, target procedures, and the
  evidence workflow.
- [Logging format](LOGGING_FORMAT.md): what the blackbox must carry.
- [Publication checklist](PUBLICATION_CHECKLIST.md).

Retained historical evidence is located through
[the evidence index](testing/EVIDENCE_INDEX.md). Moving material to the archive
does not weaken its evidence status, though historical evidence does not
describe current defaults. Preserve artifact identities, hashes,
dates, feature sets, and limitations. `archive/` is restricted; see `AGENTS.md`.
Immutable archives may retain the former `project_docs` path; those strings are
provenance, not routing.

## Context Guard

`DOCUMENT_REGISTRY.json` registers every Markdown file here with a lifecycle,
a context class, and for entry points a reviewed byte budget. Read a
`targeted` file only when the task needs it; an `exclude` file is never default
context. Run:

```text
python tools/check_repository_context.py
```

It checks registration, lifecycle and context values, byte budgets, the ADR
index, the test catalog and evidence records, and the always-loaded line limit.
It does not interpret prose, delete history, or authorize firmware behaviour.
