# FerroWasp Host Tools

Python and PowerShell tools run from the repository root. None has motor or
safety authority. Each is documented in one book chapter:

| Tool | What | Documented in |
|---|---|---|
| `terminal_embed.py` | Build, flash over SWD, decode and log RTT | [Bench and Analysis Tools](../mdbook/src/rtt_debug_tools.md) |
| `imu_live_view.py`, `accel_live_plot.py` | Live plots of RTT output | same |
| `blackbox_analyzer.py`, `imu_noise_analyzer.py` | Log and noise analysis | same |
| `flight_report.py` | Report and plots from a log | same |
| `bench_rig.py`, `foxeer_bench_gates.py` | Run bench gates and draft run records | `project_meta/testing/` procedures |
| `components.py`, `test_evidence.py` | Component IDs and run-record checks | `project_meta/testing/evidence/README.md` |
| `app_context.py`, `check_repository_context.py`, `check_rtic_boundaries.py`, `release/` | Repository routing and checks | `project_meta/tooling.md` |
| `dev/` | Development-container scripts | [Developer Getting Started](../mdbook/src/developer_getting_started.md) |
| `ferro-configurator/` | FerroConfigurator, its own workspace | [its README](ferro-configurator/README.md) |
| `rtic-app-builder/` | The earlier app generator, due to be retired | its README |

Tests: `python -m unittest discover -s tools/tests -p "test_*.py"`.
