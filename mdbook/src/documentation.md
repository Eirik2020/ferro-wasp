# Documentation Policy

The mdBook is the canonical home for FerroWasp user and developer
documentation. Its `SUMMARY.md`, the sidebar, is the maintained documentation
index.

Use the following boundaries:

- The root README is a concise project landing page.
- User workflows, developer setup, architecture, board support, interfaces,
  testing concepts, and the roadmap belong in this book.
- Crate, app, and tool READMEs may document a narrow package contract, but
  should link here instead of duplicating general guides.
- `project_meta` is an internal maintainers' book, never published: governing
  requirements, design decisions, remaining work, and the machine-checked test
  and evidence records. It is not a user or developer entry point.
- `archive/` keeps retired documents verbatim, for provenance only.
- Agent instructions are one root `AGENTS.md`.
- Legal distribution files remain at the repository root. GitHub-recognized
  policy pointers and per-tag release-delivery metadata live under `.github/`.
  Canonical contribution guidance and release history remain in this book.

Each topic has one owning chapter; other chapters link to it rather than
restating it. When a public behavior, workflow, or supported feature changes,
update the owning chapter and the Summary in the same change. Do not add a
second standalone guide elsewhere in the repository.
