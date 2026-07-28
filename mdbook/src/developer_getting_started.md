# Developer Getting Started

This guide is for contributors building or changing FerroWasp from source.
Users who only want to flash and configure a Foxeer F405 V2 should use the
[Foxeer USB Quick Start](user/foxeer_f405_v2.md) instead.

A successful build is not target or flight evidence. Ordinary software setup
does not require actuator power or installed propellers.

## Preferred environment: WSL 2 and Docker

The reference developer environment is an x86-64 Linux container running
through Docker Desktop's WSL 2 backend. It pins the repository's three Rust
toolchains, Cortex-M target, Python runtime and packages, mdBook, and
mdbook-mermaid. Cargo lockfiles continue to pin project dependencies.

The container is for source builds, tests, documentation, log conversion, and
headless plots. It intentionally has no default MCU, USB, serial-port, SWD, or
motor access. Use the ready Windows FerroConfigurator package for the normal
Foxeer USB workflow, and keep debugger-assisted hardware work as an explicit
host-side procedure.

### Install the host prerequisites

On Windows 11, open an administrator PowerShell and install WSL if it is not
already present:

```powershell
wsl --install -d Ubuntu-24.04
```

After any requested reboot, install Docker Desktop, select its WSL 2 engine,
and enable integration for the Ubuntu distribution.

Enter the installed distribution from PowerShell:

```powershell
wsl -d Ubuntu-24.04
```

The repository uses an SSH GitHub remote. In the WSL shell, make sure the SSH
client is installed, start an agent, and load a key registered with GitHub:

```bash
sudo apt-get update
sudo apt-get install --yes openssh-client
eval "$(ssh-agent -s)"
ssh-add ~/.ssh/id_ed25519
ssh-add -l
ssh -T git@github.com
```

Use the actual path of the registered key when it is not `id_ed25519`. On the
first connection, compare the displayed host-key fingerprint with the
[GitHub SSH documentation](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/testing-your-ssh-connection)
before accepting it. GitHub reports successful authentication but exits with
status 1 because it does not provide shell access. Keep this WSL shell open so
its `SSH_AUTH_SOCK` remains available to Docker Compose, or launch VS Code from
this shell with `code .`. Never copy a private key into the image or
repository.

Clone the repository into the WSL Linux filesystem, not under `/mnt/c`.
Linux-native storage avoids slow Cargo metadata and build operations.

Only after the prompt changes to a Linux shell, run:

```bash
pwd
mkdir -p ~/src
cd ~/src
git clone git@github.com:Eirik2020/ferro-wasp.git
cd ferro-wasp
pwd
```

After `wsl -d Ubuntu-24.04`, the prompt must be a Linux shell rather than
`PS C:\...`, and the first `pwd` should report a path under `/home/<user>`.
The final `pwd` should resemble `/home/<user>/src/ferro-wasp`. Do not continue
if either path begins with `/mnt/c`, or if PowerShell reports a `C:\...`
location: PowerShell's `mkdir` alias would create another Windows-hosted
checkout.

If the repository already exists on Windows, make a fresh WSL clone rather
than copying Windows `target` directories or virtual environments.

### Build and verify the environment

From the repository root inside WSL:

```bash
docker compose build dev
docker compose run --rm dev bash tools/dev/check-environment.sh
```

The environment check is hardware-free. It verifies exact tool versions, the
pinned Codex CLI, Git and OpenSSH clients, namespace diagnostic tools, Python
analysis dependencies, the embedded target, and discovery of every isolated
Cargo workspace without downloading project dependencies. Authenticated Git access and
nested-userns execution are checked after the recreated container starts.

Most WSL distributions use user and group ID `1000`, which is the container
default. If `id -u` or `id -g` reports another value, build with matching
values so generated source-tree files remain owned by the WSL user:

```bash
DEV_UID="$(id -u)" DEV_GID="$(id -g)" docker compose build dev
```

### Preserve Codex sessions

Codex stores local session rollouts, its session index, configuration, and
login state under `/home/ferrowasp/.codex`. The reference environment mounts
that complete directory from the external Docker volume
`ferrowasp-codex-home`, so container recreation does not discard chats or
settings. The CLI package itself is installed outside that mount under
`/opt/codex`, so mounting retained state cannot hide the executable supplied
by the image.

If an existing FerroWasp container was created before this volume was added,
migrate it exactly once before recreating the container:

```bash
bash tools/dev/migrate-codex-state.sh
```

The migration pauses the running container while taking a bounded snapshot,
refuses to overwrite a non-empty target volume, verifies the copied file
count, and removes its temporary host copy. The volume includes authentication
material; never copy it into the repository or a shared archive. Run migration
as the final action before recreation because later messages written to the
old container are not part of the snapshot.

After recreation, the Codex UI can reopen its retained threads. The installed
CLI also supports:

```bash
codex resume
codex resume --last
codex resume --all
```

### Daily use

Start the persistent development service with the host SSH agent mounted,
then open a shell:

```bash
bash tools/dev/compose-with-ssh-agent.sh up -d dev
bash tools/dev/compose-with-ssh-agent.sh exec dev bash
```

The development service currently disables the Docker default seccomp profile
because that profile blocks the unprivileged `unshare --user` operation
required by the Codex/Bubblewrap sandbox. It also enables
`no-new-privileges` and grants no additional Linux capabilities. This policy remains under
review for replacement with a narrower custom seccomp profile.

The helper refuses to create the service when the host agent socket is absent
or has no loaded identity. Inside the container, verify the forwarded agent
and the configured remote before relying on pull or push:

