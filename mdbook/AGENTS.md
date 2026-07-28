# mdBook Documentation Instructions

These instructions apply under `mdbook/` in addition to the repository root
rules.

## Scope and authority

`src/` is the canonical public documentation set for users and contributors.
Keep `src/SUMMARY.md` as its only maintained index. Update the existing
authoritative chapter instead of adding a competing guide or summary.

Route content by purpose:

- user installation, flashing, configuration, and log workflows:
  `src/user/`;
- contributor setup and workflow: `src/developer_getting_started.md` and
  `src/contributing.md`;
- current public board and feature support: `src/current_support.md`;
- planned public work: `src/roadmap.md`;
- release history: `src/changelog.md`;
- licensing, disclaimer, and publication status: `src/publication_status.md`;
- architecture and subsystem explanations: the narrowest existing topic
  chapter.

Do not copy agent instructions, live handoffs, internal TODOs, machine-enforced
test metadata, raw test evidence, decision provenance, or archives into the
book. Do not direct users or ordinary contributors to `project_meta` as a
documentation entry point.

## Editing rules

- Keep the root README concise and link it to the relevant book chapter.
- Preserve explicit prototype limitations and certification-aligned language.
- Do not turn a prior test result into a general support or safety claim.
- Keep board-specific facts distinct from reusable behavior and clearly label
  target-specific limitations.
- Use relative links that work in both the source tree and generated book.
- Update `src/SUMMARY.md` in the same change when adding, moving, or removing a
  chapter.
- Do not edit generated output under `book/`.
- Preserve generated Mermaid assets and their retained license headers unless
  intentionally regenerating them with the pinned documentation tool.

## Verification

For documentation changes, run:

```text
mdbook build mdbook
python tools/check_repository_context.py
```

Run the relevant repository tests when documentation paths are referenced by
catalogs, packaging scripts, CI, or machine-enforced routing.
