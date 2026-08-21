import json
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.check_repository_context import (
    validate_architecture_decisions,
    validate_repository,
    validate_test_catalog,
)


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
FIXTURES = Path(__file__).resolve().parent / "fixtures" / "repository_context"
TEST_CATALOG_FIXTURES = Path(__file__).resolve().parent / "fixtures" / "test_catalog"


class RepositoryContextTests(unittest.TestCase):
    def test_current_repository_passes(self) -> None:
        self.assertEqual(validate_repository(REPOSITORY_ROOT), [])

    def test_architecture_decisions_are_bounded_and_consistent(self) -> None:
        self.assertEqual(validate_architecture_decisions(REPOSITORY_ROOT), [])

        registry_path = REPOSITORY_ROOT / "project_meta" / "DOCUMENT_REGISTRY.json"
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        documents = {entry["path"]: entry for entry in registry["documents"]}
        adr_entries = {
            path: entry
            for path, entry in documents.items()
            if path.startswith("project_meta/decisions/ADR-")
        }

        discovered_adrs = {
            path.relative_to(REPOSITORY_ROOT).as_posix()
            for path in (REPOSITORY_ROOT / "project_meta" / "decisions").glob(
                "ADR-*.md"
            )
        }
        self.assertEqual(set(adr_entries), discovered_adrs)
        self.assertEqual(
            documents["project_meta/ARCHITECTURE_DECISIONS.md"]["max_bytes"],
            8192,
        )
        self.assertTrue(
            all(entry["max_bytes"] == 4096 for entry in adr_entries.values())
        )

    def test_architecture_decision_guard_rejects_drift(self) -> None:
        errors = validate_architecture_decisions(FIXTURES / "invalid_adr")

        self.assertTrue(any("H1 ID ADR-0002 does not match filename" in e for e in errors))
        self.assertTrue(any("duplicate ADR H1 ID: ADR-0002" in e for e in errors))
        self.assertTrue(any("exactly one non-empty Status line" in e for e in errors))
        self.assertTrue(any("duplicate ADR index entry: ADR-0001" in e for e in errors))
        self.assertTrue(any("ADR index entry does not resolve: ADR-0003" in e for e in errors))
        self.assertTrue(any("orphan ADR file missing from index: ADR-0002" in e for e in errors))
        self.assertTrue(any("unknown ADR reference ADR-9999" in e for e in errors))
        self.assertTrue(any("supersession target must not be itself" in e for e in errors))

    def test_agent_instruction_tree_is_budgeted_and_root_routed(self) -> None:
        registry_path = REPOSITORY_ROOT / "project_meta" / "DOCUMENT_REGISTRY.json"
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        agent_budgets = {
            entry["path"]: entry["max_bytes"]
            for entry in registry["context_budgets"]
            if Path(entry["path"]).name == "AGENTS.md"
        }
        expected = {
            "AGENTS.md": 6144,
            "apps/AGENTS.md": 5120,
            "apps/foxeer-f405-v2/AGENTS.md": 4096,
            "mdbook/AGENTS.md": 4096,
            "project_meta/AGENTS.md": 4096,
            "tools/AGENTS.md": 4096,
            "tools/ferro-configurator/AGENTS.md": 4096,
            "tools/app-builder-v3/AGENTS.md": 4096,
            "tools/app-builder/AGENTS.md": 4096,
            "tools/rtic-app-builder/AGENTS.md": 5120,
        }
        self.assertEqual(agent_budgets, expected)

        root_instructions = (REPOSITORY_ROOT / "AGENTS.md").read_text(encoding="utf-8")
        for path in expected.keys() - {"AGENTS.md"}:
            with self.subTest(path=path):
                self.assertIn(path, root_instructions)
                self.assertLessEqual(
                    (REPOSITORY_ROOT / path).stat().st_size,
                    expected[path],
                )

    def test_rejects_unbudgeted_agent_instruction(self) -> None:
        with patch(
            "tools.check_repository_context._discover_agent_instruction_paths",
            return_value={"nested/AGENTS.md"},
        ):
            errors = validate_repository(FIXTURES / "invalid_budget")

        self.assertIn(
            "unbudgeted agent instruction file: nested/AGENTS.md",
            errors,
        )

    def test_reports_unregistered_document_and_budget_overflow(self) -> None:
        errors = validate_repository(FIXTURES / "invalid_budget")
        self.assertTrue(
            any("unregistered project document: project_meta/EXTRA.md" in e for e in errors)
        )
        self.assertTrue(
            any("CONTEXT.md:" in e and "exceeds budget 4" in e for e in errors)
        )

    def test_rejects_modified_immutable_archive(self) -> None:
        errors = validate_repository(FIXTURES / "invalid_budget")
        self.assertTrue(
            any(
                "CONTEXT.md: immutable size 10 does not match 1" in error
                for error in errors
            )
        )
        self.assertTrue(
            any(
                "CONTEXT.md: immutable SHA-256" in error
                and "does not match" in error
                for error in errors
            )
        )

    def test_rejects_historical_default_context_and_multiple_current_states(self) -> None:
        errors = validate_repository(FIXTURES / "invalid_historical")
        self.assertTrue(any("historical documents must use context 'exclude'" in e for e in errors))
        self.assertTrue(any("2 Current State headings exceeds limit 1" in e for e in errors))

    def test_rejects_unsafe_flight_definition(self) -> None:
        errors = validate_test_catalog(
            TEST_CATALOG_FIXTURES / "invalid_flight",
            "TEST_CATALOG.json",
        )

        self.assertTrue(any("requires user execution" in error for error in errors))
        self.assertTrue(any("hardware tiers require stop conditions" in error for error in errors))
        self.assertTrue(any("evidence must not be empty" in error for error in errors))
        self.assertTrue(any("flight tests require a preflight prerequisite" in error for error in errors))
        self.assertTrue(any("heading 'Missing Heading' not found" in error for error in errors))
        self.assertTrue(any("test cannot require itself" in error for error in errors))
        self.assertTrue(any("test prerequisite cycle" in error for error in errors))
        self.assertTrue(
            any(
                "active procedure must not point into an archive" in error
                for error in errors
            )
        )
        self.assertTrue(
            any(
                "active target hardware procedure must use "
                "project_meta/testing/targets/" in error
                for error in errors
            )
                )

    def test_common_bench_gate_remains_active_and_routed(self) -> None:
        catalog_path = REPOSITORY_ROOT / "project_meta" / "testing" / "TEST_CATALOG.json"
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        definitions = {entry["id"]: entry for entry in catalog["tests"]}
        entry = definitions["BENCH-COMMON-001"]

        self.assertEqual(entry["status"], "active")
        self.assertEqual(entry["target"], "common")
        self.assertEqual(entry["tier"], "bench-unpowered")
        self.assertEqual(entry["prerequisites"], ["SW-COMMON-001"])
        self.assertEqual(
            entry["procedure"]["path"],
            "project_meta/testing/targets/common-boot-idle.md",
        )
        self.assertEqual(entry["procedure"]["heading"], "Boot and Idle State")

    def test_evidence_policy_remains_bounded_and_artifact_only(self) -> None:
        registry_path = REPOSITORY_ROOT / "project_meta" / "DOCUMENT_REGISTRY.json"
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        policy = registry["test_evidence"]

        self.assertEqual(policy["max_record_bytes"], 8192)
        self.assertEqual(policy["artifact_roots"], ["logs"])
        self.assertEqual(
            policy["records_root"],
            "project_meta/testing/evidence/runs",
        )
        self.assertEqual(
            policy["template"],
            "project_meta/testing/evidence/RUN_RECORD_TEMPLATE.json",
        )

    def test_foxeer_rollout_remains_active_and_ordered(self) -> None:
        catalog_path = REPOSITORY_ROOT / "project_meta" / "testing" / "TEST_CATALOG.json"
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        definitions = {entry["id"]: entry for entry in catalog["tests"]}
        expected_prerequisite = {
            "BENCH-FOX-USB-001": "BENCH-COMMON-001",
            "BENCH-FOX-001": "BENCH-FOX-USB-001",
            "PREFLIGHT-FOX-001": "BENCH-FOX-001",
            "FLIGHT-FOX-001": "PREFLIGHT-FOX-001",
        }

        for test_id, prerequisite in expected_prerequisite.items():
            with self.subTest(test_id=test_id):
                entry = definitions[test_id]
                self.assertEqual(entry["status"], "active")
                self.assertIn(prerequisite, entry["prerequisites"])
                self.assertEqual(
                    entry["procedure"]["path"],
                    "project_meta/testing/targets/foxeer-f405-v2.md",
                )

    def test_fcu3_rollout_remains_active_and_ordered(self) -> None:
        catalog_path = REPOSITORY_ROOT / "project_meta" / "testing" / "TEST_CATALOG.json"
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        definitions = {entry["id"]: entry for entry in catalog["tests"]}
        expected_prerequisite = {
            "BENCH-FCU3-DSHOT-001": "BENCH-COMMON-001",
            "PREFLIGHT-FCU3-001": "BENCH-FCU3-DSHOT-001",
            "FLIGHT-FCU3-001": "PREFLIGHT-FCU3-001",
        }

        for test_id, prerequisite in expected_prerequisite.items():
            with self.subTest(test_id=test_id):
                entry = definitions[test_id]
                self.assertEqual(entry["status"], "active")
                self.assertIn(prerequisite, entry["prerequisites"])
                self.assertEqual(
                    entry["procedure"]["path"],
                    "project_meta/testing/targets/fcu3.md",
                )


if __name__ == "__main__":
    unittest.main()
