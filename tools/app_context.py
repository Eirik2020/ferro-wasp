#!/usr/bin/env python3
"""Route agents to bounded FerroWasp app context without reading whole app shells."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MAX_OUTPUT_BYTES = 8192
FUNCTION_DECLARATION = re.compile(
    r"^ {4}(?:(?:pub(?:\([^)]*\))?|async|const|unsafe)\s+)*fn\s+"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
APP_MODULE = re.compile(r"^mod app\s*\{")
ATTRIBUTE_KIND = re.compile(r"#\[\s*(task|init|idle)\b")
FEATURE_NAME = re.compile(r'feature\s*=\s*"([^"]+)"')
PRIORITY = re.compile(r"\bpriority\s*=\s*(\d+)")
INTERRUPT_BINDING = re.compile(r"\bbinds\s*=\s*([A-Z][A-Z0-9_]*)")
RESOURCE_FIELD = re.compile(r"^ {8}([A-Za-z_][A-Za-z0-9_]*)\s*:")


class RouteError(RuntimeError):
    """A deterministic route could not be built from the current repository."""


@dataclass(frozen=True)
class AppTarget:
    name: str
    source: PurePosixPath
    readme: PurePosixPath
    cargo_toml: PurePosixPath
    instructions: tuple[PurePosixPath, ...]
    test_chain: tuple[str, ...]
    bsp_directory: PurePosixPath


@dataclass(frozen=True)
class TopicRoute:
    summary: str
    common_symbols: tuple[str, ...]
    foxeer_symbols: tuple[str, ...] = ()
    golden_symbols: tuple[str, ...] | None = None
    companions: tuple[PurePosixPath, ...] = ()


@dataclass(frozen=True)
class RustSymbol:
    name: str
    start_line: int
    declaration_line: int
    end_line: int
    kind: str
    priority: int | None
    interrupt_binding: str | None
    features: tuple[str, ...]
    source_bytes: int


@dataclass(frozen=True)
class ResourceBlock:
    name: str
    start_line: int
    end_line: int
    field_count: int


APP_TARGETS = {
    "fcu3": AppTarget(
        name="fcu3",
        source=PurePosixPath("apps/stm32f405-flight/src/main.rs"),
        readme=PurePosixPath("apps/stm32f405-flight/README.md"),
        cargo_toml=PurePosixPath("apps/stm32f405-flight/Cargo.toml"),
        instructions=(PurePosixPath("AGENTS.md"), PurePosixPath("apps/AGENTS.md")),
        test_chain=(
            "SW-COMMON-001",
            "BUILD-FCU3-001",
            "BENCH-COMMON-001",
            "BENCH-FCU3-DSHOT-001",
            "PREFLIGHT-FCU3-001",
            "FLIGHT-FCU3-001",
        ),
        bsp_directory=PurePosixPath(
            "crates/ferrowasp-bsp/src/stm32f4/ferrowasp_fcu3"
        ),
    ),
    "foxeer-f405-v2": AppTarget(
        name="foxeer-f405-v2",
        source=PurePosixPath("apps/foxeer-f405-v2/src/main.rs"),
        readme=PurePosixPath("apps/foxeer-f405-v2/README.md"),
        cargo_toml=PurePosixPath("apps/foxeer-f405-v2/Cargo.toml"),
        instructions=(
            PurePosixPath("AGENTS.md"),
            PurePosixPath("apps/AGENTS.md"),
            PurePosixPath("apps/foxeer-f405-v2/AGENTS.md"),
        ),
        test_chain=(
            "SW-COMMON-001",
            "BUILD-FOX-001",
            "BENCH-COMMON-001",
            "BENCH-FOX-USB-001",
            "BENCH-FOX-001",
            "PREFLIGHT-FOX-001",
            "FLIGHT-FOX-001",
        ),
        bsp_directory=PurePosixPath(
            "crates/ferrowasp-bsp/src/stm32f4/foxeer_f405_v2"
        ),
    ),
}


TOPIC_ROUTES = {
    "overview": TopicRoute(
        summary="RTIC task, resource, priority, interrupt, and feature outline.",
        common_symbols=(),
    ),
    "init": TopicRoute(
        summary="Board construction, resource ownership, startup, and heartbeat.",
        common_symbols=("init", "heartbeat"),
        companions=(PurePosixPath("project_docs/CODEX_PROJECT_CONTEXT.md"),),
    ),
    "arming": TopicRoute(
        summary="Safety events, live guards, guarded holds, and actuator handoff.",
        common_symbols=(
            "warn_arming_abort",
            "safety_master",
            "validate_live_arming_guard",
            "current_arming_guard",
            "wait_arming_hold",
            "actuator_idle_notify",
            "actuator_output",
        ),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-core/src/safety.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/esc_manager.rs"),
        ),
    ),
    "control": TopicRoute(
        summary="Control-loop scheduling, setpoints, state reset, mixing, and publication.",
        common_symbols=(
            "motor_command_timestamp",
            "publish_motor_command",
            "control_loop",
        ),
        foxeer_symbols=("enqueue_flash_record",),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-tasks/src/drone_toolbox.rs"),
            PurePosixPath("crates/ferrowasp-core/src/safety.rs"),
        ),
    ),
    "actuator": TopicRoute(
        summary="Fresh-command validation, actuator ownership, DShot service, and DMA completion.",
        common_symbols=(
            "validate_live_arming_guard",
            "current_arming_guard",
            "wait_arming_hold",
            "actuator_idle_notify",
            "dshot_service",
            "dshot_motor1_dma_complete",
            "dshot_motor2_dma_complete",
            "dshot_motor3_dma_complete",
            "dshot_motor4_dma_complete",
            "take_fresh_motor_outputs",
            "actuator_output",
        ),
        foxeer_symbols=("service_dshot_dma_irq",),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-core/src/safety.rs"),
            PurePosixPath("crates/ferrowasp-stm32f4/src/dshot.rs"),
            PurePosixPath("crates/ferrowasp-waveform/src/dshot.rs"),
        ),
    ),
    "dshot": TopicRoute(
        summary="DShot mapping, qualification fault injection, service, and lane IRQs.",
        common_symbols=(
            "logical_motor_for_esc_output",
            "inject_idle_qualification_fault",
            "dshot_service",
            "dshot_motor1_dma_complete",
            "dshot_motor2_dma_complete",
            "dshot_motor3_dma_complete",
            "dshot_motor4_dma_complete",
            "take_fresh_motor_outputs",
            "actuator_output",
        ),
        foxeer_symbols=(
            "dshot_motor_for_output",
            "logical_motor_for_physical_index",
            "service_dshot_dma_irq",
        ),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-tasks/src/esc_manager.rs"),
            PurePosixPath("crates/ferrowasp-stm32f4/src/dshot.rs"),
            PurePosixPath("crates/ferrowasp-waveform/src/dshot.rs"),
        ),
    ),
    "telemetry": TopicRoute(
        summary="Legacy ESC telemetry UART ownership, association, and manager service.",
        common_symbols=(
            "usart1_rx_dma_transfer",
            "usart1_rx_peripheral",
            "esc_manager_task",
            "dshot_service",
        ),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-drivers/src/blheli_telemetry.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/esc_manager.rs"),
        ),
    ),
    "imu": TopicRoute(
        summary="IMU triggering, SPI ownership, DMA completion, timeout, and parsing.",
        common_symbols=(
            "spi1_poll",
            "pend_spi1_owner",
            "spi1_owner_service",
            "spi1_rx_dma",
            "io_watchdog",
            "spi1_timeout",
            "spi1_parser",
        ),
        foxeer_symbols=("imu_orientation_snapshot", "imu_data_ready"),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-drivers/src/mpu6500.rs"),
            PurePosixPath("crates/ferrowasp-drivers/src/icm42688p.rs"),
            PurePosixPath("crates/ferrowasp-stm32f4/src/spi_dma.rs"),
        ),
    ),
    "rc": TopicRoute(
        summary="SBUS DMA/peripheral ingress, discontinuity handling, link state, and commands.",
        common_symbols=(
            "record_uart2_discontinuity",
            "publish_uart2_owned",
            "record_uart2_dma_error",
            "usart2_rx_dma_transfer",
            "usart2_rx_peripheral",
            "neutralize_rc_input",
            "rc_input",
        ),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-core/src/safety.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/drone_toolbox.rs"),
            PurePosixPath("crates/ferrowasp-stm32f4/src/uart_dma.rs"),
        ),
    ),
    "osd": TopicRoute(
        summary="MSP OSD receive, refresh, transmit queue, and DMA completion.",
        common_symbols=(
            "osd_write",
            "uart4_rx_dma_transfer",
            "uart4_rx_peripheral",
            "osd_refresh",
            "uart4_tx_worker",
            "uart4_tx_dma_transfer",
        ),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-tasks/src/osd.rs"),
            PurePosixPath("crates/ferrowasp-mspv1/src/lib.rs"),
        ),
    ),
    "adc": TopicRoute(
        summary="ADC DMA observation, conversion, battery state, and polling.",
        common_symbols=("dma_adc1", "adc1_polling"),
        companions=(
            PurePosixPath("crates/ferrowasp-stm32f4/src/adc.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/osd.rs"),
        ),
    ),
    "usb": TopicRoute(
        summary="USB task plus Foxeer debug/configurator request handling.",
        common_symbols=("usb_fs",),
        foxeer_symbols=(
            "active_usb_debug_imu_kind",
            "device_uid",
            "fixed_bytes",
            "msp_common_info",
            "rpc_device_info",
            "stage_rpc_response",
            "handle_msp_packet",
        ),
        golden_symbols=("usb_fs", "safety_master", "actuator_output"),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-tasks/src/usb_debug.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/flash_storage.rs"),
        ),
    ),
    "storage": TopicRoute(
        summary="Foxeer flash discovery, configuration, blackbox access, and manager state.",
        common_symbols=(),
        foxeer_symbols=(
            "enqueue_flash_record",
            "scan_flash_log",
            "load_flash_config",
            "queue_storage_response",
            "queue_rpc_response",
            "finish_config_rpc_error",
            "blackbox_flight_id_at",
            "blackbox_bounds",
            "blackbox_info",
            "list_blackboxes",
            "read_blackbox_chunk",
            "queue_page_hex",
            "flash_scratch_test_page",
            "flash_manager_task",
        ),
        golden_symbols=("safety_master", "control_loop", "actuator_output"),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-core/src/blackbox.rs"),
            PurePosixPath("crates/ferrowasp-drivers/src/spi_nor.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/flash_storage.rs"),
        ),
    ),
    "logging": TopicRoute(
        summary="Control-record production and Foxeer persisted-log ownership.",
        common_symbols=("control_loop",),
        foxeer_symbols=("enqueue_flash_record", "flash_manager_task"),
        golden_symbols=("control_loop", "safety_master", "actuator_output"),
        companions=(
            PurePosixPath("project_docs/CODEX_ACTIVE_WORK.md"),
            PurePosixPath("crates/ferrowasp-core/src/blackbox.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/drone_toolbox.rs"),
            PurePosixPath("crates/ferrowasp-tasks/src/flash_storage.rs"),
        ),
    ),
}


def _repository_path(root: Path, path: PurePosixPath) -> Path:
    return root.joinpath(*path.parts)


def _attribute_start(lines: list[str], declaration_index: int, module_index: int) -> int:
    nearest_attribute: int | None = None
    for index in range(declaration_index - 1, module_index, -1):
        line = lines[index].rstrip("\r\n")
        if FUNCTION_DECLARATION.match(line) or line == "    }":
            break
        stripped = line.lstrip()
        if stripped.startswith("#["):
            nearest_attribute = index
    return nearest_attribute if nearest_attribute is not None else declaration_index


def discover_symbols_from_text(text: str) -> tuple[list[RustSymbol], list[ResourceBlock]]:
    """Discover top-level functions and RTIC resource blocks inside `mod app`."""

    lines = text.splitlines(keepends=True)
    module_index = next(
        (index for index, line in enumerate(lines) if APP_MODULE.match(line)),
        None,
    )
    if module_index is None:
        raise RouteError("RTIC app module `mod app {` was not found")

    declarations: list[tuple[int, int, str]] = []
    for index in range(module_index + 1, len(lines)):
        match = FUNCTION_DECLARATION.match(lines[index].rstrip("\r\n"))
        if match is None:
            continue
        start_index = _attribute_start(lines, index, module_index)
        declarations.append((start_index, index, match.group("name")))

    if not declarations:
        raise RouteError("no top-level functions were found inside the RTIC app module")

    symbols: list[RustSymbol] = []
    for position, (start_index, declaration_index, name) in enumerate(declarations):
        next_start = (
            declarations[position + 1][0]
            if position + 1 < len(declarations)
            else len(lines)
        )
        attribute_text = "".join(lines[start_index:declaration_index])
        kind_match = ATTRIBUTE_KIND.search(attribute_text)
        priority_match = PRIORITY.search(attribute_text)
        binding_match = INTERRUPT_BINDING.search(attribute_text)
        symbols.append(
            RustSymbol(
                name=name,
                start_line=start_index + 1,
                declaration_line=declaration_index + 1,
                end_line=next_start,
                kind=kind_match.group(1) if kind_match else "helper",
                priority=int(priority_match.group(1)) if priority_match else None,
                interrupt_binding=binding_match.group(1) if binding_match else None,
                features=tuple(sorted(set(FEATURE_NAME.findall(attribute_text)))),
                source_bytes=len("".join(lines[start_index:next_start]).encode("utf-8")),
            )
        )

    resources: list[ResourceBlock] = []
    for name in ("Shared", "Local"):
        struct_index = next(
            (
                index
                for index in range(module_index + 1, len(lines))
                if lines[index].strip() == f"struct {name} {{"
            ),
            None,
        )
        if struct_index is None:
            raise RouteError(f"RTIC resource block `struct {name}` was not found")

        end_index = next(
            (
                index
                for index in range(struct_index + 1, len(lines))
                if lines[index].rstrip("\r\n") == "    }"
            ),
            None,
        )
        if end_index is None:
            raise RouteError(f"RTIC resource block `struct {name}` was not closed")

        attribute_name = f"#[{name.lower()}]"
        start_index = next(
            (
                index
                for index in range(struct_index - 1, module_index, -1)
                if lines[index].strip() == attribute_name
            ),
            struct_index,
        )
        resources.append(
            ResourceBlock(
                name=name,
                start_line=start_index + 1,
                end_line=end_index + 1,
                field_count=sum(
                    RESOURCE_FIELD.match(line.rstrip("\r\n")) is not None
                    for line in lines[struct_index + 1 : end_index]
                ),
            )
        )

    return symbols, resources


def discover_symbols(root: Path, target: AppTarget) -> tuple[list[RustSymbol], list[ResourceBlock]]:
    source_path = _repository_path(root, target.source)
    try:
        text = source_path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise RouteError(f"cannot read {target.source.as_posix()}: {error}") from error
    return discover_symbols_from_text(text)


def _index_symbols(symbols: Iterable[RustSymbol]) -> dict[str, list[RustSymbol]]:
    indexed: dict[str, list[RustSymbol]] = {}
    for symbol in symbols:
        indexed.setdefault(symbol.name, []).append(symbol)
    return indexed


def _topic_symbol_names(board: str, topic: TopicRoute) -> tuple[str, ...]:
    if board == "foxeer-f405-v2":
        return topic.common_symbols + topic.foxeer_symbols
    return topic.common_symbols


def _topic_supported(board: str, topic_name: str, topic: TopicRoute) -> bool:
    return (
        topic_name == "overview"
        or bool(topic.common_symbols)
        or (board == "foxeer-f405-v2" and bool(topic.foxeer_symbols))
    )


def _select_symbols(
    indexed: dict[str, list[RustSymbol]],
    names: Iterable[str],
    *,
    label: str,
) -> list[RustSymbol]:
    selected: list[RustSymbol] = []
    for name in names:
        matches = indexed.get(name)
        if not matches:
            raise RouteError(f"{label}: routed symbol `{name}` was not found")
        selected.extend(matches)
    return sorted(selected, key=lambda symbol: symbol.start_line)


def _describe_symbol(source: PurePosixPath, symbol: RustSymbol) -> str:
    metadata: list[str] = [symbol.kind]
    if symbol.priority is not None:
        metadata.append(f"priority={symbol.priority}")
    if symbol.interrupt_binding is not None:
        metadata.append(f"binds={symbol.interrupt_binding}")
    if symbol.features:
        metadata.append(f"features={','.join(symbol.features)}")
    details = " ".join(metadata)
    return (
        f"- {symbol.name} [{details}] "
        f"{source.as_posix()}:{symbol.start_line}-{symbol.end_line} "
        f"({symbol.source_bytes} B)"
    )


def _selected_bytes(symbols: Iterable[RustSymbol]) -> int:
    return sum(symbol.source_bytes for symbol in symbols)


def _render_route(root: Path, board: str, topic_name: str) -> str:
    target = APP_TARGETS[board]
    topic = TOPIC_ROUTES[topic_name]
    if not _topic_supported(board, topic_name, topic):
        raise RouteError(f"{board}/{topic_name}: topic is not supported for this app")
    symbols, resources = discover_symbols(root, target)
    indexed = _index_symbols(symbols)

    if topic_name == "overview":
        primary = [symbol for symbol in symbols if symbol.kind != "helper"]
    else:
        primary = _select_symbols(
            indexed,
            _topic_symbol_names(board, topic),
            label=f"{board}/{topic_name}",
        )

    lines = [
        "FerroWasp app context route",
        f"board: {board}",
        f"topic: {topic_name}",
        f"purpose: {topic.summary}",
        "",
        "Required routing files:",
    ]
    for path in (*target.instructions, target.readme, target.cargo_toml):
        lines.append(f"- {path.as_posix()}")

    lines.extend(
        [
            "",
            "RTIC resources:",
            *(
                f"- {block.name}: {target.source.as_posix()}:"
                f"{block.start_line}-{block.end_line} ({block.field_count} fields)"
                for block in resources
            ),
            "",
            "Primary source anchors:",
        ]
    )
    lines.extend(_describe_symbol(target.source, symbol) for symbol in primary)
    lines.append(f"Primary routed source: {_selected_bytes(primary)} B")

    if board == "foxeer-f405-v2" and topic_name != "overview":
        golden_target = APP_TARGETS["fcu3"]
        golden_symbols, _ = discover_symbols(root, golden_target)
        golden_index = _index_symbols(golden_symbols)
        golden_names = (
            topic.golden_symbols
            if topic.golden_symbols is not None
            else topic.common_symbols
        )
        golden = _select_symbols(
            golden_index,
            golden_names,
            label=f"fcu3 golden comparison/{topic_name}",
        )
        if golden:
            lines.extend(["", "FCU3 golden-app comparison anchors:"])
            lines.extend(
                _describe_symbol(golden_target.source, symbol) for symbol in golden
            )
            lines.append(f"Golden routed source: {_selected_bytes(golden)} B")

    companion_paths = (
        PurePosixPath("project_docs/testing/README.md"),
        PurePosixPath("project_docs/testing/TEST_CATALOG.json"),
        target.bsp_directory,
        *topic.companions,
    )
    lines.extend(["", "Companion context:"])
    for path in dict.fromkeys(companion_paths):
        lines.append(f"- {path.as_posix()}")

    lines.extend(
        [
            "",
            "Potential catalog chain if app code changes:",
            f"- {' -> '.join(target.test_chain)}",
            "",
            "This route selects context only. It does not authorize hardware work,",
            "decide that every listed test is required, or claim that a test passed.",
        ]
    )

    output = "\n".join(lines) + "\n"
    output_bytes = len(output.encode("utf-8"))
    if output_bytes > MAX_OUTPUT_BYTES:
        raise RouteError(
            f"{board}/{topic_name}: output is {output_bytes} bytes; "
            f"limit is {MAX_OUTPUT_BYTES}"
        )
    return output


def build_route(
    board: str,
    topic: str,
    *,
    root: Path = REPOSITORY_ROOT,
) -> str:
    if board not in APP_TARGETS:
        raise RouteError(f"unknown board: {board}")
    if topic not in TOPIC_ROUTES:
        raise RouteError(f"unknown topic: {topic}")
    return _render_route(root.resolve(), board, topic)


def validate_routes(root: Path = REPOSITORY_ROOT) -> list[str]:
    """Return route drift and output-budget failures."""

    root = root.resolve()
    errors: list[str] = []
    catalog_path = root / "project_docs" / "testing" / "TEST_CATALOG.json"
    try:
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        test_ids = {
            entry["id"]
            for entry in catalog["tests"]
            if isinstance(entry, dict) and isinstance(entry.get("id"), str)
        }
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        errors.append(f"cannot read test catalog identities: {error}")
        test_ids = set()

    discovered_by_board: dict[str, list[RustSymbol]] = {}
    for board, target in APP_TARGETS.items():
        required_paths = (
            target.source,
            target.readme,
            target.cargo_toml,
            target.bsp_directory,
            *target.instructions,
        )
        for path in required_paths:
            if not _repository_path(root, path).exists():
                errors.append(f"{board}: routed path does not exist: {path.as_posix()}")

        for test_id in target.test_chain:
            if test_id not in test_ids:
                errors.append(f"{board}: routed test ID does not exist: {test_id}")

        try:
            discovered_by_board[board], _ = discover_symbols(root, target)
        except RouteError as error:
            errors.append(f"{board}: {error}")
            discovered_by_board[board] = []

    for topic_name, topic in TOPIC_ROUTES.items():
        for path in topic.companions:
            if not _repository_path(root, path).exists():
                errors.append(
                    f"{topic_name}: companion path does not exist: {path.as_posix()}"
                )

        for board in APP_TARGETS:
            if not _topic_supported(board, topic_name, topic):
                continue
            indexed = _index_symbols(discovered_by_board[board])
            for symbol_name in _topic_symbol_names(board, topic):
                if symbol_name not in indexed:
                    errors.append(
                        f"{board}/{topic_name}: routed symbol `{symbol_name}` "
                        "was not found"
                    )
            try:
                _render_route(root, board, topic_name)
            except RouteError as error:
                errors.append(str(error))

    for board, symbols in discovered_by_board.items():
        routed_task_names = {
            name
            for topic in TOPIC_ROUTES.values()
            for name in _topic_symbol_names(board, topic)
        }
        for symbol in symbols:
            if symbol.kind == "helper" or symbol.name in routed_task_names:
                continue
            errors.append(f"{board}: RTIC task `{symbol.name}` has no topic route")

    return sorted(set(errors))


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--board",
        choices=tuple(APP_TARGETS),
        default="fcu3",
        help="App to route. Foxeer topics include FCU3 golden-app anchors.",
    )
    parser.add_argument(
        "--topic",
        choices=tuple(TOPIC_ROUTES),
        default="overview",
        help="Bounded context topic. Defaults to overview.",
    )
    parser.add_argument(
        "--list-topics",
        action="store_true",
        help="List stable topic names and exit.",
    )
    parser.add_argument(
        "--validate",
        action="store_true",
        help="Validate every route, symbol, companion path, test ID, and output budget.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.list_topics:
        for name, topic in TOPIC_ROUTES.items():
            print(f"{name}: {topic.summary}")
        return 0

    if args.validate:
        errors = validate_routes()
        if errors:
            print("app context route validation failed:", file=sys.stderr)
            for error in errors:
                print(f"- {error}", file=sys.stderr)
            return 1
        print("app context route validation passed")
        return 0

    try:
        print(build_route(args.board, args.topic), end="")
    except RouteError as error:
        print(f"app context route failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
