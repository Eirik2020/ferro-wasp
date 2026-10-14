"""Start FerroConfigurator from anywhere in the repository.

    python tools/configurator.py               the desktop app
    python tools/configurator.py preview       the browser preview, simulated controller
    python tools/configurator.py cli ARGS...   the command-line tool

The GUI's npm project lives in tools/ferro-configurator/gui, so `npm run tauri
dev` from the repository root finds no package.json. This runs each command in
the right directory, and installs the GUI's npm packages first when they are
missing.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Sequence

REPOSITORY = Path(__file__).resolve().parent.parent
CONFIGURATOR = REPOSITORY / "tools" / "ferro-configurator"
GUI = CONFIGURATOR / "gui"
PREVIEW_URL = "http://localhost:5173/?connect"


def plan(mode: str, cli_args: Sequence[str], *, gui_installed: bool) -> list[tuple[Path, list[str]]]:
    """The commands to run, each with its working directory."""
    if mode == "cli":
        return [
            (
                CONFIGURATOR,
                ["cargo", "run", "--release", "--quiet", "-p", "ferro-configurator-cli", "--", *cli_args],
            )
        ]
    steps: list[tuple[Path, list[str]]] = []
    if not gui_installed:
        steps.append((GUI, ["npm", "install"]))
    script = ["npm", "run", "tauri", "dev"] if mode == "app" else ["npm", "run", "dev"]
    steps.append((GUI, script))
    return steps


def resolve(command: list[str]) -> list[str]:
    # On Windows npm is npm.cmd, which subprocess finds only by its full name.
    found = shutil.which(command[0])
    if found is None:
        raise SystemExit(f"{command[0]} is not installed or not on PATH")
    return [found, *command[1:]]


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("mode", nargs="?", default="app", choices=["app", "preview", "cli"])
    parser.add_argument("cli_args", nargs=argparse.REMAINDER, help="arguments for the CLI")
    args = parser.parse_args(argv)
    if args.mode != "cli" and args.cli_args:
        parser.error(f"`{args.mode}` takes no further arguments")

    if args.mode == "preview":
        print(f"Open {PREVIEW_URL} once Vite is ready; Ctrl+C stops it.", flush=True)
    for cwd, command in plan(args.mode, args.cli_args, gui_installed=(GUI / "node_modules").is_dir()):
        try:
            code = subprocess.call(resolve(command), cwd=cwd)
        except KeyboardInterrupt:
            return 130
        if code != 0:
            return code
    return 0


if __name__ == "__main__":
    sys.exit(main())
