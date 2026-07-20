# FerroWasp Public Repository Checklist

This checklist prepares FerroWasp for publication as an experimental,
safety-oriented research prototype. Publication of source code is not a
firmware release, airworthiness statement, or recommendation to fly.

## Publication blockers

- [ ] Rotate the probe/debug credential that appeared in reachable private Git
  history. Removing it from the current tree is not sufficient.
- [ ] Publish from sanitized history. The preferred route is a fresh
  squashed/orphan `main` containing only the reviewed current tree.
- [ ] If any old history is retained instead, explicitly confirm that all
  project-owned historical revisions are being offered under Apache-2.0; early
  repository revisions carried different publication terms.
- [ ] Ensure obsolete refs containing raw `python_sandbox` CSV and Saleae
  captures are not present in the public repository.
- [ ] Consolidate the intended source files into an intentional publication
  commit and require a clean worktree.
- [ ] Publish through Git from the reviewed index, not by uploading a workspace
  archive containing ignored local logs, captures, or configuration.
- [ ] Run a history-aware secret scanner such as `gitleaks` or `trufflehog` on
  the proposed public history.

History rewriting, remote-ref deletion, and credential rotation are explicit
maintainer operations. Do not perform them as an incidental cleanup step.

## Source and CI

- [x] All reusable-crate host tests and doc tests pass with `--locked`.
- [x] Workspace Clippy passes with warnings denied.
- [x] FCU3 default DShot, explicit PWM fallback, and fault-injection checks
  pass.
- [x] Foxeer F405 V2 and NUCLEO-F401RE release checks pass.
- [x] Every isolated firmware package passes its own rustfmt check.
- [x] mdBook is included in pull-request CI and builds locally.
- [ ] Workflow push/deploy branches match the final default branch (`main`).
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
- [ ] Setup prerequisites and known-good build commands work from a fresh
  clone.

## Security, licensing, and repository hygiene

- [ ] A concrete private vulnerability-reporting path is enabled and linked.
- [x] Apache-2.0 project licensing and third-party Mermaid notices are clear.
- [x] The maintainer selected `Kaldstrand` as the public copyright/author
  identity used by LICENSE, NOTICE, Cargo metadata, and documentation.
- [x] `.gitignore` excludes local credentials, logs, captures, build outputs,
  and private keys without hiding required examples.
- [x] `.gitattributes` pins text line endings and marks binary assets.
- [x] `CHATGPT_REPO_OVERVIEW.md` is intentionally ignored as a local
  presentation/session artifact and is not part of the public snapshot.

## Final clean-room check

- [ ] Clone the proposed public repository into a new directory.
- [ ] Follow only the published prerequisites and build instructions.
- [ ] Run formatting, tests, strict Clippy, all supported firmware checks, and
  mdBook.
- [ ] Re-run secret and large-object scans against all public refs.
- [ ] Confirm the default branch, repository description, topics, license,
  security reporting, Pages deployment, and branch protection in the hosting
  service.
