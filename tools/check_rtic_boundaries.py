#!/usr/bin/env python3
"""Enforce the mechanical parts of FerroWasp's thin-RTIC-app boundary."""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
import tomllib
from collections import defaultdict
from pathlib import Path


USE = re.compile(r"^[ \t]*use[ \t]+([^;]+);", re.MULTILINE)
FORBIDDEN_MAIN_DECLARATION = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?"
    r"(type|const|static|enum|trait|union)[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
MAIN_STRUCT = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?struct[ \t]+"
    r"([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
MAIN_FUNCTION = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:async[ \t]+)?fn[ \t]+"
    r"([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
MACRO_DECLARATION = re.compile(r"^[ \t]*macro_rules!", re.MULTILINE)
BOARD_DECLARATION = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?"
    r"(?:struct|enum|type)[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
DIRECT_TIMER_REGISTER_ACCESS = re.compile(
    r"\.(?:bdtr|ccer|ccmr1_output|ccmr2_output|psc|arr|egr)\(\)"
)
DIRECT_USB_CONSTRUCTION = re.compile(
    r"(?:USB|UsbBusType|UsbDeviceBuilder)::new\s*\("
)

SHARED_BOARD_DECLARATIONS = {
    "AdcStorageResources",
    "BoardIdentity",
    "DmaDirection",
    "DmaRoute",
    "EscIdleQualificationConfig",
    "ImuControlAxisProfile",
    "PinAssignment",
    "ResourceClaim",
    "SerialCapabilities",
    "SerialRoute",
    "SerialRouteError",
    "SpiDmaStorageResources",
    "SpiRoute",
    "StaticPwmRoute",
    "TimerGroupDescription",
    "TimerMode",
    "UartRxStorageResources",
}


