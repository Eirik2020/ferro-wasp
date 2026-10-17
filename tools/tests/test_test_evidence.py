import unittest
from pathlib import Path

from tools.test_evidence import validate_evidence_records


FIXTURE_ROOT = Path(__file__).resolve().parent / "fixtures" / "test_evidence"


class TestEvidenceTests(unittest.TestCase):
    def _policy(
        self,
        records_root: str,
        *,
        max_record_bytes: int = 8192,
        components: str | None = None,
    ) -> dict:
        policy = {
            "records_root": records_root,
            "template": "RUN_RECORD_TEMPLATE.json",
            "max_record_bytes": max_record_bytes,
            "artifact_roots": ["logs"],
        }
        if components is not None:
            policy["components"] = components
        return policy

    def _validate(
        self,
        records_root: str,
        *,
        max_record_bytes: int = 8192,
        components: str | None = None,
    ) -> list[str]:
        return validate_evidence_records(
            FIXTURE_ROOT,
            self._policy(
                records_root,
                max_record_bytes=max_record_bytes,
                components=components,
            ),
            "TEST_CATALOG.json",
        )

    def test_accepts_bounded_software_record(self) -> None:
        self.assertEqual(
            self._validate("records/valid_software"),
            [],
        )

    def test_leaves_reqtool_schema_3_records_to_reqtool(self) -> None:
        self.assertEqual(
            self._validate("records/with_reqtool"),
            [],
        )

    def test_accepts_user_confirmed_hardware_record_and_matching_artifact(
        self,
    ) -> None:
        self.assertEqual(
            self._validate("records/valid_hardware"),
            [],
        )

    def test_rejects_unsafe_hardware_pass_and_embedded_samples(self) -> None:
        errors = self._validate("records/invalid")

        self.assertTrue(any("may not embed raw artifact content" in e for e in errors))
        self.assertTrue(
            any("hardware evidence cannot be agent-only" in e for e in errors)
        )
        self.assertTrue(any("requires user confirmation" in e for e in errors))
        self.assertTrue(any("requires firmware SHA-256" in e for e in errors))
        self.assertTrue(any("requires power and propeller state" in e for e in errors))
        self.assertTrue(any("pass requires known power" in e for e in errors))
        self.assertTrue(any("pass cannot include triggered" in e for e in errors))

    def test_rejects_catalog_mismatch_and_bad_record_location(self) -> None:
        errors = self._validate("records/invalid")

        self.assertTrue(any("must use runs/YYYY/MM" in e for e in errors))
        self.assertTrue(
            any("target 'common' does not match catalog" in e for e in errors)
        )
        self.assertTrue(
            any("tier 'flight' does not match catalog" in e for e in errors)
        )

    def test_rejects_artifact_mismatch_and_unsafe_path(self) -> None:
        errors = self._validate("records/invalid")

        self.assertTrue(any("artifact size" in e for e in errors))
        self.assertTrue(any("artifact SHA-256" in e for e in errors))
        self.assertTrue(any("must stay within the repository" in e for e in errors))

    def test_rejects_record_and_policy_budget_growth(self) -> None:
        record_errors = self._validate(
            "records/valid_software",
            max_record_bytes=256,
        )
        policy_errors = self._validate(
            "records/valid_software",
            max_record_bytes=16384,
        )

        self.assertTrue(any("exceeds evidence budget 256" in e for e in record_errors))
        self.assertTrue(
            any("max_record_bytes must not exceed 8192" in e for e in policy_errors)
        )

    def test_accepts_schema_2_record_naming_known_components(self) -> None:
        self.assertEqual(
            self._validate("records/valid_components", components="COMPONENTS.json"),
            [],
        )

    def test_rejects_unknown_repeated_empty_and_missing_components(self) -> None:
        errors = self._validate(
            "records/invalid_components",
            components="COMPONENTS.json",
        )

        self.assertTrue(any("unknown component gamma" in e for e in errors))
        self.assertTrue(any("component alpha is listed twice" in e for e in errors))
        self.assertTrue(any("components must be a non-empty list" in e for e in errors))
        self.assertTrue(any("missing fields: components" in e for e in errors))

    def test_reports_a_source_file_no_component_owns(self) -> None:
        errors = self._validate(
            "records/valid_components",
            components="COMPONENTS_UNCOVERED.json",
        )

        self.assertTrue(
            any("belongs to no component: src/beta.rs.fixture" in e for e in errors)
        )


if __name__ == "__main__":
    unittest.main()
