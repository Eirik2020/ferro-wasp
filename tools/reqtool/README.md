# reqtool

FerroWasp's requirements, gates and evidence records are checked by
[reqtool](https://github.com/Eirik2020/reqtool), a separate project whose README
holds its rules and commands. `revision` here pins the exact commit FerroWasp uses;
`reqtool.toml` at the repository root holds FerroWasp's settings.

Install it at that revision, then run it from anywhere in the repository:

```text
python tools/run_tests.py reqtool
tools/reqtool/bin/reqtool check
```

Moving to a newer revision is a change to `revision` alone, and it may turn evidence
stale, as any change to what records are judged by would.

## What FerroWasp uses

reqtool's terms are generic. Here they mean:

| Level | In FerroWasp | Hazard and preconditions |
|---|---|---|
| `analysis` | tool checks on source and images | none |
| `unit` | host tests, including conformance suites | none |
| `simulation` | software in the loop, once a harness exists | none |
| `target` | on the flight controller, nothing actuating | none |
| `system` | the full drone, ESCs powered, propellers removed | `equipment`, `propellers: removed` |
| `field` | in flight | `personnel`, `propellers: installed` |

An environment of `product` is a flight controller, and `development` a development
board. A person runs and confirms anything with a hazard; an agent may run the rest
while the system is disarmed. A `target` step that needs hands, such as moving a
board, still needs a person there, and the record says who did what.

FerroWasp also turns on reqtool's safety-critical tracing, marking crates with
`[package.metadata.ferrowasp] critical = true`. Its FAQ's format is checked separately,
by `tools/check_faq.py`.
