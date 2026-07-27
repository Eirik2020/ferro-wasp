# Codex Repository Review Playbook

## Purpose

Use this document to review a software repository for:

1. **AI/Codex readiness** - how easily an agent can understand, modify, and validate the codebase.
2. **Token efficiency** - how much unnecessary context, exploration, output, and rework the repository causes.
3. **Engineering quality** - architecture, maintainability, testability, security, and change control.
4. **Operational safety** - whether agent-generated changes can be bounded, reviewed, and proven correct.

This is a review specification, not a request to refactor the repository. Do not modify files unless explicitly instructed.

---

## Review operating rules

- Start with repository metadata and targeted search; do not recursively read every file.
- Treat checked-in source, tests, schemas, build scripts, and CI as stronger evidence than descriptive prose.
- Prefer concise command output. Redirect large logs to files and inspect only relevant failures or summaries.
- Do not read generated, vendor, dependency-cache, build-output, binary, telemetry, or archive directories unless needed to verify a specific finding.
- Distinguish confirmed findings from inference.
- Cite every finding with concrete file paths, symbols, commands, or configuration keys.
- Do not recommend documentation when an executable test, lint rule, schema, or script would enforce the requirement better.
- Avoid generic advice. Every recommendation must be tied to observed repository evidence.
- Prioritize changes that reduce ambiguity, repeated exploration, failed attempts, and review burden.

---

## Required review outcome

Produce a repository review that answers:

1. Can a new Codex session quickly determine what the system is, where responsibilities live, and how to validate a change?
2. Are instructions concise, layered, current, and placed near the code they govern?
3. Are setup, build, test, lint, generation, and acceptance commands deterministic and discoverable?
4. Does the repository encourage targeted context retrieval, or does it force broad reading and repeated rediscovery?
5. Are generated code, external dependencies, logs, and large artifacts clearly separated from authored source?
6. Are changes reviewable, testable, secure, and appropriately bounded?
7. What are the largest likely token drains?
8. Which improvements provide the highest value for the least effort?

---

# Review procedure

## Phase 1 - Establish scope and inventory

Inspect only enough repository structure to identify:

- primary languages and frameworks;
- packages, crates, services, applications, boards, or targets;
- entry points;
- build and package managers;
- test directories and test frameworks;
- CI workflows;
- documentation entry points;
- code-generation paths;
- generated, vendor, build, data, binary, and archive directories;
- security- or safety-sensitive subsystems.

Recommended first-pass evidence:

```text
Repository root listing
README and root instructions
Build/package manifests
Task runner files
CI workflow names
Documentation index
Top-level source and test directories
```

Do **not** open all source files during this phase.

### Phase 1 output

Create a compact repository map:

| Area | Purpose | Entry points | Validation | Notes |
|---|---|---|---|---|
| Example subsystem | What it owns | Key paths/symbols | Relevant commands/tests | Generated, sensitive, unclear, etc. |

Identify any areas whose ownership or dependency direction cannot be determined from repository evidence.

---

## Phase 2 - Review agent instructions

Search for:

- `AGENTS.md` and nested instruction files;
- contributor guidance;
- coding-agent rules;
- task templates;
- pull-request and code-review rules;
- local instructions embedded in scripts or CI.

Evaluate whether the root instruction file provides a **map and operating contract**, rather than a complete manual.

A good root instruction file should normally contain:

- a one-paragraph repository purpose;
- authoritative documentation links;
- key directories and ownership boundaries;
- setup and validation commands;
- generated/vendor directories to avoid;
- critical invariants and prohibited changes;
- expected final report format;
- references to nested instructions where local rules differ.

Flag:

- excessive length or duplicated rules;
- stale paths or commands;
- architecture copied into instructions instead of linked;
- style guidance already enforced mechanically;
- volatile project status;
- conflicting instructions;
- subsystem rules placed only at the root;
- missing guidance for generated or safety-sensitive code.

### Desired hierarchy

```text
Root AGENTS.md
  -> repository-wide rules and navigation
Nested AGENTS.md
  -> subsystem-specific commands, constraints, and ownership
Authoritative docs/tests/scripts
  -> detailed and executable source of truth
```

