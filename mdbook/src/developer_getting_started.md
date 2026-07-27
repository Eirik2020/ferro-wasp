# Developer Getting Started

This guide is for contributors building or changing FerroWasp from source.
Users who only want to flash and configure a Foxeer F405 V2 should use the
[Foxeer USB Quick Start](../../docs/FOXEER_F405_V2_QUICK_START.md) instead.

FerroWasp is a safety-oriented experimental flight-control prototype. A
successful build is not flight evidence. Do not connect actuator power or
install propellers as part of ordinary software setup.

## Prerequisites

Required for hardware-free repository development:

- Git;
- [rustup](https://rustup.rs/);
- Python 3 for repository checks and host-side analysis tools.

The repository's `rust-toolchain.toml` pins the Rust nightly, rustfmt, Clippy,
and active Cortex-M target. Let rustup install that exact environment rather
than selecting an unrelated toolchain manually.

Install additional tools only for the work that needs them:

- `probe-rs` for SWD programming and RTT sessions;
- STM32CubeProgrammer for the board-local Foxeer DFU development script;
- mdBook `0.5.2` and mdbook-mermaid `0.17.0` for documentation;
- PlotJuggler or another ULog reader for flight-log inspection.

The ready FerroConfigurator package carries its own reviewed `dfu-util`; users
of that package do not need STM32CubeProgrammer.

## Clone and verify the workspace

```powershell
git clone https://github.com/Eirik2020/ferro-wasp.git
Set-Location ferro-wasp
rustup show active-toolchain
```

Run the hardware-free root checks before making changes:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked
cargo check --workspace --locked
cargo test --workspace --locked
python tools\check_repository_context.py
```

The root workspace contains reusable crates. Deployable firmware applications
are isolated because their STM32 peripheral-access configurations are not
compatible in one Cargo dependency graph:

```text
apps/stm32f405-flight  FerroWasp FCU3 flight app and behavioral reference
apps/foxeer-f405-v2    Foxeer F405 V2 flight app
apps/stm32f401-bringup NUCLEO-F401RE non-actuator bring-up app
```

Run embedded commands from the selected application directory. Do not use one
board's image, pins, DMA routes, sensor orientation, or motor mapping on
another board.

## Build the flight applications

FCU3 is the golden flight app for established runtime and safety behavior:

```powershell
Set-Location apps\stm32f405-flight
cargo check --release --locked
Set-Location ..\..
```

Build the normal Foxeer image with onboard configuration and blackbox support:

```powershell
Set-Location apps\foxeer-f405-v2
cargo build --release --locked --features flash_blackbox
Set-Location ..\..
```

The resulting Foxeer ELF is:

```text
apps/foxeer-f405-v2/target/thumbv7em-none-eabihf/release/FerroWaspFoxeerF405V2
```

Prefer `cargo check` when target programming is not part of the task. Do not
assume a debugger, MCU, receiver, ESC power, or safe motor bench is available.
Hardware execution and powered tests are separate, user-controlled gates.

Before implementing or bench-testing a Foxeer feature, compare the affected
behavior and enabled features with the current FCU3 app. Board-specific
hardware differences remain explicit exceptions, not copied assumptions.

## SWD, RTT, and developer DFU

With the correct board selected and a debugger attached, repository tooling can
build, program, and retain an RTT transcript:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 `
  --release `
  --locked `
  --features flash_blackbox `
  --probe-speed-khz 1800 `
  --connect-under-reset
```

Use only the selected board's documented command and test procedure. Keep
propellers removed, record the exact features and ELF hash, and do not infer a
successful target test from compilation.

The Foxeer application also provides `flash-dfu.ps1` for developer ROM-DFU
work. The normal user workflow is the manifest-verified configurator package,
not a locally selected ELF.

## Build FerroConfigurator

FerroConfigurator is an isolated host workspace:

```powershell
Set-Location tools\ferro-configurator
cargo fmt --all --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --locked -p ferro-configurator-cli
```

Build the exact Foxeer `flash_blackbox` image before assembling a local Windows
package, then run:

```powershell
Set-Location tools\ferro-configurator
.\packaging\build-release.ps1 -Version 0.1.0-dev
```

Generated packages are written to the Git-ignored
[`tools/ferro-configurator/dist`](../../tools/ferro-configurator/dist/)
directory. It does not exist in a fresh checkout until packaging succeeds.

```powershell
$Repo = git rev-parse --show-toplevel
$Dist = Join-Path $Repo "tools\ferro-configurator\dist"
Get-ChildItem $Dist -Filter "ferrowasp-v*-windows-x86_64.zip"
explorer $Dist
```

Validate the package from its extracted folder before using it:

```powershell
.\ferro-configurator.exe flash --board foxeer-f405-v2 --dry-run
```

The dry run must verify the manifest, firmware hash, board, flash range, and
vector table without accessing the MCU. See the
[FerroConfigurator README](../../tools/ferro-configurator/README.md) for its
full development and packaging contract.

## Build the documentation

Install the pinned documentation tools when needed:

```powershell
cargo install mdbook --version 0.5.2 --locked
cargo install mdbook-mermaid --version 0.17.0 --locked
mdbook build mdbook
```

Documentation and context changes must also pass:

```powershell
python tools\check_repository_context.py
python -m unittest tools.tests.test_repository_context -v
```

## Contribution workflow

The default branch is `main`. Keep patches small, preserve unrelated
worktree changes, and use a non-target branch plus pull request for normal
publication. Repository rules require passing checks and protect `main` from
deletion and force-pushes.

Read [CONTRIBUTING.md](../../CONTRIBUTING.md) before submitting work. For
safety-relevant changes, record the reason, exact image/configuration,
verification performed, remaining target gaps, and any timing or unsafe-code
implications. Never claim certification, airworthiness, or production safety
from prototype evidence.