def _relative(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def _line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def _internal_import(import_path: str) -> bool:
    compact = re.sub(r"\s+", "", import_path)
    return compact == "super::*" or (
        compact.startswith("ferrowasp_app_") and compact.endswith("::internal::*")
    )


def _validate_main(path: Path, root: Path, errors: list[str]) -> None:
    text = path.read_text(encoding="utf-8")
    label = _relative(path, root)

    for match in USE.finditer(text):
        if not _internal_import(match.group(1)):
            errors.append(
                f"{label}:{_line_number(text, match.start())}: "
                "RTIC main may import only the app internal facade or super::*"
            )

    for match in DIRECT_USB_CONSTRUCTION.finditer(text):
        errors.append(
            f"{label}:{_line_number(text, match.start())}: "
            "USB construction belongs in the chip family's backend crate"
        )

    for match in FORBIDDEN_MAIN_DECLARATION.finditer(text):
        errors.append(
            f"{label}:{_line_number(text, match.start())}: "
            f"{match.group(1)} declaration {match.group(2)!r} belongs outside RTIC main"
        )

    for match in MACRO_DECLARATION.finditer(text):
        errors.append(
            f"{label}:{_line_number(text, match.start())}: "
            "macro declaration belongs outside RTIC main"
        )

    for match in MAIN_STRUCT.finditer(text):
        if match.group(1) not in {"Shared", "Local"}:
            errors.append(
                f"{label}:{_line_number(text, match.start())}: "
                f"struct {match.group(1)!r} is not an RTIC resource struct"
            )

    for match in MAIN_FUNCTION.finditer(text):
        # A body starts with `{`; a FerroForge task instance has none and ends
        # with `;`. Either ends the signature, whichever comes first.
        ends = [
            end
            for end in (text.find("{", match.end()), text.find(";", match.end()))
            if end >= 0
        ]
        signature = text[match.start() : min(ends)] if ends else ""
        name = match.group(1)
        # A FerroForge group member, `fn name;`, has no signature of its own:
        # the group supplies it.
        if "(" not in signature:
            continue
        if f"{name}::Context" not in signature:
            errors.append(
                f"{label}:{_line_number(text, match.start())}: "
                f"function {name!r} does not use its matching RTIC context"
            )


def _board_sources(root: Path) -> list[tuple[str, Path]]:
    """Each app's board support: its HAL glue in `src/board/` and its data
    crate's sources in `board/src/`."""

    sources: list[tuple[str, Path]] = []
    board_dirs = [*root.glob("firmware/*/src/board"), *root.glob("firmware/*/board/src")]
    for board_dir in sorted(board_dirs):
        board = board_dir.parents[1].name
        for path in sorted(board_dir.rglob("*.rs")):
            sources.append((board, path))
    return sources


def _validate_board_sources(root: Path, errors: list[str]) -> None:
    normalized_files: dict[str, list[tuple[str, Path]]] = defaultdict(list)

    for board, path in _board_sources(root):
        text = path.read_text(encoding="utf-8")
        label = _relative(path, root)

        for match in BOARD_DECLARATION.finditer(text):
            name = match.group(1)
            if name in SHARED_BOARD_DECLARATIONS:
                errors.append(
                    f"{label}:{_line_number(text, match.start())}: "
                    f"shared declaration {name!r} must live in a common crate"
                )

        for match in DIRECT_TIMER_REGISTER_ACCESS.finditer(text):
            errors.append(
                f"{label}:{_line_number(text, match.start())}: "
                "direct STM32 timer register sequencing belongs in ferrowasp-stm32f4"
            )

        normalized = re.sub(r"\s+", "", text)
        if len(normalized) >= 200:
            digest = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
            normalized_files[digest].append((board, path))

    for copies in normalized_files.values():
        boards = {board for board, _path in copies}
        if len(boards) < 2:
            continue
        labels = ", ".join(_relative(path, root) for _board, path in copies)
        errors.append(
            "cross-board source files are identical after whitespace normalization: "
            f"{labels}"
        )


def _validate_foxeer_required_usb(root: Path, errors: list[str]) -> None:
    app = root / "firmware" / "foxeer-f405-v2"
    manifest = app / "Cargo.toml"
    main = app / "src" / "main.rs"
    facade = app / "src" / "lib.rs"
    if not manifest.exists() or not main.exists():
        return

    manifest_text = manifest.read_text(encoding="utf-8")
    feature = re.search(r"^[ \t]*usb_serial[ \t]*=", manifest_text, re.MULTILINE)
    if feature:
        errors.append(
            f"{_relative(manifest, root)}:{_line_number(manifest_text, feature.start())}: "
            "Foxeer USB is mandatory and must not be a Cargo feature"
        )

    usb_feature_gate = re.compile(
        r"cfg!?\s*\(\s*feature\s*=\s*\"usb_serial\""
    )
    for path in (main, facade):
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8")
        for match in usb_feature_gate.finditer(text):
            errors.append(
                f"{_relative(path, root)}:{_line_number(text, match.start())}: "
                "Foxeer USB is mandatory and must not be feature-gated"
            )

    main_text = main.read_text(encoding="utf-8")
    if "stm32_usb::init_usb_cdc_serial(" not in main_text:
        errors.append(
            f"{_relative(main, root)}: Foxeer must initialize mandatory USB "
            "through ferrowasp-stm32f4"
        )



FLIGHT_APP_CONTRACTS = {
    "foxeer-f405-v2": {
        "board_feature": "board-foxeer-f405-v2",
        "mandatory_features": (
            "dshot",
            "pwm_cal",
            "usb_serial",
            "esc_telemetry",
            "flash_storage",
            "flash_writes",
            "flash_blackbox",
        ),
        "required_calls": (
            "init_dshot_motor_bank(",
            "init_usb_cdc_serial(",
            "init_uart_ports(",
            "init_spi2_flash(",
        ),
    },
    # The Foxeer's feature set on the STM32H743: UART ports bound at boot
    # (by default SBUS on USART1, MSP on USART3, ESC telemetry on UART8), and
    # blackbox storage on the microSD card.
    "tbs-lucid-h7": {
        "board_feature": "board-tbs-lucid-h7",
        "dshot_feature": "ferrowasp-stm32-app/dshot",
        "mandatory_features": (
            "dshot",
            "pwm_cal",
            "usb_serial",
            "esc_telemetry",
            "flash_storage",
            "flash_writes",
            "flash_blackbox",
        ),
        "required_calls": (
            "init_dshot_motor_bank(",
            "init_usb_cdc_serial(",
            "init_uart_ports(",
            "init_sd_flash(",
        ),
    },
}


def _validate_flight_app_contracts(root: Path, errors: list[str]) -> None:
    for app_name, contract in FLIGHT_APP_CONTRACTS.items():
        app = root / "firmware" / app_name
        manifest = app / "Cargo.toml"
        main = app / "src" / "main.rs"
        facade = app / "src" / "lib.rs"
        if not manifest.exists() or not main.exists():
            continue

        manifest_text = manifest.read_text(encoding="utf-8")
        for feature_name in contract["mandatory_features"]:
            declaration = re.search(
                rf"^[ \t]*{re.escape(feature_name)}[ \t]*=",
                manifest_text,
                re.MULTILINE,
            )
            if declaration:
                errors.append(
                    f"{_relative(manifest, root)}:{_line_number(manifest_text, declaration.start())}: "
                    f"{feature_name} is board-standard and must not be an app feature"
                )

        board_feature = re.search(
            rf"^[ \t]*{re.escape(contract['board_feature'])}[ \t]*=[ \t]*\[(.*?)\]",
            manifest_text,
            re.MULTILINE | re.DOTALL,
        )
        dshot_feature = contract.get("dshot_feature", "ferrowasp-stm32f4/dshot")
        if board_feature is None or dshot_feature not in board_feature.group(1):
            errors.append(
                f"{_relative(manifest, root)}: the board feature must enable "
                f"{dshot_feature}"
            )

        for path in (main, facade):
            if not path.exists():
                continue
            text = path.read_text(encoding="utf-8")
            for feature_name in contract["mandatory_features"]:
                gate = re.compile(rf'feature\s*=\s*"{re.escape(feature_name)}"')
                for match in gate.finditer(text):
                    errors.append(
                        f"{_relative(path, root)}:{_line_number(text, match.start())}: "
                        f"{feature_name} is board-standard and must not be feature-gated"
                    )

        main_text = main.read_text(encoding="utf-8")
        for required_call in contract["required_calls"]:
            if required_call not in main_text:
                errors.append(
                    f"{_relative(main, root)}: missing mandatory board initialization "
                    f"through {required_call}"
                )
        if re.search(r"(?:stm32_static_pwm::)?init_esc_pwm\s*\(", main_text):
            errors.append(
                f"{_relative(main, root)}: flight ESC output must use DShot; "
                "RC PWM remains shared servo/auxiliary infrastructure"
            )

        routes = app / "src" / "board" / "routes.rs"
        if routes.exists():
            routes_text = routes.read_text(encoding="utf-8")
            for stale_name in (
                "ACTIVE_STATIC_PWM_ROUTES",
                "DEFERRED_MOTOR_DMA_ROUTES",
                "OPTIONAL_ESC_TELEMETRY_DMA_ROUTE",
            ):
                if stale_name in routes_text:
                    errors.append(
                        f"{_relative(routes, root)}: stale flight-output route {stale_name}"
                    )


STATIC_DEFINITION = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?static[ \t]+(?:mut[ \t]+)?([A-Z][A-Z0-9_]*)[ \t]*:",
    re.MULTILINE,
)
SHARED_CRATES = Path("crates")