---

## Phase 3 - Review documentation as the system of record

Determine whether the repository has authoritative, discoverable sources for:

- architecture and dependency direction;
- public interfaces and contracts;
- setup and development workflow;
- testing strategy and validation tiers;
- configuration and schema rules;
- security or safety invariants;
- code generation and regeneration;
- deployment or release process;
- major design decisions;
- ownership and maintenance status.

Look for a documentation index that identifies which documents are authoritative.

Flag:

- contradictory documents;
- duplicate explanations with no clear authority;
- design facts stored only in issues, chat transcripts, or comments;
- documents that describe old commands or removed components;
- large unstructured documents that Codex must repeatedly search;
- undocumented boundaries that require reading implementation details to infer architecture.

Prefer recommendations that create a small routing layer, such as:

```text
README.md          Human introduction
AGENTS.md          Agent operating contract
ARCHITECTURE.md    Domains, layers, and dependency rules
docs/index.md      Authoritative document map
docs/decisions/    Durable architectural decisions
```

---

## Phase 4 - Review environment and command determinism

Identify the canonical commands for:

1. clean-checkout setup;
2. formatting check;
3. lint/static analysis;
4. targeted unit test;
5. package/subsystem test;
6. full repository verification;
7. code generation and clean regeneration check;
8. security/dependency audit;
9. hardware, integration, HIL, or end-to-end validation where relevant.

Evaluate whether commands are:

- documented in one authoritative location;
- reproducible from a clean checkout;
- non-interactive where practical;
- scoped for fast feedback;
- consistent locally and in CI;
- explicit about required tools, versions, features, targets, and environment variables;
- concise in normal output while preserving full artifacts for diagnosis;
- able to return meaningful non-zero exit codes.

Flag token drains such as:

- Codex having to infer commands from CI YAML;
- several near-equivalent scripts with unclear authority;
- full test suites required for every small change;
- environment repair during each task;
- unbounded compiler, test, or telemetry output;
- tests that fail nondeterministically;
- generated files that change without a reproducible generator.

Recommend a small command surface, for example:

```text
setup
check-fast
check-package <name>
test-targeted <selector>
verify-full
generate
generate-check
```

Use the repository's existing task runner where possible; do not introduce a new one without a clear benefit.

---

## Phase 5 - Review context and token efficiency

Assess how much context Codex must consume before it can safely act.

### Efficient characteristics

- clear package and directory ownership;
- stable naming and predictable layout;
- searchable symbols and narrow interfaces;
- concise authoritative documentation;
- small modules with explicit responsibilities;
- tests located near the behavior they specify;
- machine-readable manifests and schemas;
- generated and external code excluded by default;
- scripts that summarize large artifacts;
- task prompts that identify goal, scope, constraints, and definition of done.

### Common token drains

Rank observed drains from highest to lowest impact:

1. **Ambiguous task or repository boundaries** - causes broad exploration and rework.
2. **Missing canonical commands** - causes command discovery and failed attempts.
3. **Large or duplicated instruction files** - consumes context before task code is loaded.
4. **Unstructured documentation** - forces repeated search and rereading.
5. **Huge logs, diffs, generated files, or data files** - crowds out relevant code.
6. **No targeted tests** - forces full-suite execution and large failure output.
7. **Hidden invariants** - produces technically valid but unacceptable changes.
8. **Cross-cutting modules** - expands the number of files needed for every task.
9. **Long-lived mixed-objective sessions** - retains irrelevant history.
10. **Excessive reasoning or parallel agents** - spends tokens without reducing uncertainty.
11. **Repeated human review feedback** - indicates a missing automated check or explicit contract.
12. **Unclear generated/authored boundaries** - encourages edits to disposable output.

For each significant drain, state:

- evidence;
- likely effect on agent behavior;
- engineering consequence;
- recommended correction;
- expected effort and benefit.

---

## Phase 6 - Review architecture for agent navigation

Evaluate:

