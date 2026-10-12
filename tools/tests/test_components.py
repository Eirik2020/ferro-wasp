import tempfile
import unittest
from pathlib import Path

from tools.components import (
    assign_files,
    component_status,
    content_id,
    validate_component_registry,
)


REGISTRY = {
    "schema_version": 1,
    "source_roots": ["crates"],
    "components": [
        {"id": "drivers", "paths": ["crates/drivers"]},
        {"id": "driver-imu", "paths": ["crates/drivers/src/imu.rs"]},
    ],
}


class ComponentTests(unittest.TestCase):
    def test_a_file_belongs_to_the_longest_matching_path(self) -> None:
        owned, uncovered = assign_files(
            REGISTRY,
            [
                "crates/drivers/src/imu.rs",
                "crates/drivers/src/lib.rs",
                "crates/drivers-extra/src/lib.rs",
            ],
        )

        self.assertEqual(owned["driver-imu"], ["crates/drivers/src/imu.rs"])
        self.assertEqual(owned["drivers"], ["crates/drivers/src/lib.rs"])
        self.assertEqual(uncovered, ["crates/drivers-extra/src/lib.rs"])

    def test_content_id_follows_file_content_and_path(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text("one\n", encoding="utf-8")
            (root / "b.rs").write_text("one\n", encoding="utf-8")
            first = content_id(root, ["a.rs"])

            self.assertEqual(first, content_id(root, ["a.rs"]))
            self.assertNotEqual(first, content_id(root, ["b.rs"]))
            (root / "a.rs").write_text("two\n", encoding="utf-8")
            self.assertNotEqual(first, content_id(root, ["a.rs"]))

    def test_a_pass_proves_only_the_content_it_recorded(self) -> None:
        records = [
            {
                "run_id": "old",
                "test_id": "BENCH-COMMON-001",
                "components": [
                    {"id": "drivers", "content_sha256": "1" * 64},
                    {"id": "driver-imu", "content_sha256": "2" * 64},
                ],
            }
        ]

        status = component_status(
            {"drivers": "1" * 64, "driver-imu": "3" * 64},
            records,
        )

        self.assertEqual(status["drivers"]["proven_by"], ["old"])
        self.assertEqual(status["driver-imu"]["proven_by"], [])
        self.assertEqual(status["driver-imu"]["earlier_tests"], ["BENCH-COMMON-001"])

    def test_registry_rejects_a_path_claimed_twice(self) -> None:
        registry = {
            **REGISTRY,
            "components": [
                {"id": "a", "paths": ["crates/x"]},
                {"id": "b", "paths": ["crates/x"]},
            ],
        }

        errors = validate_component_registry(registry, "registry")

        self.assertTrue(any("more than one component" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
