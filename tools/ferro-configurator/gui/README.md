# FerroConfigurator GUI (prototype)

A desktop front-end over the same library the CLI uses. It is a second
front-end, not a wrapper: nothing here shells out to `ferro-configurator`, and
nothing re-parses its output.

```
gui/src/            TypeScript interface, builds and type-checks on its own
gui/src-tauri/      the desktop shell, needs a system webview to build
crates/ferro-configurator-bridge/   the session layer both of them sit on
```

## What it does

Connect to a controller, read and apply tuning configuration, and catalogue and
download flight logs — the read and configure half of the CLI.

It is laid out like the Betaflight Configurator: connection across the top,
tabs down the left (Setup, Ports, Receiver, Motors, PID Tuning, Rates,
Blackbox, Firmware), and the safety banner above every tab. The settings tabs
edit one configuration, so each has the same Save bar and saving sends all of
it; a tab with unsaved edits is marked. Leaving the Motors tab stops every
motor and turns the motor test off. The Setup tab calibrates the accelerometer's level. PID Tuning has a slider beside every gain
and a master slider that scales them all, springing back when released. A
pulled cable, or a controller that stops answering, ends the connection with
"Connection lost" in the safety banner.

The Firmware tab flashes the bundled image or a chosen ELF. A connected,
disarmed Foxeer is restarted into its ROM bootloader, flashed, and reconnected
when it returns; a board already in its bootloader from the BOOT button is
flashed without connecting. Reboot, in the header, restarts the controller
and reconnects. The session in `ferro-configurator-bridge` makes every check;
the page only reflects it.

It also explains why the controller will not arm, as a pre-arm checklist built
by `ferro-configurator-core::prearm` from the status line; draws the rates
curve as the form is edited; and shows live receiver channels with a
find-a-control helper. The channels need a proposed `ch=` status field that no
firmware emits yet, so on hardware that panel says so; the mock exercises it.

## Authority

It has none over the actuator or arming path, and neither does the library
beneath it. The protocol has no disarm command, so **"forces disarm" means the
session refuses to write while the controller reports itself armed**, not that
the host can disarm anything.

That gate lives in `ferro-configurator-bridge`, not in this interface. The
disabled Apply button is a courtesy; the refusal is enforced in Rust and cannot
be bypassed by a front-end bug or by invoking the commands directly.

The status behind the gate is read **fresh for every write**. The client caches
the last snapshot it saw, which is right for a display and wrong for a
precondition, so the session calls `read_status_fresh` instead.

## Running it

From anywhere in the repository, `tools/configurator.py` runs these in the
right directory and installs the npm packages first if they are missing. The
desktop app:

```
python3 tools/configurator.py
```

The browser preview, against the simulated controller:

```
python3 tools/configurator.py preview
```

Or by hand, from this directory:

Without hardware — the interface runs against a scripted controller, including
a button that flips it to armed so the refusal path can be exercised:

```
npm install
npm run dev
```

`#rates` (any tab's name) in the address opens that tab, and `?connect`
connects the simulated controller at once, for previews.

As a desktop application, which needs the webview libraries:

```
sudo apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev \
  build-essential curl file libssl-dev libayatana-appindicator3-dev librsvg2-dev
npm install
npx tauri dev
```

## State

| Piece | State |
| --- | --- |
| `ferro-configurator-bridge` | Built and tested, 6 tests against a mock transport |
| `gui/src` TypeScript | Type-checks under `strict`, builds with Vite |
| `gui/src-tauri` | Compiles, clippy-clean, and launches |

What remains unverified is the part no amount of building can settle: none of
the commands have run against a controller. The shell is deliberately thin so
that the untested surface is small - every command locks the session, calls one
bridge method on the blocking pool, and returns.

## Not here yet

Firmware flashing, DFU, ULog conversion, stored profiles, and `doctor`. The
library supports all of them; the interface does not expose them. Flashing in
particular deserves its own review before it gets a button.