- separation of concerns;
- dependency direction;
- public versus internal interfaces;
- configuration flow;
- error propagation;
- generated versus handwritten code;
- platform-specific versus portable logic;
- ownership of cross-cutting concerns;
- module and package size;
- circular dependencies or hidden global state.

Do not perform a full architectural redesign. Focus on structures that make changes hard to scope or prove.

Flag areas where a small task requires reading many unrelated files, or where the implementation is the only source of architectural truth.

---

## Phase 7 - Review testing and change control

Determine whether the repository supports a progression from fast local evidence to complete acceptance evidence.

Review:

- unit, component, integration, system, and hardware tests;
- regression tests for known failures;
- test naming and discoverability;
- fixtures and test-data size;
- deterministic seeds and time control;
- CI required checks;
- formatting and lint gates;
- generated-file consistency checks;
- API/schema compatibility checks;
- review ownership for sensitive areas;
- final diff and clean-tree checks.

Flag:

- tests with unclear purpose;
- only full-system tests and no narrow loop;
- tests that rely on undocumented local state;
- failures that emit excessive output without a summary;
- skipped or flaky checks with no ownership;
- safety/security invariants represented only by comments;
- AI changes allowed to bypass normal review.

Recommend that recurring review comments become tests, lint rules, schema checks, or scripts where practical.

---

## Phase 8 - Review security, permissions, and high-assurance constraints

Inspect repository evidence for:

- secret handling;
- dependency and supply-chain controls;
- network access assumptions;
- external tools or MCP connectors;
- least-privilege permissions;
- protected branches and required review;
- sensitive generated artifacts;
- unsafe code or privileged boundaries;
- threat models or safety analyses;
- release signing, reproducibility, and provenance where relevant.

Flag any workflow that grants broad write, shell, network, or credential access by default when the normal task does not require it.

For safety-critical or security-critical code, require findings to distinguish:

- compile evidence;
- static-analysis evidence;
- unit/integration evidence;
- simulation evidence;
- hardware/HIL evidence;
- operational evidence.

Do not claim stronger assurance than the available evidence supports.

---

## Phase 9 - Embedded Rust / hardware repository checks

Apply this phase only when relevant.

Review whether the repository explicitly separates:

- portable control/domain logic;
- platform or HAL backend;
- board-specific pin, interrupt, DMA, timer, and memory mapping;
- generated RTIC/application bindings;
- host simulator;
- hardware manifests and schemas;
- HIL and bench-test tooling.

Verify that hardware tasks identify:

- MCU and board target;
- exact feature set;
- toolchain version;
- memory region constraints;
- DMA accessibility and cache/coherency requirements;
- `no_std` and allocation constraints;
- approved unsafe-code boundaries;
- required validation tier.

Flag:

- routine reading of PAC/HAL-generated source when a symbol index would suffice;
- board mappings encoded only in handwritten application code;
- no machine-readable resource inventory;
- generated variants committed without deterministic regeneration;
- host tests presented as hardware validation;
- raw uLog/telemetry passed directly into model context.

For logs and telemetry, prefer a compact analysis bundle:

```text
metadata
selected signal list
summary statistics
important event windows
anomaly table
plot files
reproduction script
```

---

# Scoring model

Score each category from **0 to 5**.

| Score | Meaning |
|---:|---|
| 0 | Missing or actively obstructive |
| 1 | Ad hoc; major manual discovery required |
| 2 | Partial; important gaps or contradictions |
| 3 | Functional; reasonable workflow with avoidable friction |
| 4 | Strong; clear, deterministic, and mostly agent-ready |
| 5 | Excellent; concise, layered, executable, and continuously maintained |

## Categories and weights

| Category | Weight |
|---|---:|
| Repository map and architecture discoverability | 15% |
| Agent instructions and documentation hierarchy | 15% |
| Setup/build/test determinism | 15% |
| Context and token efficiency | 15% |
| Testing and validation quality | 15% |
| Generated/external artifact hygiene | 5% |
| Security, permissions, and change control | 10% |
| Maintainability and reviewability | 10% |

Calculate a weighted score out of 100, but do not let the number obscure critical findings. A repository with an unmitigated critical safety or security issue cannot receive an overall "ready" conclusion.

## Readiness bands

