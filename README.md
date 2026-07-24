# RTIC App Builder prototype

This directory contains the working NUCLEO-F401RE prototype and the migration
path toward the RTIC App Builder architecture. Its canonical location is
`tools/rtic-app-builder` in the FerroWasp monorepo. It remains an intentionally
isolated nested Cargo workspace: FerroWasp's root workspace does not include it,
so builder dependency and toolchain changes cannot alter firmware builds
implicitly. Run builder commands from this directory.

Current commands and behavior are documented here. The canonical forward roadmap is
[`RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN.md`](RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN.md);
the [older feature-assembler plan](docs/archive/rtic_feature_assembler_mvp_implementation_plan.md)
is retained only as historical context.

This repository builds a deterministic RTIC application for the
NUCLEO-F401RE from separate strict BSP and application manifests plus a
handwritten feature bundle. The BSP owns physical hardware facts and provides
stable resource IDs. The application selects those resources and owns blink
and button behavior. The current example blinks LD2 and uses the B1 EXTI
interrupt to enable or disable blinking after a 20 ms debounce check. The
builder translates declared values; it does not allocate pins,
peripherals, interrupts, or priorities. Ordinary software timing is derived
from one backend-owned 1 kHz SysTick monotonic, so blink, debounce, and OSD
refresh do not reserve TIM2/TIM3/TIM4. Feature-specific electrical policy,
such as the blink LED's push-pull mode, pull, and speed, is fixed and
documented by the backend rather than repeated in either manifest.

An additional, isolated `nucleo-f401re-osd` application exercises USART1 on
PA9/PA10 with separate RX/TX DMA streams and an MSP DisplayPort consumer. It
does not modify or depend directly on any FerroWasp flight application. Its
temporary compatibility crates are marked for replacement by the canonical
FerroWasp crates once a stable in-tree interface exists.
Its debounced B1 feature toggles only the displayed demonstration ARM state;
it cannot arm motors or enter FerroWasp's flight arming path.

The BSP selects a Betaflight-style MCU compatibility profile with the compact
identifier `STM32F401`. The backend derives the Rust target, HAL/PAC selection,
and linker memory layout from that profile; those implementation details are
not duplicated in the BSP manifest.

## Prerequisites

The pinned toolchain and embedded target are declared in
`rust-toolchain.toml`. Rustup installs them automatically when network access
is available.

## Generate

```text
cargo xtask generate --app nucleo-f401re-blinky
cargo xtask generate --app nucleo-f401re-osd
```

The application manifest identifies its BSP. The explicit `--manifest` plus
`--bsp` form remains supported for relative paths, absolute paths, or direct
catalog selection.

The command validates all inputs before touching generated output, checks an
empty RTIC application, inserts each feature in manifest order, runs an
embedded `cargo check --locked` after every insertion, and finishes with a
linked release build.

The last checked checkpoint is stored under:

```text
generated/nucleo-f401re-blinky/working
```

Failed candidates and their command diagnostics are retained below
`generated/nucleo-f401re-blinky/failed`. Generated source is disposable; edit
the BSP, application manifest, feature fragments, templates, or backend
instead.

Resume is intentionally strict:

```text
cargo xtask generate `
  --app nucleo-f401re-blinky `
  --resume
```

It is accepted only when every fingerprinted input is unchanged and the
working checkpoint still passes its checks. After correcting an input, run a
normal `generate` so the application is rebuilt from scratch.

To remove one generated application:

```text
cargo xtask clean --app nucleo-f401re-blinky
```

To regenerate, build, flash through a connected probe, verify, and reset:

```text
cargo xtask flash --app nucleo-f401re-blinky
```

For an explicitly named release build without touching hardware, or an
attached `cargo embed` session:

```text
cargo xtask build --app nucleo-f401re-blinky
cargo xtask embed --app nucleo-f401re-blinky
```

The hardware commands require the probe-rs tools on `PATH`. They derive the
probe-rs chip name and Rust target from the BSP's MCU profile. See
`docs/cheatsheet.md` for the common development commands.

## Verification

```text
cargo check -p xtask --locked
cargo test -p xtask --locked
```

The generator itself performs the embedded check and release-link gates. The
manual development-board procedure is documented in
`docs/hardware-smoke-test.md`. The OSD architecture, reference provenance,
and safe replacement boundary are documented in `docs/osd-usart1-dma.md`.

## Design notes

The reference implementation plan owns sequencing, milestones, schemas, and
migration policy. Focused notes remain authoritative only for the narrower
topics they document:

- `docs/architecture-observations.md` records evidence and lessons from the
  current prototypes;
- `docs/betaflight-target-definition-notes.md` records the board-capability and
  boot-frozen platform-configuration model;
- `docs/stm32f4-backend.md` records the current narrow backend contract;
- `docs/osd-usart1-dma.md` records the current OSD prototype and provenance.

Current FerroWasp board status, golden applications, and target evidence remain
owned by the monorepo root. Inspect those sources and the applicable
`../../project_docs` guidance at a pinned commit when a task depends on them;
do not duplicate their status in builder documentation.

The agreed vocabulary and lessons from the complete UART RX/TX DMA plus OSD
prototype are maintained in `docs/architecture-observations.md`.

The component, endpoint, and capability authoring handbook starts at
`docs/authoring/README.md`. It records the common ownership and testing
checklist and separates current builder behavior from the future typed
capability metadata.

For ChatGPT Project use, `docs/chatgpt-project-context.md` is the self-contained
architectural handoff. Upload that file when ChatGPT needs project goals,
invariants, current implementation status, and known transitional boundaries
without access to the complete repository.
