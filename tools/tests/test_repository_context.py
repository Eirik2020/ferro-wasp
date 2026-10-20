import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.check_repository_context import (
    _discover_agent_instruction_paths,
    validate_always_loaded,
    validate_architecture_decisions,
    validate_internal_book,
    validate_repository,
)


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
FIXTURES = Path(__file__).resolve().parent / "fixtures" / "repository_context"


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

    def test_retired_and_active_decisions_sit_in_their_own_index(self) -> None:
        errors = validate_architecture_decisions(FIXTURES / "adr_history")

        self.assertTrue(any("ADR-0002 is retired" in e and "HISTORY" in e for e in errors), errors)
        self.assertTrue(any("ADR-0004 is active" in e for e in errors), errors)
        self.assertFalse(any("ADR-0001" in e or "ADR-0003" in e for e in errors), errors)
        self.assertFalse(any("orphan" in e or "does not resolve" in e for e in errors), errors)

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

    def test_root_agent_instructions_are_the_only_ones(self) -> None:
        registry_path = REPOSITORY_ROOT / "project_meta" / "DOCUMENT_REGISTRY.json"
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        agent_budgets = {
            entry["path"]: entry["max_bytes"]
            for entry in registry["context_budgets"]
            if Path(entry["path"]).name == "AGENTS.md"
        }
        self.assertEqual(agent_budgets, {"AGENTS.md": 13312})
        self.assertEqual(
            _discover_agent_instruction_paths(REPOSITORY_ROOT),
            {"AGENTS.md"},
        )
        self.assertEqual(validate_always_loaded(REPOSITORY_ROOT), [])

    def test_internal_book_lists_exactly_the_live_documents(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "project_meta").mkdir()
            (root / "project_meta" / "SUMMARY.md").write_text(
                "- [Live](live.md)\n- [Old](archive/old.md)\n",
                encoding="utf-8",
            )
            registry = {
                "documents": [
                    {"path": "project_meta/live.md", "lifecycle": "live"},
                    {"path": "project_meta/durable.md", "lifecycle": "durable"},
                    {"path": "project_meta/archive/old.md", "lifecycle": "historical"},
                ]
            }

            errors = validate_internal_book(root, registry)

        self.assertEqual(
            errors,
            [
                "project_meta/SUMMARY.md: missing chapter project_meta/durable.md",
                "project_meta/SUMMARY.md: lists a historical or unregistered "
                "document project_meta/archive/old.md",
            ],
        )

    def test_rejects_oversized_always_loaded_context(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "CLAUDE.md").write_text("@AGENTS.md\n@missing.md\n", encoding="utf-8")
            (root / "AGENTS.md").write_text("line\n" * 400, encoding="utf-8")

            errors = validate_always_loaded(root)

        self.assertIn("CLAUDE.md import does not resolve: missing.md", errors)
        self.assertIn(
            "always-loaded context is 402 lines, over the 400-line limit",
            errors,
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


if __name__ == "__main__":
    unittest.main()
