#!/usr/bin/env python3
"""Validate FerroWasp release metadata before a tag can publish artifacts."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_CONFIG = REPO_ROOT / "tools/release/release.toml"
SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")


def load_toml(path: Path) -> dict:
    with path.open("rb") as source:
        return tomllib.load(source)


def manifest_versions() -> dict[str, str]:
    versions: dict[str, str] = {}
    manifests = sorted((REPO_ROOT / "crates").glob("*/Cargo.toml"))
    manifests.extend(sorted((REPO_ROOT / "apps").glob("*/Cargo.toml")))

    for manifest in manifests:
        package = load_toml(manifest).get("package")
        if not isinstance(package, dict) or not isinstance(package.get("version"), str):
            raise ValueError(f"{manifest.relative_to(REPO_ROOT)} has no package.version")
        versions[manifest.relative_to(REPO_ROOT).as_posix()] = package["version"]

    configurator = REPO_ROOT / "tools/ferro-configurator/Cargo.toml"
    workspace_package = load_toml(configurator).get("workspace", {}).get("package")
    if not isinstance(workspace_package, dict) or not isinstance(
        workspace_package.get("version"), str
    ):
        raise ValueError(
            "tools/ferro-configurator/Cargo.toml has no workspace.package.version"
        )
    versions[configurator.relative_to(REPO_ROOT).as_posix()] = workspace_package[
        "version"
    ]
    return versions


def git_is_clean() -> bool:
    result = subprocess.run(
        ["git", "status", "--porcelain=v1", "--untracked-files=all"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return not result.stdout.strip()


def validate(requested_tag: str | None, require_clean: bool) -> list[str]:
    errors: list[str] = []
    config = load_toml(RELEASE_CONFIG)

    version = config.get("version")
    tag = config.get("tag")
    notes = config.get("release_notes")
    if not isinstance(version, str) or not SEMVER.fullmatch(version):
        errors.append("release.toml version must be a plain MAJOR.MINOR.PATCH value")
        return errors
    if tag != f"v{version}":
        errors.append(f"release.toml tag must be v{version}, got {tag!r}")
    if config.get("github_prerelease") is not True:
        errors.append("v0.x publication must remain a GitHub pre-release")
    if requested_tag is not None and requested_tag != tag:
        errors.append(f"workflow tag {requested_tag!r} does not match {tag!r}")

    try:
        versions = manifest_versions()
    except (OSError, tomllib.TOMLDecodeError, ValueError) as error:
        errors.append(str(error))
    else:
        for manifest, manifest_version in versions.items():
            if manifest_version != version:
                errors.append(
                    f"{manifest} version {manifest_version!r} does not match {version!r}"
                )

    if not isinstance(notes, str):
        errors.append("release.toml release_notes must be a repository-relative path")
    else:
        notes_path = REPO_ROOT / notes
        if not notes_path.is_file():
            errors.append(f"release notes are missing: {notes}")
        elif f"# FerroWasp {tag} " not in notes_path.read_text(encoding="utf-8"):
            errors.append(f"{notes} does not identify FerroWasp {tag}")

    changelog = (REPO_ROOT / "mdbook/src/changelog.md").read_text(encoding="utf-8")
    if f"## {version} - " not in changelog:
        errors.append(f"mdBook changelog has no dated {version} release section")

    if require_clean and not git_is_clean():
        errors.append("release checkout is not clean")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--tag",
        help="tag supplied by the release workflow; must match release.toml",
    )
    parser.add_argument(
        "--require-clean",
        action="store_true",
        help="fail if tracked or untracked worktree changes are present",
    )
    args = parser.parse_args()

    errors = validate(args.tag, args.require_clean)
    if errors:
        for error in errors:
            print(f"release check: {error}", file=sys.stderr)
        return 1

    config = load_toml(RELEASE_CONFIG)
    print(
        "Release metadata ready: "
        f"{config['tag']} (GitHub pre-release, notes: {config['release_notes']})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
