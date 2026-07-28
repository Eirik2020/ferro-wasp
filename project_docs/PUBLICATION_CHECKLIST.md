# FerroWasp Public Repository Checklist

This checklist prepares FerroWasp for publication as an experimental,
safety-oriented research prototype. Publication of source code is not a
firmware release, airworthiness statement, or recommendation to fly.

## Publication blockers

- [x] Rotate the probe/debug credential that appeared in reachable private Git
  history. Removing it from the current tree is not sufficient.
- [x] Prepare the publication repository from sanitized history: a parentless
  `main` containing only the reviewed current tree.
- [x] Keep the old history out of the publication repository. The separate
  historical archive remains private, so historical revisions with earlier
  publication terms are not being offered as part of the public repository.
- [x] Ensure obsolete refs containing raw `python_sandbox` CSV and Saleae
  captures are not present in the public repository.
- [x] Consolidate the intended source files into an intentional publication
  commit and require a clean worktree.
- [x] Publish through Git from the reviewed index, not by uploading a workspace
  archive containing ignored local logs, captures, or configuration.
- [x] Run a history-aware secret scan on the proposed public `main`. Gitleaks
  scanned the single reachable commit with no findings.

History rewriting, remote-ref deletion, and credential rotation are explicit
maintainer operations. Do not perform them as an incidental cleanup step.

## Source and CI

- [x] All reusable-crate host tests and doc tests pass with `--locked`.
- [x] Workspace Clippy passes with warnings denied.
- [x] FCU3 standard DShot and fault-injection checks
  pass.
- [x] Foxeer F405 V2 and NUCLEO-F401RE release checks pass.
- [x] Every isolated firmware package passes its own rustfmt check.
- [x] mdBook is included in pull-request CI and builds locally.
- [x] Workflow push/deploy branches match the final default branch (`main`).
- [x] The Rust toolchain and safety-critical HAL revision are pinned.

## Public documentation

- [x] README and mdBook describe DShot600 as the FCU3 default and PWM as an
  explicit fallback.
- [x] PA10/USART1 legacy BLHeli telemetry, the ESC manager, and
  telemetry-qualified arming are described accurately, including DShot-only
  activation, queued-response quarantine, timeout behavior, and the five-second
  manager boot delay.
- [x] Logical motor names are clearly separated from physical output lanes and
  pins.
- [x] The open IMU arming limitation is explicit: initialization, calibration,
  and freshness are not yet pre-arm prerequisites, while the first post-arm
  stale-IMU check requests disarm.
- [x] Current status records the successful controlled experimental flight
  without closing the outstanding DShot waveform measurements.
- [x] Historical PWM-era plans are archived or clearly labeled as historical.
- [x] Publicly cited evidence is either included in a small sanitized evidence
  set or labeled as privately retained and unavailable in the repository.
- [x] Setup prerequisites and known-good build commands work from a fresh
  clone.

## Security, licensing, and repository hygiene

- [ ] Enable GitHub private vulnerability reporting after the repository is
  public. `SECURITY.md` already links the intended private reporting path.
- [x] Apache-2.0 project licensing and third-party Mermaid notices are clear.
- [x] The maintainer selected `Kaldstrand` as the public copyright/author
  identity used by LICENSE, NOTICE, Cargo metadata, and documentation.
- [x] `.gitignore` excludes local credentials, logs, captures, build outputs,
  and private keys without hiding required examples.
- [x] `.gitattributes` pins text line endings and marks binary assets.
- [x] `CHATGPT_REPO_OVERVIEW.md` is intentionally ignored as a local
  presentation/session artifact and is not part of the public snapshot.

## Final clean-room check

- [x] Clone the proposed public repository into a new directory.
- [x] Follow only the published prerequisites and build instructions.
- [x] Run formatting, tests, strict Clippy, all supported firmware checks, and
  mdBook.
- [ ] Re-run secret and large-object scans against all public refs.
- [x] Confirm `main` as the default branch and configure the repository
  description, topics, and recognized Apache-2.0 license.
- [x] Protect `main` with pull requests, required CI checks, conversation
  resolution, linear history, and deletion/force-push protection. Only squash
  and rebase merge methods are allowed.
- [ ] After changing visibility, enable private vulnerability reporting and
  Pages deployment from `gh-pages`, then verify both public surfaces.
