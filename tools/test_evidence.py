#!/usr/bin/env python3
"""Validate bounded FerroWasp test-run evidence records."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
from datetime import datetime
from pathlib import Path, PurePosixPath
from typing import Any


REGISTRY_PATH = PurePosixPath("project_docs/DOCUMENT_REGISTRY.json")
MAX_ALLOWED_RECORD_BYTES = 8192
MAX_STRING_CHARS = 2048
MAX_LIST_ITEMS = 32
MAX_OBJECT_FIELDS = 64
SHA256_HEX = re.compile(r"^[0-9A-Fa-f]{64}$")
TEST_ID = re.compile(r"^[A-Z][A-Z0-9]*(?:-[A-Z0-9]+){2,}$")
UTC_TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
RUN_ID = re.compile(
    r"^(?P<stamp>\d{8}T\d{6}Z)__"
    r"(?P<test>[A-Z][A-Z0-9]*(?:-[A-Z0-9]+){2,})__"
    r"(?P<target>common|fcu3|foxeer-f405-v2)__"
    r"(?P<sequence>\d{2})$"
)
ARTIFACT_ID = re.compile(r"^[a-z][a-z0-9-]{0,63}$")

ALLOWED_POLICY_FIELDS = {
    "artifact_roots",
    "max_record_bytes",
    "records_root",
    "template",
}
REQUIRED_RECORD_FIELDS = {
    "artifacts",
    "candidate",
    "commands",
    "completed_at_utc",
    "conditions",
    "definition",
    "execution",
    "limitations",
    "measurements",
    "observations",
    "result",
    "run_id",
    "schema_version",
    "started_at_utc",
    "stop_conditions_triggered",
    "target",
    "test_id",
    "tier",
}
REQUIRED_DEFINITION_FIELDS = {
    "catalog_sha256",
    "procedure_heading",
    "procedure_path",
    "procedure_sha256",
}
REQUIRED_EXECUTION_FIELDS = {"performed_by", "user_confirmed"}
REQUIRED_CANDIDATE_FIELDS = {
    "configuration",
    "features",
    "firmware_sha256",
    "revision",
    "target_triple",
    "working_tree",
}
REQUIRED_CONDITION_FIELDS = {
    "actuator_power",
    "environment",
    "equipment",
    "propellers",
}
REQUIRED_COMMAND_FIELDS = {"command", "exit_code"}
REQUIRED_ARTIFACT_FIELDS = {"bytes", "id", "kind", "path", "sha256"}

ALLOWED_TARGETS = {"common", "fcu3", "foxeer-f405-v2"}
ALLOWED_TIERS = {
    "software",
    "embedded-build",
    "bench-unpowered",
    "bench-powered-props-off",
    "preflight",
    "flight",
}
HARDWARE_TIERS = {
    "bench-unpowered",
    "bench-powered-props-off",
    "preflight",
    "flight",
}
ALLOWED_RESULTS = {"pass", "fail", "aborted", "inconclusive"}
ALLOWED_PERFORMERS = {"agent", "user", "user-with-agent-assistance"}
ALLOWED_WORKING_TREE_STATES = {"clean", "dirty"}
ALLOWED_PROPELLER_STATES = {
    "installed",
    "not-applicable",
    "removed",
    "unknown",
}
ALLOWED_ACTUATOR_POWER_STATES = {
    "connected",
    "disconnected",
    "not-applicable",
    "unknown",
}
ALLOWED_ARTIFACT_KINDS = {
    "analysis",
    "command-output",
    "configuration",
    "firmware",
    "image",
    "log",
    "other",
    "report",
    "trace",
}
FORBIDDEN_INLINE_KEYS = {
    "base64",
    "binary",
    "blob",
    "content",
    "data",
    "log_text",
    "output",
    "raw",
    "samples",
    "series",
}


def _repository_path(root: Path, raw_path: str) -> tuple[Path | None, str | None]:
    if not raw_path or "\\" in raw_path:
        return None, f"path must be a non-empty forward-slash path: {raw_path!r}"

    relative = PurePosixPath(raw_path)
    if relative.is_absolute() or ".." in relative.parts:
        return None, f"path must stay within the repository: {raw_path!r}"

    return root.joinpath(*relative.parts), None


def _positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def _nonnegative_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _load_json(path: Path, label: str, errors: list[str]) -> Any:
    try:
        return json.loads(
            path.read_text(encoding="utf-8"),
            parse_constant=lambda value: (_ for _ in ()).throw(
                ValueError(f"non-finite number {value}")
            ),
        )
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        errors.append(f"{label}: cannot read JSON: {error}")
        return None


def _check_fields(
    value: Any,
    required: set[str],
    label: str,
    errors: list[str],
) -> dict[str, Any] | None:
    if not isinstance(value, dict):
        errors.append(f"{label} must be an object")
        return None

    missing = sorted(required - value.keys())
    extra = sorted(value.keys() - required)
    if missing:
        errors.append(f"{label} missing fields: {', '.join(missing)}")
    if extra:
        errors.append(f"{label} has unsupported fields: {', '.join(extra)}")
    return value


def _string(
    value: Any,
    label: str,
    errors: list[str],
    *,
    allow_empty: bool = False,
    max_chars: int = MAX_STRING_CHARS,
) -> str | None:
    if not isinstance(value, str):
        errors.append(f"{label} must be a string")
        return None
    if not allow_empty and not value.strip():
        errors.append(f"{label} must not be empty")
        return None
    if len(value) > max_chars:
        errors.append(f"{label} exceeds {max_chars} characters")
    return value


def _enum(
    value: Any,
    allowed: set[str],
    label: str,
    errors: list[str],
) -> str | None:
    text = _string(value, label, errors)
    if text is not None and text not in allowed:
        errors.append(f"{label} has unsupported value {text!r}")
    return text


def _sha256(value: Any, label: str, errors: list[str]) -> str | None:
    text = _string(value, label, errors, max_chars=64)
    if text is not None and not SHA256_HEX.fullmatch(text):
        errors.append(f"{label} must be 64 hexadecimal characters")
    return text


def _string_list(
    value: Any,
    label: str,
    errors: list[str],
    *,
    allow_empty: bool = True,
) -> list[str]:
    if not isinstance(value, list):
        errors.append(f"{label} must be a list")
        return []
    if not allow_empty and not value:
        errors.append(f"{label} must not be empty")
    if len(value) > MAX_LIST_ITEMS:
        errors.append(f"{label} exceeds {MAX_LIST_ITEMS} items")

    strings: list[str] = []
    for index, item in enumerate(value):
        text = _string(item, f"{label}[{index}]", errors)
        if text is not None:
            strings.append(text)
    return strings


def _bounded_json(value: Any, label: str, errors: list[str]) -> None:
    if isinstance(value, dict):
        if len(value) > MAX_OBJECT_FIELDS:
            errors.append(f"{label} exceeds {MAX_OBJECT_FIELDS} fields")
        for key, item in value.items():
            if not isinstance(key, str):
                errors.append(f"{label} contains a non-string field name")
                continue
            if key.lower() in FORBIDDEN_INLINE_KEYS:
                errors.append(f"{label}.{key} may not embed raw artifact content")
            if len(key) > 128:
                errors.append(f"{label} field name exceeds 128 characters")
            _bounded_json(item, f"{label}.{key}", errors)
    elif isinstance(value, list):
        if len(value) > MAX_LIST_ITEMS:
            errors.append(f"{label} exceeds {MAX_LIST_ITEMS} items")
        for index, item in enumerate(value):
            _bounded_json(item, f"{label}[{index}]", errors)
    elif isinstance(value, str):
        if len(value) > MAX_STRING_CHARS:
            errors.append(f"{label} exceeds {MAX_STRING_CHARS} characters")
    elif isinstance(value, float) and not math.isfinite(value):
        errors.append(f"{label} must be finite")
    elif value is not None and not isinstance(value, (bool, int, float)):
        errors.append(f"{label} contains an unsupported JSON value")


def _timestamp(value: Any, label: str, errors: list[str]) -> datetime | None:
    text = _string(value, label, errors, max_chars=20)
    if text is None:
        return None
    if not UTC_TIMESTAMP.fullmatch(text):
        errors.append(f"{label} must use YYYY-MM-DDTHH:MM:SSZ UTC format")
        return None
    try:
        return datetime.strptime(text, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError as error:
        errors.append(f"{label} is not a valid UTC timestamp: {error}")
        return None


def _safe_record_path(
    value: Any,
    label: str,
    errors: list[str],
) -> PurePosixPath | None:
    text = _string(value, label, errors, max_chars=512)
    if text is None:
        return None
    if "\\" in text:
        errors.append(f"{label} must use forward slashes")
        return None
    path = PurePosixPath(text)
    if path.is_absolute() or ".." in path.parts:
        errors.append(f"{label} must stay within the repository")
        return None
    return path


def _validate_template(path: Path, errors: list[str]) -> None:
    template = _load_json(path, path.as_posix(), errors)
    if template is None:
        return
    record = _check_fields(
        template,
        REQUIRED_RECORD_FIELDS,
        f"{path.as_posix()} template",
        errors,
    )
    if record is not None and record.get("schema_version") != 1:
        errors.append(f"{path.as_posix()} template schema_version must be 1")


def _load_catalog(
    root: Path,
    raw_path: str,
    errors: list[str],
) -> dict[str, dict[str, Any]]:
    path, path_error = _repository_path(root, raw_path)
    if path_error:
        errors.append(f"test evidence catalog: {path_error}")
        return {}
    assert path is not None
    data = _load_json(path, raw_path, errors)
    if not isinstance(data, dict) or not isinstance(data.get("tests"), list):
        errors.append(f"{raw_path}: test evidence requires a catalog test list")
        return {}

    definitions: dict[str, dict[str, Any]] = {}
    for entry in data["tests"]:
        if isinstance(entry, dict) and isinstance(entry.get("id"), str):
            definitions[entry["id"]] = entry
    return definitions


def _validate_definition(
    value: Any,
    label: str,
    errors: list[str],
) -> None:
    definition = _check_fields(value, REQUIRED_DEFINITION_FIELDS, label, errors)
    if definition is None:
        return
    _sha256(definition.get("catalog_sha256"), f"{label}.catalog_sha256", errors)
    _sha256(definition.get("procedure_sha256"), f"{label}.procedure_sha256", errors)
    _safe_record_path(
        definition.get("procedure_path"),
        f"{label}.procedure_path",
        errors,
    )
    _string(
        definition.get("procedure_heading"),
        f"{label}.procedure_heading",
        errors,
        max_chars=256,
    )


def _validate_execution(
    value: Any,
    label: str,
    errors: list[str],
) -> tuple[str | None, bool | None]:
    execution = _check_fields(value, REQUIRED_EXECUTION_FIELDS, label, errors)
    if execution is None:
        return None, None
    performer = _enum(
        execution.get("performed_by"),
        ALLOWED_PERFORMERS,
        f"{label}.performed_by",
        errors,
    )
    user_confirmed = execution.get("user_confirmed")
    if not isinstance(user_confirmed, bool):
        errors.append(f"{label}.user_confirmed must be boolean")
        user_confirmed = None
    return performer, user_confirmed


def _validate_candidate(
    value: Any,
    label: str,
    errors: list[str],
) -> str | None:
    candidate = _check_fields(value, REQUIRED_CANDIDATE_FIELDS, label, errors)
    if candidate is None:
        return None

    _string(candidate.get("revision"), f"{label}.revision", errors, max_chars=256)
    _enum(
        candidate.get("working_tree"),
        ALLOWED_WORKING_TREE_STATES,
        f"{label}.working_tree",
        errors,
    )

    firmware_sha256 = candidate.get("firmware_sha256")
    if firmware_sha256 is not None:
        _sha256(firmware_sha256, f"{label}.firmware_sha256", errors)

    features = _string_list(candidate.get("features"), f"{label}.features", errors)
    if len(features) != len(set(features)):
        errors.append(f"{label}.features must not contain duplicates")

    target_triple = candidate.get("target_triple")
    if target_triple is not None:
        _string(target_triple, f"{label}.target_triple", errors, max_chars=128)

    configuration = candidate.get("configuration")
    if not isinstance(configuration, dict):
        errors.append(f"{label}.configuration must be an object")
    return firmware_sha256 if isinstance(firmware_sha256, str) else None


def _validate_conditions(
    value: Any,
    label: str,
    errors: list[str],
) -> tuple[str | None, str | None]:
    conditions = _check_fields(value, REQUIRED_CONDITION_FIELDS, label, errors)
    if conditions is None:
        return None, None
    propellers = _enum(
        conditions.get("propellers"),
        ALLOWED_PROPELLER_STATES,
        f"{label}.propellers",
        errors,
    )
    actuator_power = _enum(
        conditions.get("actuator_power"),
        ALLOWED_ACTUATOR_POWER_STATES,
        f"{label}.actuator_power",
        errors,
    )
    _string_list(conditions.get("equipment"), f"{label}.equipment", errors)
    _string_list(conditions.get("environment"), f"{label}.environment", errors)
    return propellers, actuator_power


def _validate_commands(value: Any, label: str, errors: list[str]) -> None:
    if not isinstance(value, list):
        errors.append(f"{label} must be a list")
        return
    if len(value) > MAX_LIST_ITEMS:
        errors.append(f"{label} exceeds {MAX_LIST_ITEMS} items")
    for index, item in enumerate(value):
        command_label = f"{label}[{index}]"
        command = _check_fields(item, REQUIRED_COMMAND_FIELDS, command_label, errors)
        if command is None:
            continue
        _string(
            command.get("command"),
            f"{command_label}.command",
            errors,
            max_chars=1024,
        )
        exit_code = command.get("exit_code")
        if not isinstance(exit_code, int) or isinstance(exit_code, bool):
            errors.append(f"{command_label}.exit_code must be an integer")


def _validate_artifacts(
    root: Path,
    value: Any,
    artifact_roots: set[str],
    label: str,
    errors: list[str],
) -> None:
    if not isinstance(value, list):
        errors.append(f"{label} must be a list")
        return
    if len(value) > MAX_LIST_ITEMS:
        errors.append(f"{label} exceeds {MAX_LIST_ITEMS} items")

    seen_ids: set[str] = set()
    for index, item in enumerate(value):
        artifact_label = f"{label}[{index}]"
        artifact = _check_fields(
            item,
            REQUIRED_ARTIFACT_FIELDS,
            artifact_label,
            errors,
        )
        if artifact is None:
            continue

        artifact_id = _string(
            artifact.get("id"),
            f"{artifact_label}.id",
            errors,
            max_chars=64,
        )
        if artifact_id is not None:
            if not ARTIFACT_ID.fullmatch(artifact_id):
                errors.append(f"{artifact_label}.id must be a lowercase stable ID")
            if artifact_id in seen_ids:
                errors.append(f"{label} contains duplicate ID {artifact_id!r}")
            seen_ids.add(artifact_id)

        _enum(
            artifact.get("kind"),
            ALLOWED_ARTIFACT_KINDS,
            f"{artifact_label}.kind",
            errors,
        )
        path = _safe_record_path(
            artifact.get("path"),
            f"{artifact_label}.path",
            errors,
        )
        expected_bytes = artifact.get("bytes")
        if not _nonnegative_int(expected_bytes):
            errors.append(f"{artifact_label}.bytes must be a non-negative integer")
        expected_sha256 = _sha256(
            artifact.get("sha256"),
            f"{artifact_label}.sha256",
            errors,
        )

        if path is None:
            continue
        if not path.parts or path.parts[0] not in artifact_roots:
            errors.append(
                f"{artifact_label}.path must use one of: "
                f"{', '.join(sorted(artifact_roots))}"
            )
            continue

        local_path = root.joinpath(*path.parts)
        if not local_path.exists():
            continue
        if not local_path.is_file():
            errors.append(f"{artifact_label}.path must reference a file")
            continue
        if (
            _nonnegative_int(expected_bytes)
            and local_path.stat().st_size != expected_bytes
        ):
            errors.append(
                f"{artifact_label}: artifact size {local_path.stat().st_size} "
                f"does not match {expected_bytes}"
            )
        if expected_sha256 is not None and SHA256_HEX.fullmatch(expected_sha256):
            digest = hashlib.sha256()
            try:
                with local_path.open("rb") as artifact_file:
                    for chunk in iter(lambda: artifact_file.read(1024 * 1024), b""):
                        digest.update(chunk)
            except OSError as error:
                errors.append(f"{artifact_label}: cannot hash artifact: {error}")
            else:
                actual = digest.hexdigest().upper()
                if actual != expected_sha256.upper():
                    errors.append(
                        f"{artifact_label}: artifact SHA-256 {actual} does not "
                        f"match {expected_sha256.upper()}"
                    )


def _validate_record(
    root: Path,
    records_root: Path,
    path: Path,
    max_record_bytes: int,
    artifact_roots: set[str],
    definitions: dict[str, dict[str, Any]],
    seen_run_ids: set[str],
    errors: list[str],
) -> None:
    raw_path = path.relative_to(root).as_posix()
    if path.stat().st_size > max_record_bytes:
        errors.append(
            f"{raw_path}: {path.stat().st_size} bytes exceeds evidence budget "
            f"{max_record_bytes}"
        )

    data = _load_json(path, raw_path, errors)
    if data is None:
        return
    record = _check_fields(data, REQUIRED_RECORD_FIELDS, raw_path, errors)
    if record is None:
        return
    _bounded_json(record, raw_path, errors)

    if record.get("schema_version") != 1:
        errors.append(f"{raw_path}: schema_version must be 1")

    run_id = _string(
        record.get("run_id"),
        f"{raw_path}.run_id",
        errors,
        max_chars=160,
    )
    run_match = RUN_ID.fullmatch(run_id) if run_id is not None else None
    if run_id is not None:
        if run_id != path.stem:
            errors.append(f"{raw_path}: run_id must equal the filename stem")
        if run_id in seen_run_ids:
            errors.append(f"duplicate evidence run_id: {run_id}")
        seen_run_ids.add(run_id)
        if run_match is None:
            errors.append(f"{raw_path}.run_id has invalid format")

    relative = path.relative_to(records_root)
    if len(relative.parts) != 3:
        errors.append(f"{raw_path}: evidence record must use runs/YYYY/MM/")
    elif run_match is not None:
        stamp = run_match.group("stamp")
        if relative.parts[0] != stamp[:4] or relative.parts[1] != stamp[4:6]:
            errors.append(f"{raw_path}: year/month path must match run_id")

    test_id = _string(
        record.get("test_id"),
        f"{raw_path}.test_id",
        errors,
        max_chars=128,
    )
    if test_id is not None and not TEST_ID.fullmatch(test_id):
        errors.append(f"{raw_path}.test_id must be a stable uppercase test ID")
    if run_match is not None and test_id != run_match.group("test"):
        errors.append(f"{raw_path}: test_id must match run_id")

    target = _enum(
        record.get("target"),
        ALLOWED_TARGETS,
        f"{raw_path}.target",
        errors,
    )
    if run_match is not None and target != run_match.group("target"):
        errors.append(f"{raw_path}: target must match run_id")

    tier = _enum(
        record.get("tier"),
        ALLOWED_TIERS,
        f"{raw_path}.tier",
        errors,
    )
    result = _enum(
        record.get("result"),
        ALLOWED_RESULTS,
        f"{raw_path}.result",
        errors,
    )

    definition = definitions.get(test_id) if test_id is not None else None
    if test_id is not None and definition is None:
        errors.append(f"{raw_path}: unknown test_id {test_id}")
    elif definition is not None:
        catalog_target = definition.get("target")
        if catalog_target != "common" and target != catalog_target:
            errors.append(
                f"{raw_path}: target {target!r} does not match catalog "
                f"{catalog_target!r}"
            )
        if tier != definition.get("tier"):
            errors.append(
                f"{raw_path}: tier {tier!r} does not match catalog "
                f"{definition.get('tier')!r}"
            )

    started = _timestamp(
        record.get("started_at_utc"),
        f"{raw_path}.started_at_utc",
        errors,
    )
    completed = _timestamp(
        record.get("completed_at_utc"),
        f"{raw_path}.completed_at_utc",
        errors,
    )
    if started is not None and completed is not None and completed < started:
        errors.append(f"{raw_path}: completed_at_utc precedes started_at_utc")
    if completed is not None and run_match is not None:
        expected_stamp = completed.strftime("%Y%m%dT%H%M%SZ")
        if run_match.group("stamp") != expected_stamp:
            errors.append(f"{raw_path}: run_id timestamp must match completion time")

    _validate_definition(record.get("definition"), f"{raw_path}.definition", errors)
    performer, user_confirmed = _validate_execution(
        record.get("execution"),
        f"{raw_path}.execution",
        errors,
    )
    firmware_sha256 = _validate_candidate(
        record.get("candidate"),
        f"{raw_path}.candidate",
        errors,
    )
    propellers, actuator_power = _validate_conditions(
        record.get("conditions"),
        f"{raw_path}.conditions",
        errors,
    )
    _validate_commands(record.get("commands"), f"{raw_path}.commands", errors)
    _validate_artifacts(
        root,
        record.get("artifacts"),
        artifact_roots,
        f"{raw_path}.artifacts",
        errors,
    )

    measurements = record.get("measurements")
    if not isinstance(measurements, dict):
        errors.append(f"{raw_path}.measurements must be an object")
    observations = _string_list(
        record.get("observations"),
        f"{raw_path}.observations",
        errors,
    )
    limitations = _string_list(
        record.get("limitations"),
        f"{raw_path}.limitations",
        errors,
    )
    stop_conditions = _string_list(
        record.get("stop_conditions_triggered"),
        f"{raw_path}.stop_conditions_triggered",
        errors,
    )
    del observations, limitations

    if result == "pass" and stop_conditions:
        errors.append(f"{raw_path}: pass cannot include triggered stop conditions")

    if tier in HARDWARE_TIERS:
        if performer == "agent":
            errors.append(f"{raw_path}: hardware evidence cannot be agent-only")
        if user_confirmed is not True:
            errors.append(f"{raw_path}: hardware evidence requires user confirmation")
        if firmware_sha256 is None or not SHA256_HEX.fullmatch(firmware_sha256):
            errors.append(f"{raw_path}: hardware evidence requires firmware SHA-256")
        if propellers == "not-applicable" or actuator_power == "not-applicable":
            errors.append(
                f"{raw_path}: hardware evidence requires power and propeller state"
            )
        if result == "pass" and (
            propellers == "unknown" or actuator_power == "unknown"
        ):
            errors.append(
                f"{raw_path}: pass requires known power and propeller state"
            )


def validate_evidence_records(
    root: Path,
    policy: Any,
    catalog_path: str,
) -> list[str]:
    """Return concise evidence-record validation failures."""

    root = root.resolve()
    errors: list[str] = []
    if not isinstance(policy, dict):
        return ["document registry 'test_evidence' must be an object"]

    missing_policy = sorted(ALLOWED_POLICY_FIELDS - policy.keys())
    extra_policy = sorted(policy.keys() - ALLOWED_POLICY_FIELDS)
    if missing_policy:
        errors.append(
            "test_evidence missing fields: " + ", ".join(missing_policy)
        )
    if extra_policy:
        errors.append(
            "test_evidence has unsupported fields: " + ", ".join(extra_policy)
        )

    max_record_bytes = policy.get("max_record_bytes")
    if not _positive_int(max_record_bytes):
        errors.append("test_evidence.max_record_bytes must be a positive integer")
        max_record_bytes = MAX_ALLOWED_RECORD_BYTES
    elif max_record_bytes > MAX_ALLOWED_RECORD_BYTES:
        errors.append(
            f"test_evidence.max_record_bytes must not exceed "
            f"{MAX_ALLOWED_RECORD_BYTES}"
        )
        max_record_bytes = MAX_ALLOWED_RECORD_BYTES

    records_raw = policy.get("records_root")
    template_raw = policy.get("template")
    if not isinstance(records_raw, str):
        errors.append("test_evidence.records_root must be a string")
        records_path = None
    else:
        records_path, path_error = _repository_path(root, records_raw)
        if path_error:
            errors.append(f"test_evidence.records_root: {path_error}")
            records_path = None

    if not isinstance(template_raw, str):
        errors.append("test_evidence.template must be a string")
        template_path = None
    else:
        template_path, path_error = _repository_path(root, template_raw)
        if path_error:
            errors.append(f"test_evidence.template: {path_error}")
            template_path = None

    artifact_roots_raw = policy.get("artifact_roots")
    artifact_roots: set[str] = set()
    if not isinstance(artifact_roots_raw, list) or not artifact_roots_raw:
        errors.append("test_evidence.artifact_roots must be a non-empty list")
    else:
        for index, item in enumerate(artifact_roots_raw):
            if (
                not isinstance(item, str)
                or not item
                or "\\" in item
                or len(PurePosixPath(item).parts) != 1
                or PurePosixPath(item).is_absolute()
            ):
                errors.append(
                    f"test_evidence.artifact_roots[{index}] must be one "
                    "repository-root directory"
                )
            else:
                artifact_roots.add(item)

    if template_path is not None:
        if not template_path.is_file():
            errors.append(f"test evidence template does not exist: {template_raw}")
        else:
            _validate_template(template_path, errors)

    if records_path is None:
        return errors
    if not records_path.is_dir():
        errors.append(f"test evidence records root does not exist: {records_raw}")
        return errors

    definitions = _load_catalog(root, catalog_path, errors)
    seen_run_ids: set[str] = set()
    for path in sorted(records_path.rglob("*")):
        if not path.is_file():
            continue
        if path.name == ".gitkeep":
            continue
        if path.suffix.lower() != ".json":
            errors.append(
                f"{path.relative_to(root).as_posix()}: evidence records must be JSON"
            )
            continue
        _validate_record(
            root,
            records_path,
            path,
            max_record_bytes,
            artifact_roots,
            definitions,
            seen_run_ids,
            errors,
        )

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

    registry_path = args.root.joinpath(*REGISTRY_PATH.parts)
    try:
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"cannot read {REGISTRY_PATH.as_posix()}: {error}", file=sys.stderr)
        return 1

    catalog_path = registry.get("test_catalog")
    if not isinstance(catalog_path, str):
        print("document registry must identify test_catalog", file=sys.stderr)
        return 1

    errors = validate_evidence_records(
        args.root,
        registry.get("test_evidence"),
        catalog_path,
    )
    if errors:
        print("test evidence check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("test evidence check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
