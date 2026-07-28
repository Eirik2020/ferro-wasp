# Documentation Policy

The mdBook is the canonical home for FerroWasp user and developer
documentation. Its [Summary](SUMMARY.md) is the maintained documentation
index.

Use the following boundaries:

- The root README is a concise project landing page.
- User workflows, developer setup, architecture, board support, interfaces,
  testing concepts, and the roadmap belong in this book.
- Crate, app, and tool READMEs may document a narrow package contract, but
  should link here instead of duplicating general guides.
- `project_meta` is reserved for agent context, machine-enforced test and
  evidence metadata, design-decision provenance, internal work tracking, and
  archives. It is not a user or developer entry point.
- Agent instructions remain next to the files they govern.
- Legal distribution files remain at the repository root. GitHub-recognized
  policy pointers and per-tag release-delivery metadata live under `.github/`.
  Canonical contribution guidance and release history remain in this book.

When a public behavior, workflow, or supported feature changes, update the
relevant chapter and the Summary in the same change. Do not add a second
standalone guide elsewhere in the repository.
