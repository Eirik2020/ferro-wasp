#!/usr/bin/env python3
"""Validate FerroWasp documentation routing and context-size budgets."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any

if __package__:
    from .app_context import validate_routes as validate_app_context_routes
else:
    from app_context import validate_routes as validate_app_context_routes


REGISTRY_PATH = PurePosixPath("project_meta/DOCUMENT_REGISTRY.json")
ALLOWED_LIFECYCLES = {"live", "durable", "historical", "generated"}
ALLOWED_CONTEXTS = {"default", "targeted", "exclude"}
CURRENT_STATE_HEADING = re.compile(r"^## Current State(?:\s|$)", re.MULTILINE)
AGENT_INSTRUCTION_FILE = "AGENTS.md"
CLAUDE_INSTRUCTION_FILE = "CLAUDE.md"
CLAUDE_IMPORT = re.compile(r"^@(\S+)\s*$", re.MULTILINE)
ALWAYS_LOADED_MAX_LINES = 400
INTERNAL_BOOK_SUMMARY = PurePosixPath("project_meta/SUMMARY.md")
SUMMARY_LINK = re.compile(r"\]\(([^)#\s]+\.md)\)")
AGENT_DISCOVERY_IGNORED_DIRS = {
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    ".venv",
    "__pycache__",
    "book",
    "target",
    "venv",
}
SHA256_HEX = re.compile(r"^[0-9A-Fa-f]{64}$")


def _repository_path(root: Path, raw_path: str) -> tuple[Path | None, str | None]:
    if not raw_path or "\\" in raw_path:
        return None, f"path must be a non-empty forward-slash path: {raw_path!r}"

    relative = PurePosixPath(raw_path)
    if relative.is_absolute() or ".." in relative.parts:
        return None, f"path must stay within the repository: {raw_path!r}"

    return root.joinpath(*relative.parts), None


def _positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def _discover_agent_instruction_paths(root: Path) -> set[str]:
    discovered: set[str] = set()

    for directory, child_dirs, filenames in os.walk(
        root,
        topdown=True,
        onerror=lambda _error: None,
    ):
        child_dirs[:] = [
            name for name in child_dirs if name not in AGENT_DISCOVERY_IGNORED_DIRS
        ]
        if AGENT_INSTRUCTION_FILE not in filenames:
            continue

        path = Path(directory) / AGENT_INSTRUCTION_FILE
        discovered.add(path.relative_to(root).as_posix())

    return discovered


def validate_always_loaded(root: Path) -> list[str]:
    """Check that `CLAUDE.md` and its imports fit their shared line budget."""

    claude = root / CLAUDE_INSTRUCTION_FILE
    if not claude.is_file():
        return []

    text = claude.read_text(encoding="utf-8")
    errors: list[str] = []
    total = len(text.splitlines())
    for raw_path in CLAUDE_IMPORT.findall(text):
        path, path_error = _repository_path(root, raw_path)
        if path_error or path is None or not path.is_file():
            errors.append(f"{CLAUDE_INSTRUCTION_FILE} import does not resolve: {raw_path}")
            continue
        total += len(path.read_text(encoding="utf-8").splitlines())

    if total > ALWAYS_LOADED_MAX_LINES:
        errors.append(
            f"always-loaded context is {total} lines, over the "
            f"{ALWAYS_LOADED_MAX_LINES}-line limit"
        )
    return errors


def validate_internal_book(root: Path, registry: dict[str, Any]) -> list[str]:
    """Check that the internal book lists exactly the non-historical documents."""

    summary = root / INTERNAL_BOOK_SUMMARY
    if not summary.is_file():
        return []

    book_root = INTERNAL_BOOK_SUMMARY.parent
    listed = {
        (book_root / link).as_posix()
        for link in SUMMARY_LINK.findall(summary.read_text(encoding="utf-8"))
    }
    expected = {
        entry["path"]
        for entry in registry.get("documents", [])
        if isinstance(entry, dict)
        and isinstance(entry.get("path"), str)
        and entry.get("lifecycle") != "historical"
        and entry["path"] != INTERNAL_BOOK_SUMMARY.as_posix()
    }
    errors = [
        f"{INTERNAL_BOOK_SUMMARY}: missing chapter {path}"
        for path in sorted(expected - listed)
    ]
    errors.extend(
        f"{INTERNAL_BOOK_SUMMARY}: lists a historical or unregistered document {path}"
        for path in sorted(listed - expected)
    )
    return errors


def _string_list(
    value: Any,
    label: str,
    errors: list[str],
    *,
    allow_empty: bool,
) -> list[str]:
    if not isinstance(value, list):
        errors.append(f"{label} must be a list")
        return []
    if not value and not allow_empty:
        errors.append(f"{label} must not be empty")

    strings: list[str] = []
    for index, item in enumerate(value):
        if not isinstance(item, str) or not item.strip():
            errors.append(f"{label}[{index}] must be a non-empty string")
        else:
            strings.append(item)
    return strings


def _load_registry(root: Path, errors: list[str]) -> dict[str, Any]:
    registry_file = root.joinpath(*REGISTRY_PATH.parts)
    try:
        data = json.loads(registry_file.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"cannot read {REGISTRY_PATH.as_posix()}: {error}")
        return {}

    if not isinstance(data, dict):
        errors.append("document registry root must be a JSON object")
        return {}
    if data.get("schema_version") != 1:
        errors.append("document registry schema_version must be 1")
    return data


def _validate_immutable_archives(
    root: Path,
    value: Any,
    errors: list[str],
) -> None:
    if value is None:
        return
    if not isinstance(value, list):
        errors.append("document registry 'immutable_archives' must be a list")
        return

    seen: set[str] = set()
    for index, entry in enumerate(value):
        label = f"immutable_archives[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue

        raw_path = entry.get("path")
        expected_bytes = entry.get("bytes")
        expected_sha256 = entry.get("sha256")
        if not isinstance(raw_path, str):
            errors.append(f"{label}.path must be a string")
            continue
        if raw_path in seen:
            errors.append(f"duplicate immutable archive: {raw_path}")
        seen.add(raw_path)

        path, path_error = _repository_path(root, raw_path)
        if path_error:
            errors.append(f"{label}: {path_error}")
            continue
        assert path is not None

        if not _positive_int(expected_bytes):
            errors.append(f"{raw_path}: bytes must be a positive integer")
        if not isinstance(expected_sha256, str) or not SHA256_HEX.fullmatch(
            expected_sha256
        ):
            errors.append(f"{raw_path}: sha256 must be 64 hexadecimal characters")

        if not path.is_file():
            errors.append(f"immutable archive does not exist: {raw_path}")
            continue

        if _positive_int(expected_bytes) and path.stat().st_size != expected_bytes:
            errors.append(
                f"{raw_path}: immutable size {path.stat().st_size} does not match "
                f"{expected_bytes}"
            )

        if isinstance(expected_sha256, str) and SHA256_HEX.fullmatch(expected_sha256):
            digest = hashlib.sha256()
            try:
                with path.open("rb") as archive:
                    for chunk in iter(lambda: archive.read(1024 * 1024), b""):
                        digest.update(chunk)
            except OSError as error:
                errors.append(f"cannot hash immutable archive {raw_path}: {error}")
            else:
                actual_sha256 = digest.hexdigest().upper()
                if actual_sha256 != expected_sha256.upper():
                    errors.append(
                        f"{raw_path}: immutable SHA-256 {actual_sha256} does not "
                        f"match {expected_sha256.upper()}"
                    )


def validate_repository(root: Path) -> list[str]:
    """Return concise validation failures; an empty list means success."""

    root = root.resolve()
    errors: list[str] = []
    registry = _load_registry(root, errors)
    _validate_immutable_archives(root, registry.get("immutable_archives"), errors)
    documents = registry.get("documents", [])
    budgets = registry.get("context_budgets", [])

    if not isinstance(documents, list):
        errors.append("document registry 'documents' must be a list")
        documents = []
    if not isinstance(budgets, list):
        errors.append("document registry 'context_budgets' must be a list")
        budgets = []

    registered: set[str] = set()
    for index, entry in enumerate(documents):
        label = f"documents[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue

        raw_path = entry.get("path")
        if not isinstance(raw_path, str):
            errors.append(f"{label}.path must be a string")
            continue

        path, path_error = _repository_path(root, raw_path)
        if path_error:
            errors.append(f"{label}: {path_error}")
            continue
        assert path is not None

        if raw_path in registered:
            errors.append(f"duplicate document registration: {raw_path}")
        registered.add(raw_path)

        if not raw_path.startswith("project_meta/") or not raw_path.endswith(".md"):
            errors.append(f"registered document must be project_meta/**/*.md: {raw_path}")

        role = entry.get("role")
        if not isinstance(role, str) or not role.strip():
            errors.append(f"{raw_path}: role must be a non-empty string")

        lifecycle = entry.get("lifecycle")
        if lifecycle not in ALLOWED_LIFECYCLES:
            errors.append(f"{raw_path}: invalid lifecycle {lifecycle!r}")

        context = entry.get("context")
        if context not in ALLOWED_CONTEXTS:
            errors.append(f"{raw_path}: invalid context {context!r}")
        if lifecycle == "historical" and context != "exclude":
            errors.append(f"{raw_path}: historical documents must use context 'exclude'")

        if not path.is_file():
            errors.append(f"registered document does not exist: {raw_path}")
            continue

        max_bytes = entry.get("max_bytes")
        if max_bytes is not None:
            if not _positive_int(max_bytes):
                errors.append(f"{raw_path}: max_bytes must be a positive integer")
            elif path.stat().st_size > max_bytes:
                errors.append(
                    f"{raw_path}: {path.stat().st_size} bytes exceeds budget {max_bytes}"
                )

        max_headings = entry.get("max_current_state_headings")
        if max_headings is not None:
            if not _positive_int(max_headings):
                errors.append(
                    f"{raw_path}: max_current_state_headings must be a positive integer"
                )
            else:
                heading_count = len(
                    CURRENT_STATE_HEADING.findall(path.read_text(encoding="utf-8"))
                )
                if heading_count > max_headings:
                    errors.append(
                        f"{raw_path}: {heading_count} Current State headings exceeds "
                        f"limit {max_headings}"
                    )

    docs_root = root / "project_meta"
    discovered = {
        path.relative_to(root).as_posix()
        for path in docs_root.rglob("*.md")
        if path.is_file()
    }
    for raw_path in sorted(discovered - registered):
        errors.append(f"unregistered project document: {raw_path}")
    for raw_path in sorted(registered - discovered):
        errors.append(f"registry entry is not a discovered project document: {raw_path}")

    errors.extend(validate_internal_book(root, registry))

    budget_paths: set[str] = set()
    for index, entry in enumerate(budgets):
        label = f"context_budgets[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue

        raw_path = entry.get("path")
        max_bytes = entry.get("max_bytes")
        if not isinstance(raw_path, str):
            errors.append(f"{label}.path must be a string")
            continue
        if raw_path in budget_paths:
            errors.append(f"duplicate context budget: {raw_path}")
        budget_paths.add(raw_path)

        path, path_error = _repository_path(root, raw_path)
        if path_error:
            errors.append(f"{label}: {path_error}")
            continue
        assert path is not None

        if not _positive_int(max_bytes):
            errors.append(f"{raw_path}: max_bytes must be a positive integer")
        elif not path.is_file():
            errors.append(f"context-budget file does not exist: {raw_path}")
        elif path.stat().st_size > max_bytes:
            errors.append(
                f"{raw_path}: {path.stat().st_size} bytes exceeds budget {max_bytes}"
            )

    discovered_agent_instructions = _discover_agent_instruction_paths(root)
    budgeted_agent_instructions = {
        raw_path
        for raw_path in budget_paths
        if PurePosixPath(raw_path).name == AGENT_INSTRUCTION_FILE
    }
    for raw_path in sorted(
        discovered_agent_instructions - budgeted_agent_instructions
    ):
        errors.append(f"unbudgeted agent instruction file: {raw_path}")
    errors.extend(validate_always_loaded(root))

    if (root / "tools" / "app_context.py").is_file():
        errors.extend(validate_app_context_routes(root))

    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (defaults to the parent of tools/)",
    )
    args = parser.parse_args(argv)

    errors = validate_repository(args.root)
    if errors:
        print("repository context check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("repository context check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