def _validate_shared_snapshots(root: Path, errors: list[str]) -> None:
    """A static that shared crates define exists once.

    Task definitions and the state they share live in the shared crates. An
    app defining its own copy of one of their statics would still compile -
    its tasks would write one static while a shared definition read the
    other - so the only defence is refusing the copy here.
    """
    shared: dict[str, str] = {}
    for source in sorted((root / SHARED_CRATES).glob("*/src/**/*.rs")):
        crate = source.relative_to(root / SHARED_CRATES).parts[0]
        for name in STATIC_DEFINITION.findall(source.read_text(encoding="utf-8")):
            shared.setdefault(name, crate)
    if not shared:
        return
    for source in sorted(root.glob("firmware/*/src/**/*.rs")):
        text = source.read_text(encoding="utf-8")
        for match in STATIC_DEFINITION.finditer(text):
            if match.group(1) in shared:
                errors.append(
                    f"{_relative(source, root)}:{_line_number(text, match.start())}: "
                    f"static {match.group(1)!r} belongs to {shared[match.group(1)]}; "
                    "re-export it instead of defining a second copy"
                )


CRATE_MANIFEST_GLOBS = ("crates/*/Cargo.toml", "firmware/*/Cargo.toml", "firmware/*/board/Cargo.toml")
DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def _read_manifest(path: Path, root: Path, errors: list[str]) -> dict | None:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        errors.append(f"{_relative(path, root)}: {error}")
        return None


