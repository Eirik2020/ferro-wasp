# Releasing

How a FerroWasp release is cut. The repository has been public since the
`v0.1.0` pre-release; the checklist that took it there is archived.

## Release Metadata

`tools/release/release.toml` names the version, the tag, whether the GitHub
release is a pre-release, and the release notes under `.github/releases/`.
`tools/release/check_release.py` validates it, and the reusable crates, the
firmware apps, and FerroConfigurator all carry that same version.

## Cutting a Release

1. Update the version everywhere, `release.toml`, and the release notes, and
   commit the reviewed candidate.
2. From a clean worktree, run
   `python tools/release/check_release.py --require-clean`.
3. Create the annotated tag that `release.toml` names, and push it.
4. The tag runs CI's release job, which needs the firmware, host-test,
   publication-security, FerroConfigurator, and documentation jobs to pass
   first. Confirm the GitHub release holds the Windows ZIP and its matching
   `.zip.sha256`.
5. Clean-room check: clone the tag into a new directory, follow only the
   published prerequisites and build instructions, and run formatting, tests,
   strict Clippy, every supported firmware check, and the book build there.

## Publication Rules

- Publish through Git from the reviewed index, never by uploading a workspace
  archive, which would carry ignored logs, captures, or configuration.
- CI's publication-security job scans the whole reachable history with
  Gitleaks and `tools/release/check_public_history.py` rejects blobs over
  `maximum_public_blob_bytes` or with prohibited suffixes.
- History rewriting, remote-ref deletion, and credential rotation are explicit
  maintainer operations, never part of an incidental cleanup.
- The "Protect main" ruleset requires pull requests with resolved
  conversations, squash or rebase merges, linear history, and the Firmware
  check, Host workspace tests, and Documentation checks, and forbids deletion
  and force-pushes.
