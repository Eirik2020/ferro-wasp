# Requirements, Evidence and Safety Architecture: Design

This document records the design of FerroWasp's requirements and verification
system, and the firmware architecture that system depends on: why each
decision was made, which alternatives were rejected and why, and what in the
code motivated it. The binding records are ADR-0050 to ADR-0060
([index](../ARCHITECTURE_DECISIONS.md)); this document carries the reasoning
behind them. Where the two differ, the ADR governs and this document is
corrected.

The requirements and evidence system (sections 2 to 7) is implemented in
`tools/reqtool`, except the `app!` parser and the analysis-level checks; the
architecture (sections 8 to 16) is not implemented yet. The code findings
quoted in the chapters describe the code the design was made against, as
motivation; they are not a description of the target.

## Contents

The design is split into chapters; section numbers run across all of them.

1. [Goals and approach](#1-goals-and-approach)
2. [The requirements and evidence model](evidence.md#2-the-requirements-and-evidence-model)
3. [Verification levels and gates](evidence.md#3-verification-levels-and-gates)
4. [Fingerprints](evidence.md#4-fingerprints)
5. [Verification modes and timing](evidence.md#5-verification-modes-and-timing)
6. [Schema, tooling and the parser](schema-and-tooling.md#6-schema-tooling-and-the-parser)
7. [Governing requirements](schema-and-tooling.md#7-governing-requirements)
8. [Safety-critical and non-critical code](architecture.md#8-safety-critical-and-non-critical-code)
9. [Links between tasks](architecture.md#9-links-between-tasks)
10. [The observer and the ledger](architecture.md#10-the-observer-and-the-ledger)
11. [The coordinator](architecture.md#11-the-coordinator)
12. [Hardware contracts](hardware-and-loop.md#12-hardware-contracts)
13. [The gyro-synced control loop](hardware-and-loop.md#13-the-gyro-synced-control-loop)
14. [Stored configuration](configuration-and-crates.md#14-stored-configuration)
15. [Classifications](configuration-and-crates.md#15-classifications)
16. [Crate layout](configuration-and-crates.md#16-crate-layout)
17. [Order of work](#17-order-of-work)
18. [Open items](#18-open-items)

---

## 1. Goals and approach

The system must give machine-checkable traceability between requirements,
code and test evidence. It must be cheap for agents to query, with no silent
drift, and reviewable by people. It is kept deliberately loose until the
master's thesis and the FerroWasp 1.0 milestone, and then solidified.

From the starting plan (`ferrowasp-requirements-plan.md`, in the workspace
root), these points carried through unchanged:

- three documentation layers: user documentation and governance rules are
  Markdown prose; requirements and verification data are YAML (deep nesting:
  function → task → task group → groups of groups, with per-chip variation),
  with CSV for flat data such as bring-up checklists;
- agents edit raw files, and people review diffs against the original
  wording, so drift in paraphrase is caught;
- schema enforcement through Rust `serde` structs with `deny_unknown_fields`
  and enums for constrained fields, run in CI, so no field can be invented;
- third-party crates are recorded as `trust: external`, pinned to an exact
  version, and contract-tested at their interface;
- the tool is Rust (fast startup for frequent agent calls, one binary, shared
  types and toolchain), a library with a thin CLI, in this repository;
- the first three goals: a minimal `reqtool` and schema; the Command Input
  requirement, tagging its existing tests and exposing its validation gap;
  the Foxeer F405 V2 bring-up as the first board in the bring-up matrix;
- later: an MCP server (via `rmcp`) as separate, composable requirements,
  developer and user servers; a YAML-to-ReqIF converter if a certifier needs
  exchange; an optional web view. ReqIF is never the working format.

**Clean sheet.** The design is new, not an extension of the system it
replaces: `COMPONENTS.json`, `TEST_CATALOG.json`, the schema 2 run records and
`tools/components.py` (ADR-0038). Old parts are reused only where they earn it
on their own merits. What survives: W7's principle that evidence stands while
what it covered is unchanged; procedures as Markdown, because people run them;
raw artifacts kept in `logs/` with records holding only path and hash.

The discussion that produced this design turned, from its first case, into a
redesign of the firmware's structure, because a requirement has to be
allocated to something, and the structure it would be allocated to was the
obstacle. The two tracks are kept apart in the work that follows (section 17).

---

## 17. Order of work

1. Record the decisions: ADR-0050 to ADR-0060 and this document, on the
   `requirements-system` branch. The governing-document edits (W1-W7 IDs, W7,
   W4) land with the first schema on that branch.
2. Goal 1: a minimal `reqtool` and schema — define a requirement, link it to
   an item, `check` it — coexisting with the old system.
3. The `app!` parser in `ferroforge-contracts`, and a FerroForge release.
   `reqtool` can start with function paths and add tasks when the release is
   out.
4. The architecture redesign, as its own stack of changes on `main`. It
   changes the flight image and re-gates the Foxeer, and it is kept apart from
   the requirements tooling.

Then goal 2 (the Command Input requirement and its tests) and goal 3 (the
Foxeer F405 V2 bring-up matrix and the first test of shared-driver
verification).

Deferred, from the plan and this design: the devboard and peripheral-simulator
stages; formal substitution and risk-tailoring rules (EASA SORA, AADL, SysML),
during flight-termination and flight-controller work; MCP servers; ReqIF
export; the web view; the Betaflight-target converter, its board trust tiers
(certified, manufacturer-supported, legacy; imported boards start unverified),
and a legal review of it (GPLv3 source: extract facts into an intermediate
structure and generate FerroWasp's own format, never copy text) before it
ships. The converter stays in the open fork; a professional or defence fork
keeps its own hardware list and never ingests imported targets.

---

## 18. Open items

- **USB-host arming refusal.** ADR-0042 refuses arming while a USB host is
  connected or within 2 s of a configurator-link command, by reading flags
  written by non-critical tasks (`USB_HOST_CONNECTED`,
  `CONFIGURATOR_LINK_ACTIVE`). That breaks the flag rule. It moves either to
  an inhibit lease each link holds through the coordinator or to a
  safety-critical VBUS sense; decided during the redesign.
- **Discontinuity handling in the RC group.** The RC group observes
  ADR-0011's out-of-band handle and calls the parser's `reset` (`rc_input`);
  its fault decisions move into host-tested functions on the
  `rc-input-fault-decisions` branch, which INPUT-006 links once merged.
- **Feature-gated tests.** `reqtool test` runs each package with its default
  features, so a test behind an opt-in feature cannot be evidence yet.
- **Datasheet rates.** The MPU-6000 and ICM-42688-P output rates above are
  recalled, not checked.
- **Betaflight's scheduler.** Gyro-locking is to be confirmed in its source.
- **Stack-analysis tooling** for the image-level check, chosen at
  implementation.
- **Document sizes.** Size limits are met by restructuring, never by
  shortening content. The design is split into chapters and the ADR index
  into active and historical decisions, so the index meets its original
  8,192-byte limit. Budgets were raised where a document only grows: the
  internal book's summary (8 KB), the document registry (24 KB), and
  `AGENTS.md` (13,312 bytes; its rules stay in the one file every agent
  reads). The always-loaded files are at 296 of 300 lines.