| Score | Assessment |
|---:|---|
| 85-100 | Agent-ready |
| 70-84 | Usable with targeted improvements |
| 50-69 | High friction; agent use requires close supervision |
| Below 50 | Not ready for reliable repository-scale agent work |

---

# Finding format

Use this format for every material finding:

```text
ID: AR-01
Severity: Critical | High | Medium | Low
Category: Instructions | Documentation | Commands | Context | Architecture | Testing | Security | Embedded
Evidence: file paths, symbols, command results, or configuration keys
Observation: what exists or is missing
Agent/token impact: how this causes wasted context, failed attempts, or rework
Engineering impact: correctness, maintainability, review, security, or safety effect
Recommendation: specific corrective action
Effort: Small | Medium | Large
Expected benefit: High | Medium | Low
Confidence: Confirmed | Strong inference | Tentative
```

### Severity guidance

- **Critical** - likely to permit unsafe, insecure, destructive, or unreviewable changes.
- **High** - repeatedly causes incorrect implementation, major rework, or inability to validate.
- **Medium** - meaningful friction, token waste, or maintenance risk.
- **Low** - localized clarity or efficiency improvement.

---

# Required final report

## 1. Executive conclusion

State:

- overall readiness band and weighted score;
- whether Codex can safely perform localized changes;
- whether Codex can perform repository-scale or autonomous work;
- the three most important blockers;
- the strongest existing practices.

## 2. Repository map

Include the compact system/subsystem table from Phase 1.

## 3. Scorecard

| Category | Score / 5 | Weight | Weighted result | Evidence summary |
|---|---:|---:|---:|---|

## 4. Prioritized findings

List findings in severity and impact order. Avoid repeating the same root cause under several headings.

## 5. Token-drain analysis

Provide a ranked table:

| Rank | Token drain | Evidence | Why it is expensive | Correction |
|---:|---|---|---|---|

## 6. Recommended target state

Describe the minimum practical target state for:

- root and nested `AGENTS.md` files;
- documentation index and architecture map;
- canonical command surface;
- targeted and full validation tiers;
- generated/vendor/data exclusions;
- review and security controls.

## 7. Improvement roadmap

### Immediate - approximately one day

Only high-leverage, low-effort actions.

### Near term - approximately one week

Navigation, deterministic commands, and targeted validation.

### Medium term - approximately one month

Architecture cleanup, executable constraints, automation, and measurement.

Each action must include owner type, effort, expected benefit, and dependencies.

## 8. Proposed root `AGENTS.md` outline

Provide a repository-specific outline, not a generic full file. Keep the proposed root file short and route details to authoritative sources.

## 9. Validation of the review

List:

- commands actually run;
- areas sampled;
- areas intentionally excluded;
- limitations and unavailable evidence;
- findings that require human or hardware confirmation.

---

# Review completion checklist

Before finalizing, confirm:

- [ ] The review is based on repository evidence rather than generic best practices.
- [ ] Large/generated/vendor directories were not read without a specific reason.
- [ ] Every high or critical finding includes concrete evidence.
- [ ] Recommendations are specific and avoid unnecessary new tooling.
- [ ] Token drains are linked to observable repository behavior.
- [ ] Documentation recommendations are not substitutes for executable checks.
- [ ] The report distinguishes confirmed facts from inference.
- [ ] Validation limitations are explicit.
- [ ] The roadmap is ordered by value, not by conceptual neatness.
- [ ] No repository files were modified unless modification was explicitly requested.

---

# Invocation prompt

Use the following prompt with Codex from the repository root:

```text
Read CODEX_REPOSITORY_REVIEW_PLAYBOOK.md and perform the repository review exactly as specified.

Review the current repository without modifying files. Start with a targeted inventory and search before reading implementation files. Exclude generated, vendor, dependency, build, binary, telemetry, and archive content unless needed to verify a specific finding.

Ground every material finding in file paths, symbols, configuration, or command evidence. Produce the required scorecard, prioritized findings, token-drain analysis, target state, and phased improvement roadmap. Clearly distinguish confirmed findings from inference and state all validation limitations.
```
