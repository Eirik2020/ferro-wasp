# Repository Instructions

## Authority

This file is the highest-authority document in the repository; only the user
outranks it. On conflict the order is: the user; this file;
`project_meta/governing-requirements.md` (W1-W7); all other documents. This is
the only `AGENTS.md`; its scoped rules below bind only inside their directory.

Any other document that diverges from this file or the governing requirements is
wrong; correct it immediately without asking permission. A conflict between this
file and the governing requirements needs the user's explicit decision - raise it
and wait rather than choosing a side. Where a document and the code disagree
about what the code does, the code is right and the document is fixed.

## Two Books

- **The public book**, `mdbook/` (published): for users and contributors. How
  FerroWasp works, how to build, flash, configure, and test it, what is
  supported, and where it is going. Start at `mdbook/src/SUMMARY.md`.
- **The internal book**, `project_meta/` (never published): for maintainers and
  agents. Governing requirements, decisions, remaining work, requirements,
  procedures and evidence, the logging format, and the publication checklist.

Keep agreed design, current implementation, and open proposals distinct. Keep
remaining work visible; agreement on an approach does not mean it is
implemented. READMEs are brief entry points that link into a book.

## Documentation Map

Start from the task, not the chapter. Read the named sections, not whole files.

| Task | Read |
| --- | --- |
| A flight app or RTIC wiring | W3, W4, W5; Firmware below; `python tools/app_context.py --board foxeer-f405-v2 --topic <topic>` |
| Adding or changing a board | W4, W5, W6; `mdbook/src/board_support.md` |
| Arming, motors, DShot | W1, W2, W6; `mdbook/src/arming.md`, `mdbook/src/dshot.md` |
| Selecting or running a test, recording evidence | W7; Operating Rules below; `project_meta/testing/README.md` |
| Writing or checking a requirement | W7; `tools/reqtool/README.md`; `reqtool check <item>` before changing it |
| Blackbox and logging | `project_meta/LOGGING_FORMAT.md` |
| Host tools or FerroConfigurator | W1; Host Tools below |
| Public documentation | Public Book below; `mdbook/src/SUMMARY.md` |
| Building or checking anything | `mdbook/src/testing.md` Software Checks; `.github/workflows/ci.yml` is exact |
| Whether something was already decided | the requirement or chapter that owns it; `project_meta/faq.md` |
| What is left to build | `project_meta/remaining-work.md` |
| Publishing a release | `project_meta/releasing.md` |

One topic, one owner, across both books. Specify a topic in its owning chapter;
every other mention links there instead of restating it. A public chapter never
links into the internal book.

| Topic | Owner |
| --- | --- |
| Requirements W1-W7 | `project_meta/governing-requirements.md` |
| Why a tempting alternative lost | `project_meta/faq.md` |
| What is left to build | `project_meta/remaining-work.md` |
| Test definitions, procedures, evidence workflow | `project_meta/testing/README.md` |
| Requirements and their design | `project_meta/requirements/` and its `design.md` |
| Blackbox format requirements | `project_meta/LOGGING_FORMAT.md` |
| Board and feature support | `mdbook/src/current_support.md` |
| Public direction | `mdbook/src/roadmap.md` |
| Crate map | `mdbook/src/project_structure.md` |
| Board configuration and naming | `mdbook/src/board_support.md` |
| Repository checks and the context router | `project_meta/tooling.md` |
| The dev container's agent tooling | `project_meta/dev-environment.md` |
| How a subsystem works | its `mdbook/src/` chapter |
| Commands | `mdbook/src/testing.md` and `developer_getting_started.md` |

## Operating Rules

- Ask the user before any W6 change and before any decision that sets
  structure: crate layout, where data lives, a convention other code follows.
  Lay out the options with a recommendation; small reversible fixes go ahead.
- An agent may run hardware-in-the-loop tests over SWD and USB while the system
  is disarmed and nothing actuates. Motor, powered, and flight procedures are
  the user's, including FerroConfigurator's motor test. Propellers stay off
  until the reviewed preflight prerequisites are complete. Never assume a probe
  or board is attached.
- Stop on any unexpected motor, arming, telemetry, storage, or RC-loss
  behaviour and report it; never work around it.
- `logs/` holds the only copy of every flight log and flown binary. Never
  delete or rewrite anything in it.
- Never claim a gate passed from a build. In every report, say what was
  measured and what was inferred.

## Scoped Rules

**Firmware (`firmware/`).** Read the app's `README.md`, `Cargo.toml`, and the
code you change; route localized work through `tools/app_context.py` first. Its
test chain is routing, not a pass.

- `src/main.rs` holds `ferroforge::app!` only: resources, priorities, init
  wiring, locking, spawning. No helper types, functions, macros, constants, or
  statics; it imports only the app library's facade.
- Init selects resources and calls shared constructors. Do not feature-gate
  hardware every variant has. DShot is the flight ESC protocol; RC PWM stays in
  shared crates for servos, never as an ESC fallback.
- A new shared resource or priority change states its timing and contention
  rationale; a safety-related task states its priority and timing assumptions.
  A safety-relevant change states its reason, failure behaviour, test coverage,
  that motor authority did not move, and any unsafe-code or timing effects.
- A task definition wires resources, locks, and spawns; each decision it makes
  is a host-tested function in a HAL-free crate (`mdbook/src/testing.md`).
- Before touching a `critical = true` crate, a link, a border role, or the
  safety master, read `project_meta/requirements/` sections 8-11 and 19.
- Use typed units for time, rates, voltages, and actuator commands. Prefer an
  explicit safety state machine over a clever abstraction.