```bash
bash tools/dev/check-git-ssh.sh --require-agent
ssh -T git@github.com
git ls-remote --exit-code origin HEAD
```

As on the host, the GitHub SSH test reports success with exit status 1. The
`git ls-remote` command must exit successfully.

The repository is bind-mounted at `/workspace/ferro-wasp`. Cargo registry,
Git dependency, and Linux build-target caches live in named Docker volumes.
This prevents incompatible Windows and Linux target artifacts from mixing.

Inside the container, run the normal repository commands directly:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --locked
cargo test --workspace --locked
python -m unittest discover -s tools/tests -v
python tools/check_repository_context.py
mdbook build mdbook
```

Stop the service without deleting caches:

```bash
docker compose down
```

`docker compose down --volumes` deletes the Cargo cache volumes but not the
external `ferrowasp-codex-home` volume. Docker manages that volume outside the
Compose application lifecycle so ordinary teardown and recreation preserve
Codex sessions. Removing the external volume is a separate destructive action
that also removes its retained chats, configuration, and login state.

VS Code users may install the Dev Containers extension, verify `ssh-add -l` in
the host shell, and open the repository with **Dev Containers: Reopen in
Container**. The extension automatically forwards a running host SSH agent,
and the checked-in post-start check rejects a missing client, socket, or loaded
identity. The checked-in `.devcontainer/devcontainer.json` uses the same
Compose service and image as the command-line workflow. See the
[VS Code credential-sharing documentation](https://code.visualstudio.com/remote/advancedcontainers/sharing-git-credentials)
for the upstream behavior.

### What remains outside the container

- Windows FerroConfigurator ready-package execution and ROM-DFU flashing;
- Windows release-package assembly through the PowerShell packaging script;
- probe-rs SWD/RTT sessions and other physical target work;
- interactive Tk live views and PlotJuggler desktop use.

Flight logs stored in the repository's ignored `logs/` directory can still be
converted and analyzed inside the container. USB forwarding through
`usbipd-win` and Docker device mappings may be added later as a separate,
explicit hardware profile; it is not part of the reference software
environment or a prerequisite for development.

## Native fallback prerequisites

Required for hardware-free repository development:

- Git;
- [rustup](https://rustup.rs/);
- Python 3 for repository checks and host-side analysis tools.

The repository toolchain files pin the root nightly, the FerroConfigurator
stable release, the RTIC builder nightly, their components, and active
Cortex-M targets. Let rustup install those exact environments rather than
selecting unrelated toolchains manually.

Install additional tools only for the work that needs them:

- `probe-rs` for SWD programming and RTT sessions;
- STM32CubeProgrammer for the board-local Foxeer DFU development script;
- mdBook `0.5.2` and mdbook-mermaid `0.17.0` for documentation;
- PlotJuggler or another ULog reader for flight-log inspection.

The ready FerroConfigurator package carries its own reviewed `dfu-util`; users
of that package do not need STM32CubeProgrammer.

## Native clone and verification

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
apps/foxeer-f405-v2    Foxeer F405 V2 golden flight app and behavioral reference
apps/stm32f405-flight  FerroWasp FCU3 secondary flight app
apps/stm32f401-bringup NUCLEO-F401RE non-actuator bring-up app
```

Run embedded commands from the selected application directory. Do not use one
board's image, pins, DMA routes, sensor orientation, or motor mapping on
another board.

## Build the flight applications

Foxeer is the golden flight app for established runtime and safety behavior:

```powershell
Set-Location apps\foxeer-f405-v2
cargo build --release --locked
Set-Location ..\..
```

Check the FCU3 secondary app independently:

```powershell
Set-Location apps\stm32f405-flight
cargo check --release --locked
Set-Location ..\..
```

The resulting Foxeer ELF is:

```text
apps/foxeer-f405-v2/target/thumbv7em-none-eabihf/release/FerroWaspFoxeerF405V2
```

Prefer `cargo check` when target programming is not part of the task. Do not
assume a debugger, MCU, receiver, ESC power, or safe motor bench is available.
Hardware execution and powered tests are separate, user-controlled gates.

Before implementing or bench-testing a secondary-board feature, compare the
affected behavior and enabled features with the current Foxeer app.
Board-specific hardware differences remain explicit, not copied assumptions.

## SWD, RTT, and developer DFU

With the correct board selected and a debugger attached, repository tooling can
build, program, and retain an RTT transcript:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 `
  --release `
  --locked `
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

The Rust workspace can be checked in the Linux container. Windows package
assembly remains a Windows-host or CI operation. Build the exact Foxeer
standard Foxeer image before assembling a local Windows package, then run
this from PowerShell:

```powershell
Set-Location tools\ferro-configurator
.\packaging\build-release.ps1 -Version 0.1.0-dev
```

Generated packages are written to the Git-ignored
`tools/ferro-configurator/dist`
directory. It does not exist in a fresh checkout until packaging succeeds.

```powershell
$Repo = git rev-parse --show-toplevel
$Dist = Join-Path $Repo "tools\ferro-configurator\dist"
Get-ChildItem $Dist -Filter "ferrowasp-v*-windows-x86_64.zip"
explorer $Dist
```

Package assembly also writes a sibling `.zip.sha256` file. Publish and retain
the ZIP and checksum together.

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

Read the [contribution guide](contributing.md) before submitting work. For
safety-relevant changes, record the reason, exact image/configuration,
verification performed, remaining target gaps, and any timing or unsafe-code
implications. Never claim certification, airworthiness, or production safety
from prototype evidence.
