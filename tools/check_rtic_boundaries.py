#!/usr/bin/env python3
"""Enforce the mechanical parts of FerroWasp's thin-RTIC-app boundary."""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
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
            "USB construction belongs in ferrowasp-stm32f4"
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
        signature_end = text.find("{", match.end())
        signature = text[match.start() : signature_end] if signature_end >= 0 else ""
        name = match.group(1)
        if f"{name}::Context" not in signature:
            errors.append(
                f"{label}:{_line_number(text, match.start())}: "
                f"function {name!r} does not use its matching RTIC context"
            )


def _board_sources(root: Path) -> list[tuple[str, Path]]:
    sources: list[tuple[str, Path]] = []
    for board_dir in sorted(root.glob("apps/*/src/board")):
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
    app = root / "apps" / "foxeer-f405-v2"
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
    "stm32f405-flight": {
        "board_feature": "board-ferrowasp-fcu3",
        "mandatory_features": ("dshot", "pwm_cal"),
        "required_calls": ("init_dshot_motor_bank(",),
    },
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
            "init_usart1_esc_telemetry(",
            "init_spi2_flash(",
        ),
    },
}


def _validate_flight_app_contracts(root: Path, errors: list[str]) -> None:
    for app_name, contract in FLIGHT_APP_CONTRACTS.items():
        app = root / "apps" / app_name
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
        if board_feature is None or "ferrowasp-stm32f4/dshot" not in board_feature.group(1):
            errors.append(
                f"{_relative(manifest, root)}: the board feature must enable "
                "ferrowasp-stm32f4/dshot"
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


def validate_rtic_boundaries(root: Path) -> list[str]:
    root = root.resolve()
    errors: list[str] = []

    for main in sorted(root.glob("apps/*/src/main.rs")):
        _validate_main(main, root, errors)
    _validate_board_sources(root, errors)
    _validate_foxeer_required_usb(root, errors)
    _validate_flight_app_contracts(root, errors)
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