- Before changing a secondary app, compare with Foxeer: arming preparation and
  guard order; disarm, RC-loss, failsafe, watchdog, and fault paths; actuator
  permits, leases, and stale-command containment; task priorities and completion
  order; logging identities and feature gates. Repeat the comparison of enabled
  features before a secondary board's bench test.
- Foxeer: keep the physical-body/controller-frame pitch boundary explicit, never
  a sign change hidden in a sensor or mixer layer. The DShot/eRPM-qualified
  arming sequence is the motor path; diagnostic features never replace it
  silently. SWD pins stay reserved.
- `stm32f401-bringup` validates peripherals without actuators; it is not a
  behaviour reference.
- Run `python tools/check_rtic_boundaries.py` after app or board changes. For a
  motor-capable image, record the commit, features, artifact hash, and propeller
  and actuator-power state.

**Host Tools (`tools/`).** Read-only by default. Flashing, erase, reset, and
configuration writes are explicit commands with visible output; destructive log
or storage actions need confirmation. Select the board, app, artifact, features,
and link explicitly, never falling back to another target or a stale ELF. Never
infer success from an intermediate milestone; failure exits nonzero. Parsers are
bounded and reject malformed or mismatched input without partial success.
Acquisition, validation, conversion, and analysis stay separate so conversion
cannot make corrupt evidence look valid. Never overwrite inputs or retained
logs. Software-only tests assume no network, probe, or hardware.

- `tools/ferro-configurator/` is its own Cargo workspace and lockfile, with path
  dependencies on firmware crates; a protocol change lands on both sides in one
  commit. Bench motor tests are requests the firmware may refuse. Writes are
  disarmed-only with full readback. Validate board identity, manifest, hashes,
  address ranges, page CRCs, and resume state before claiming success; `.fwbb`
  files are never modified. MSPv2 is not enabled or expanded without approval.
- `tools/rtic-app-builder/` is an isolated workspace that never joins the root
  one without approval. `generated/` is disposable: fix the manifest, contract,
  template, or `xtask`, then regenerate. The generator translates declared
  values and never allocates pins, peripherals, interrupts, DMA, timers,
  priorities, or queues. Its NUCLEO examples have no flight authority.

**Public Book (`mdbook/`).** Never copy agent instructions, internal work
tracking, test metadata, raw evidence, decision provenance, or archives into it.
Change `src/SUMMARY.md` with any chapter added, moved, or removed. Use relative
links that work in source and build; never edit `book/`. Keep generated Mermaid
assets and their licence headers. Never turn a past test result into a general
support or safety claim; label board-specific limits.

**Internal Book (`project_meta/`).** Register every Markdown file in
`DOCUMENT_REGISTRY.json`, and list every non-historical one in `SUMMARY.md`.
`mdbook build project_meta` builds it into `target/internal-book`. Requirements
(`requirements/`) and procedures (`testing/targets/`) stay separate from dated
run evidence (`testing/evidence/`); status is computed by `reqtool`, never
written. Moving evidence preserves its dates,
hashes, feature sets, artifact identities, and limitations.

## Always-Loaded Context Budget

`CLAUDE.md`, `AGENTS.md`, and every file `CLAUDE.md` imports load into each
session before work starts, and share one line budget. **At 300 lines
combined**, stop and ask the user before adding anything: offer a path-scoped
rule under `.claude/rules/`, a skill, an on-demand chapter, or trimming what no
longer earns its place. **At 400 lines**, nothing more is added to any of them
until the user resolves it.

## Document Size and Content

A chapter crossing roughly 400 lines is a signal to apply the retiring rule
below, not a cap to design around: splitting it to get under the number is not
compliance. `governing-requirements.md` states its own limit. Never write into
either book:

- progress narration or status records ("this is now done", "as of ..."); what
  changed and why belongs in the commit message, and dated test results in run
  records under `project_meta/testing/evidence/`;
- restatements of what the code does, instead of intent, constraints, and
  rationale;
- a new document when an existing chapter covers the topic.

## Retiring Stale Material

Fix a stale passage in place. A whole spent or superseded document or section:

1. Find what still binds; a decision never superseded, only buried, is in force.
2. Promote it into the owning chapter first, grouped by topic.
3. Then archive the remainder verbatim under `archive/<date>/`, mirroring its
   repository-relative path, with a header naming the source file, the date,
   and that binding decisions were promoted out.

Never archive before promoting, never rewrite spent material to look current,
and never delete it instead of archiving. Verify before and after; a clean
`mdbook build` proves nothing about content survival:

- **Before**, find what links into the sections you move or remove:
  `rg -o '<file>\.md#[a-z0-9-]+'`, and repoint every hit in the same change.
  Renaming a heading breaks its anchor.
- **After**, `diff <(sort <backup>) <(sort <file>) | grep '^<'` lists only lines
  you deliberately changed.

## Restricted Archive - Explicit User Permission Required

The archives are the top-level `archive/` and
`tools/rtic-app-builder/docs/archive/`. Without explicit user permission, do not
list, search, open, read, summarize, or copy from anything under them, nor
recover their content through other tools or cached copies. Archived documents
are never active authority. Reviewing, documenting, building, or implementing
does not grant access: explain which material you need and why, then wait, and
use only the scope granted. Do not weaken this rule without an explicit user
instruction.

A user-requested archival operation authorizes checking destination paths and
moving named active files into a new snapshot, not reading existing archived
content. `tools/check_repository_context.py` hashing the immutable archives is
tooling, not an agent reading them. `.ignore` excludes the archives from normal
ripgrep searches; with `-uu`, exclude them by hand:

```text
rg --files -uu -g '!archive/**' -g '!tools/rtic-app-builder/docs/archive/**' -g '!**/target/**'
```
