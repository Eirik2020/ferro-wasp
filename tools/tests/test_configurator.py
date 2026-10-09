import unittest

from tools.configurator import CONFIGURATOR, GUI, plan


class ConfiguratorLauncherTests(unittest.TestCase):
    def test_the_app_runs_tauri_from_the_gui_project(self) -> None:
        self.assertEqual(plan("app", [], gui_installed=True), [(GUI, ["npm", "run", "tauri", "dev"])])

    def test_missing_npm_packages_are_installed_first(self) -> None:
        steps = plan("preview", [], gui_installed=False)
        self.assertEqual(steps, [(GUI, ["npm", "install"]), (GUI, ["npm", "run", "dev"])])

    def test_the_cli_passes_its_arguments_through(self) -> None:
        steps = plan("cli", ["--port", "/dev/ttyACM0", "device", "info"], gui_installed=False)
        self.assertEqual(
            steps,
            [
                (
                    CONFIGURATOR,
                    [
                        "cargo", "run", "--release", "--quiet", "-p", "ferro-configurator-cli",
                        "--", "--port", "/dev/ttyACM0", "device", "info",
                    ],
                )
            ],
        )
        self.assertTrue((GUI / "package.json").is_file())


if __name__ == "__main__":
    unittest.main()
