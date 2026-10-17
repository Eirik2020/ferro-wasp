import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools import reqtool_bridge

FIXTURE = Path(__file__).resolve().parents[1] / "reqtool/tests/fixtures/repo"
BENCH = "project_meta/testing/procedures.md#Bench Gate"


@unittest.skipUnless(reqtool_bridge.REQTOOL.is_file(), "reqtool is not built")
class BridgeTests(unittest.TestCase):
    """The details the gate tools build are what reqtool accepts."""

    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        shutil.copytree(FIXTURE, self.root, dirs_exist_ok=True)
        git = ["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid"]
        for args in (["init", "-q"], ["add", "-A"], ["commit", "-q", "-m", "fixture"]):
            subprocess.run([*git, *args], cwd=self.root, check=True, capture_output=True)
        (self.root / "logs/bench").mkdir(parents=True)
        self.log = self.root / "logs/bench/boot-1-swd-reset.log"
        self.log.write_text("boot ok\n")

    def record(self, propellers: str) -> subprocess.CompletedProcess[str]:
        details = reqtool_bridge.details(
            firmware_sha256="0" * 64,
            features=["board-demo"],
            target_triple="thumbv7em-none-eabihf",
            propellers=propellers,
            actuator_power="connected",
            equipment=["bench supply"],
            environment=["bench"],
            commands=[{"command": "cargo build --release --locked", "exit_code": 0}],
            artifacts=[(self.log, "log")],
            measurements={"dshot_nonzero_output_reports": 0},
            observations=["outputs at stop"],
            limitations=["no oscilloscope"],
            root=self.root,
        )
        path = self.root / "logs/bench" / reqtool_bridge.DETAILS_FILE
        path.write_text(json.dumps(details))
        command = reqtool_bridge.command(BENCH, "demo-board", "pass", path, root=self.root)
        return subprocess.run([*command, "--dry-run"], capture_output=True, text=True)

    def test_a_props_off_run_is_accepted_with_its_artifact_hashed(self) -> None:
        done = self.record("removed")
        self.assertEqual(done.returncode, 0, done.stderr)
        record = json.loads(done.stdout)
        artifact = record["details"]["artifacts"][0]
        self.assertEqual(artifact["id"], "boot-1-swd-reset")
        self.assertEqual(artifact["bytes"], 8)
        self.assertEqual(len(artifact["sha256"]), 64)
        self.assertEqual(record["results"][0]["requirement"], "INPUT-001")

    def test_a_props_off_pass_with_propellers_installed_is_refused(self) -> None:
        done = self.record("installed")
        self.assertNotEqual(done.returncode, 0)
        self.assertIn("propellers removed", done.stderr)


if __name__ == "__main__":
    unittest.main()