def _is_critical(manifest: dict) -> object:
    package = manifest.get("package", {})
    return package.get("metadata", {}).get("ferrowasp", {}).get("critical", False)


def _dependency_tables(manifest: dict) -> list[tuple[str, dict]]:
    tables = [(name, manifest.get(name, {})) for name in DEPENDENCY_TABLES]
    for target, body in manifest.get("target", {}).items():
        tables.extend((f"target.{target}.{name}", body.get(name, {})) for name in DEPENDENCY_TABLES)
    return tables


def _validate_critical_crates(root: Path, errors: list[str]) -> None:
    """ADR-0054's direction rule: a safety-critical crate never depends on a
    non-critical one. Criticality is `[package.metadata.ferrowasp] critical =
    true`; a `-critical` name must agree with it. A safety-critical crate's
    external dependencies are pinned to one version (`=x.y.z`, or a git
    `rev`). Development and build dependencies count: they enter the crate's
    unit evidence."""
    workspace_manifest = _read_manifest(root / "Cargo.toml", root, errors) if (root / "Cargo.toml").is_file() else {}
    workspace_dependencies = (workspace_manifest or {}).get("workspace", {}).get("dependencies", {})

    manifests: dict[Path, dict] = {}
    for pattern in CRATE_MANIFEST_GLOBS:
        for path in sorted(root.glob(pattern)):
            manifest = _read_manifest(path, root, errors)
            if manifest is not None and "package" in manifest:
                manifests[path.resolve()] = manifest

    for path, manifest in manifests.items():
        package = manifest["package"]["name"]
        location = _relative(path, root)
        critical = _is_critical(manifest)
        if not isinstance(critical, bool):
            errors.append(f"{location}: [package.metadata.ferrowasp] critical must be true or false")
            continue
        if package.endswith("-critical") and not critical:
            errors.append(
                f"{location}: {package} is named -critical but is not marked "
                "[package.metadata.ferrowasp] critical = true"
            )
        if not critical:
            continue
        for table, dependencies in _dependency_tables(manifest):
            for name, spec in dependencies.items():
                if isinstance(spec, dict) and spec.get("workspace") is True:
                    spec = workspace_dependencies.get(name, {})
                    base = root
                else:
                    base = path.parent
                if isinstance(spec, str):
                    spec = {"version": spec}
                if "path" in spec:
                    target_path = (base / spec["path"] / "Cargo.toml").resolve()
                    target = manifests.get(target_path) or (
                        _read_manifest(target_path, root, errors) if target_path.is_file() else None
                    )
                    if target is None:
                        errors.append(f"{location}: [{table}] {name}: no manifest at {spec['path']}")
                    elif _is_critical(target) is not True:
                        errors.append(
                            f"{location}: {package} is safety-critical and must not depend on "
                            f"{target['package']['name']}, which is not ([{table}])"
                        )
                elif "git" in spec:
                    if "rev" not in spec:
                        errors.append(
                            f"{location}: [{table}] {name}: a safety-critical crate pins a git "
                            "dependency with rev"
                        )
                elif not str(spec.get("version", "")).startswith("="):
                    errors.append(
                        f"{location}: [{table}] {name}: a safety-critical crate pins external "
                        f"dependencies to one version (=x.y.z), not {spec.get('version')!r}"
                    )


def validate_rtic_boundaries(root: Path) -> list[str]:
    root = root.resolve()
    errors: list[str] = []

    for main in sorted(root.glob("firmware/*/src/main.rs")):
        _validate_main(main, root, errors)
    _validate_board_sources(root, errors)
    _validate_foxeer_required_usb(root, errors)
    _validate_flight_app_contracts(root, errors)
    _validate_shared_snapshots(root, errors)
    _validate_critical_crates(root, errors)
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()

    errors = validate_rtic_boundaries(args.root)
    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    print("RTIC and board-support boundaries are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
