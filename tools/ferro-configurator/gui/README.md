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

Without hardware — the interface runs against a scripted controller, including
a button that flips it to armed so the refusal path can be exercised:

```
npm install
npm run dev
```

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
| `gui/src-tauri` | **Written but never compiled** — no webview available where it was written |

The shell is the only unverified part. It is deliberately thin so that what is
unverified is small: every command locks the session, calls one bridge method
on the blocking pool, and returns.

## Not here yet

Firmware flashing, DFU, ULog conversion, stored profiles, and `doctor`. The
library supports all of them; the interface does not expose them. Flashing in
particular deserves its own review before it gets a button.
