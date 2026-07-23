# RTIC Feature-Assembler MVP Implementation Plan

> **Current timing note:** This historical plan predates schema 6. The live
> examples now derive blink, debounce, and OSD refresh delays from one
> backend-owned 1 kHz SysTick monotonic; TIM2/TIM3/TIM4 are no longer BSP
> reservations. See `docs/stm32f4-backend.md` and the checked-in manifests for
> current behavior.

> **Scope note:** This is the original NUCLEO-F401RE blink MVP plan. It is not
> the post-MVP FerroWasp integration architecture or a current FerroWasp board
> roadmap. The canonical future target/platform-configuration direction is in
> `docs/betaflight-target-definition-notes.md`; live board priorities and
> support status remain in the FerroWasp repository. Paths, schemas, and
> commands below preserve the original plan and are not current usage
> instructions; use `README.md` for the implemented interface.

## 1. Purpose

Create a Rust workspace that can assemble a valid RTIC application from:

1. An exhaustive hardware/application manifest.
2. A library of manually written RTIC feature bundles.
3. A host-side `xtask` builder.

The MVP target is the **ST NUCLEO-F401RE** development board.

The generated RTIC application must:

- Configure the user LED on PA5.
- Configure TIM2 to generate a periodic update interrupt.
- Bind a handwritten RTIC hardware task to TIM2.
- Toggle the LED every 500 ms.
- Insert and validate the feature incrementally.
- Preserve the last valid generated application when insertion fails.
- Produce a final embedded build after all selected features pass.

The MVP exists to validate the assembler architecture. It does not attempt to reproduce the complete FerroWasp application.

---

# 2. Development Strategy

Development must proceed through explicit risk gates:

1. **Development board**
   - Generate and compile a minimal application.
   - Flash a NUCLEO-F401RE.
   - Verify LED behavior only.
   - No actuators or hazardous hardware are connected.

2. **Bench integration**
   - Add peripherals one at a time.
   - Validate generated configuration against datasheets and the known-working FerroWasp implementation.
   - Exercise interfaces using instrumentation and non-energized or current-limited hardware.
   - Add regression tests before introducing the next feature.

3. **Controlled flight testing**
   - Begin only after bench acceptance criteria are met.
   - Use a restrained or otherwise controlled test setup before free flight.
   - Introduce actuator authority gradually.
   - Preserve a known-working rollback configuration and firmware.
   - Record logs and review failures before expanding the test envelope.

This staged approach reduces risk, but successful compilation, bench testing, or limited flight testing does not by itself establish that the system is safe or certifiable.

---

# 3. Fundamental Architecture

```text
Hardware/application manifest
             ↓
Manually written feature library
             ↓
        xtask assembler
             ↓
Generated RTIC application
             ↓
 Incremental cargo checks
             ↓
      Final release build
```

## 3.1 Hardware/application manifest

The manifest exhaustively declares every configurable or safety-relevant choice required by a feature contract.

The builder must not infer or automatically allocate:

- GPIO pins
- Peripheral instances
- Interrupt vectors
- Task priorities
- Clock configuration
- Timer periods or frequencies
- GPIO pins or their default physical level
- DMA controller, stream, channel, direction, or mode
- Buffer sizes
- UART framing
- SPI mode
- Resource ownership
- Queue sizes
- Error-handling policy
- Feature ordering

The builder may translate explicit declarations into MCU-specific Rust syntax, but it must not select hardware on behalf of the application author.

Feature-specific electrical policy may be fixed by a documented backend
contract. For `blink_led`, push-pull output mode, no pull resistor, and low
speed are task-backend behavior rather than manifest fields.

## 3.2 Feature library

The feature library contains manually written RTIC and HAL source fragments.

The intended workflow is:

```text
Write and debug a normal RTIC application
                ↓
Extract the task, imports, resources, and init code
                ↓
Replace selected hardware values with explicit placeholders
                ↓
Store the fragments as a feature bundle
                ↓
Validate the bundle in a generated minimal RTIC application
```

The handwritten feature should remain recognizably normal Rust and RTIC code. The builder must not generate RTIC task syntax from a semantic task description.

## 3.3 xtask assembler

The assembler:

1. Loads and validates the manifest.
2. Loads the selected feature bundles.
3. Generates a minimal empty RTIC application.
4. Runs `cargo check` on the empty application.
5. Inserts one feature into a newly rendered candidate application.
6. Formats and syntax-checks the candidate.
7. Runs `cargo check`.
8. Promotes the candidate when successful.
9. Preserves the previous valid application and failed candidate on failure.
10. Repeats for each feature in manifest order.
11. Runs a final `cargo build --release --locked`.

## 3.4 Generated application

The generated application is disposable and normally regenerated from scratch.

The sources of truth are:

- Hardware/application manifest
- Feature-library source fragments
- Backend translation rules
- Builder implementation
- Locked dependency and toolchain versions

Generated Rust source must not become a manually maintained second source of truth.

---

# 4. Definition of Exhaustive Configuration

“Exhaustive” means:

> Every configurable or safety-relevant choice required by a feature contract is explicitly declared, while fixed HAL behavior and backend translation rules are documented and versioned.

It does not require the manifest to duplicate every register bit implemented by the HAL.

For the LED MVP, the manifest must explicitly declare:

- MCU and compilation target
- HAL device feature
- Flash and RAM layout
- Clock source and frequency
- System clock frequency
- Compact GPIO pin token
- Default physical output level
- Timer peripheral
- Timer event
- Interrupt vector
- Timer operating mode
- Toggle period
- RTIC task priority
- Feature insertion order

Any unsupported or missing field must stop generation.

---

# 5. Core Design Rules

## 5.1 Strict manifest parsing

Use:

```rust
#[serde(deny_unknown_fields)]
```

Do not provide hidden defaults for required fields.

Avoid:

```rust
#[serde(default)]
```

unless a field is deliberately optional, documented, and non-safety-relevant.

Errors must identify the complete configuration path:

```text
Missing required field:
features.blink_led.timer.interrupt
```

## 5.2 Translation is permitted; selection is forbidden

Permitted:

```text
pin = "PA5" → gpioa.pa5
peripheral = "TIM2" → dp.TIM2
interrupt = "TIM2" → binds = TIM2
```

Forbidden:

```text
No timer supplied → choose TIM2
USART2 requested → find a compatible DMA stream
Priority omitted → assign priority 1
```

## 5.3 Handwritten RTIC syntax

RTIC task attributes and bodies remain handwritten in the feature bundle.

The builder may replace explicitly declared placeholders:

```text
{{TIMER_INTERRUPT}}
{{TASK_PRIORITY}}
{{LED_RESOURCE_NAME}}
{{LED_RESOURCE_TYPE}}
```

It must not reconstruct the RTIC attribute from a high-level task model.

## 5.4 Regenerate complete candidates

Do not incrementally mutate an existing Rust file.

For each feature insertion:

1. Load the base template.
2. Add every previously successful feature.
3. Add the next candidate feature.
4. Render the complete candidate crate.
5. Validate and compile the complete candidate.

## 5.5 Fail closed

The following are fatal:

- Missing fields
- Unknown fields
- Unsupported values
- Unresolved placeholders
- Invalid Rust identifiers
- Duplicate resources
- Duplicate symbols
- Conflicting pins or peripherals
- Conflicting interrupt bindings
- Invalid generated syntax
- Formatting failure
- `cargo check` failure
- Final build failure

## 5.6 Accurate failure attribution

Diagnostics must say:

> Compilation failed after inserting feature `blink_led`.

They must not automatically claim:

> Feature `blink_led` is defective.

The latest insertion may only expose an interaction with existing features.

---

# 6. Proposed Repository Structure

```text
.
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
│
├── xtask/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── cli.rs
│       ├── manifest.rs
│       ├── feature.rs
│       ├── backend.rs
│       ├── validate.rs
│       ├── render.rs
│       ├── syntax.rs
│       ├── runner.rs
│       ├── state.rs
│       └── diagnostics.rs
│
├── templates/
│   └── stm32f4-rtic/
│       ├── Cargo.toml.tpl
│       ├── build.rs.tpl
│       ├── memory.x.tpl
│       └── src/
│           └── main.rs.tpl
│
├── feature-library/
│   └── stm32f4/
│       └── blink-led/
│           ├── feature.toml
│           ├── imports.rs.tpl
│           ├── shared-resources.rs.tpl
│           ├── local-resources.rs.tpl
│           ├── init.rs.tpl
│           ├── constructors.rs.tpl
│           └── tasks.rs.tpl
│
├── hardware/
│   └── nucleo-f401re-blinky.toml
│
├── generated/
│   └── nucleo-f401re-blinky/
│       ├── working/
│       ├── failed/
│       └── build-state.toml
│
└── tests/
    ├── manifests/
    ├── broken-features/
    └── golden/
```

Generated crates should not be regular root-workspace members. Invoke them through `--manifest-path` so working, candidate, and failed crates cannot create duplicate package-name conflicts.

Candidate directories should be created as temporary standalone crates and promoted only after validation.

---

# 7. MVP Hardware Manifest

Create:

```text
hardware/nucleo-f401re-blinky.toml
```

Suggested schema:

```toml
schema_version = 1

[application]
name = "nucleo-f401re-blinky"
target = "thumbv7em-none-eabihf"
feature_order = ["blink_led"]

[platform]
family = "stm32f4"
mcu = "stm32f401ret6"
hal_crate = "stm32f4xx-hal"
hal_feature = "stm32f401"
pac_path = "stm32f4xx_hal::pac"

[memory]
flash_origin = 0x08000000
flash_size_bytes = 524288
ram_origin = 0x20000000
ram_size_bytes = 98304

[clock]
source = "hsi"
source_frequency_hz = 16000000
system_clock_hz = 84000000

[features.blink_led]
implementation = "stm32f4/blink-led"
task_name = "blink_led"
task_priority = 1
toggle_period_ms = 500

[features.blink_led.led]
resource_name = "led2"
pin = "PA5"
default = "high"

[features.blink_led.timer]
resource_name = "blink_timer"
peripheral = "TIM2"
interrupt = "TIM2"
event = "update"
mode = "counter_hz"
clock_source = "apb1_timer_clock"
```

Every field above is mandatory for the MVP.

---

# 8. Feature Bundle Contract

Create:

```text
feature-library/stm32f4/blink-led/
```

## 8.1 Feature metadata

`feature.toml`:

```toml
schema_version = 1
id = "blink_led"

imports = "imports.rs.tpl"
shared_resources = "shared-resources.rs.tpl"
local_resources = "local-resources.rs.tpl"
init = "init.rs.tpl"
constructors = "constructors.rs.tpl"
tasks = "tasks.rs.tpl"

required_placeholders = [
    "LED_RESOURCE_NAME",
    "LED_RESOURCE_TYPE",
    "LED_INIT_EXPRESSION",
    "TIMER_RESOURCE_NAME",
    "TIMER_RESOURCE_TYPE",
    "TIMER_INIT_EXPRESSION",
    "TIMER_INTERRUPT",
    "TASK_PRIORITY",
]

claimed_symbols = [
    "blink_led",
]

required_symbols = []

claimed_resources = [
    "LED_RESOURCE_NAME",
    "TIMER_RESOURCE_NAME",
]

claimed_interrupts = [
    "TIMER_INTERRUPT",
]
```

The exact metadata representation may differ, but it must explicitly expose:

- Files provided by the feature
- Placeholders required
- Symbols defined
- Symbols required
- Logical resources claimed
- Pins, peripherals, and interrupts claimed after resolution
- Any insertion-order dependencies

## 8.2 Imports fragment

Contains only imports required by this feature.

Duplicate identical imports may be deduplicated structurally. Conflicting imports must fail clearly.

## 8.3 Resource fragments

Local resource fragment:

```rust
{{LED_RESOURCE_NAME}}: {{LED_RESOURCE_TYPE}},
{{TIMER_RESOURCE_NAME}}: {{TIMER_RESOURCE_TYPE}},
```

The STM32F4 backend translates the explicit pin token and default level into
concrete HAL syntax. The blink LED backend fixes push-pull mode, no pull
resistor, and low speed.

For the MVP, support only the exact required subset:

- GPIOA PA5 push-pull output
- TIM2 counter
- NUCLEO-F401RE clock configuration

Unsupported values must fail rather than trigger fallback behavior.

## 8.4 Init fragment

The handwritten init fragment must:

1. Configure the declared clock.
2. Split the GPIO port encoded by the declared compact pin token.
3. Configure the declared pin with the backend's fixed mode, pull, and speed.
4. Apply the declared default physical output level.
5. Configure the declared timer.
6. Start the timer at the declared frequency.
7. Enable the declared timer event.
8. Construct the declared local resources.

For the MVP:

```text
toggle_frequency_hz = 1000 / toggle_period_ms
```

Require:

```text
1000 % toggle_period_ms == 0
```

Reject periods that require implicit rounding.

## 8.5 Task fragment

Keep the task handwritten:

```rust
#[task(
    binds = {{TIMER_INTERRUPT}},
    priority = {{TASK_PRIORITY}},
    local = [
        {{LED_RESOURCE_NAME}},
        {{TIMER_RESOURCE_NAME}}
    ]
)]
fn blink_led(cx: blink_led::Context) {
    cx.local.{{TIMER_RESOURCE_NAME}}.clear_all_flags();
    let _ = cx.local.{{LED_RESOURCE_NAME}}.toggle();
}
```

The exact flag-clearing call must match the pinned HAL version.

The feature bundle owns:

- RTIC task syntax
- Resource-list shape
- Task body
- Interrupt acknowledgement order
- LED operation

The manifest supplies the declared hardware and configuration values.

---

# 9. STM32F4 Translation Backend

The builder requires an STM32F4-specific translation backend.

The backend may:

- Validate declared STM32F4 values.
- Translate declared MCU peripherals into Rust tokens.
- Translate declared GPIOs into HAL access expressions and types.
- Translate declared timer settings into HAL initialization expressions.
- Generate Cargo features and linker data from explicit declarations.

The backend may not:

- Select an alternative resource.
- Resolve conflicts by changing the manifest.
- Apply undocumented fallback settings.
- Guess DMA routes, priorities, pins, or buffer sizes.

All fixed backend behavior must be:

- Documented
- Versioned
- Covered by tests
- Traceable to the pinned HAL implementation

---

# 10. Base RTIC Template

The base application template must expose explicit insertion sections:

```rust
#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;

#[rtic::app(
    device = {{PAC_PATH}},
    peripherals = true
)]
mod app {
    {{FEATURE_IMPORTS}}

    #[shared]
    struct Shared {
        {{SHARED_RESOURCES}}
    }

    #[local]
    struct Local {
        {{LOCAL_RESOURCES}}
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        {{BASE_INIT}}
        {{FEATURE_INIT}}

        (
            Shared {
                {{SHARED_RESOURCE_VALUES}}
            },
            Local {
                {{LOCAL_RESOURCE_VALUES}}
            },
        )
    }

    {{FEATURE_TASKS}}
}
```

Every marker must be resolved before writing the candidate.

The rendered Rust source must be parsed with `syn` before compilation.

Do not use regex to discover Rust structure or insert code into arbitrary locations.

---

# 11. Iterative Assembly Algorithm

## 11.1 Initial validation

1. Read the hardware manifest.
2. Deserialize using strict structs.
3. Validate the schema version.
4. Validate Rust identifiers.
5. Validate feature order.
6. Load every feature bundle.
7. Verify every declared fragment exists.
8. Verify every required placeholder can be resolved.
9. Resolve explicit hardware claims.
10. Detect duplicate symbols.
11. Detect duplicate logical resources.
12. Detect duplicate pins.
13. Detect duplicate peripherals.
14. Detect duplicate interrupt bindings.
15. Validate supported backend values.
16. Calculate a complete input fingerprint.

Do not write a generated crate until this phase passes.

## 11.2 Empty application check

Generate the base application without optional features.

Run:

```text
cargo fmt --manifest-path <candidate>/Cargo.toml -- --check

cargo check \
  --manifest-path <candidate>/Cargo.toml \
  --target thumbv7em-none-eabihf \
  --locked
```

The empty check validates the target, linker setup, RTIC, HAL, PAC, runtime, and panic implementation independently of the feature.

Promote the successful empty application as the initial working checkpoint.

## 11.3 Feature insertion

For each feature in `feature_order`:

1. Render a complete standalone candidate crate in a temporary directory.
2. Include all previously successful features.
3. Add the next feature.
4. Verify that no placeholders remain.
5. Parse all generated Rust files using `syn`.
6. Run `cargo fmt`.
7. Run `cargo fmt --check`.
8. Run embedded `cargo check --locked`.
9. Capture stdout, stderr, exit status, and command line.

On success:

1. Create a backup of the current working directory.
2. Rename the validated candidate to `working`.
3. Update the checkpoint state.
4. Remove the backup only after promotion succeeds.
5. Continue to the next feature.

On failure:

1. Leave `working` unchanged.
2. Preserve the candidate under `failed/<feature-name>/`.
3. Save compiler diagnostics and resolved manifest data.
4. Record that compilation failed after the insertion.
5. Stop in fail-fast mode.
6. Print a resume command.

The promotion implementation must account for Windows directory and file-lock behavior. Do not assume Unix directory replacement semantics.

## 11.4 Final build

After all features pass:

```text
cargo build \
  --manifest-path generated/nucleo-f401re-blinky/working/Cargo.toml \
  --target thumbv7em-none-eabihf \
  --release \
  --locked
```

A successful sequence of `cargo check` operations is not sufficient. The final linked binary must build.

Record:

- Binary path
- Binary size
- Manifest fingerprint
- Feature fingerprint
- Toolchain version
- Builder version

---

# 12. Build State and Resume

Suggested state:

```toml
schema_version = 1
generator_version = "0.1.0"
backend_version = "0.1.0"
toolchain = "..."
cargo_lock_hash = "..."
template_hash = "..."
manifest_hash = "..."
ordered_feature_hash = "..."

successful_features = []
next_feature_index = 0
failed_after_inserting = ""
```

Each successful checkpoint must fingerprint:

- Hardware/application manifest
- Ordered feature list
- All successful feature metadata and fragments
- Base templates
- Cargo manifests
- `Cargo.lock`
- Rust toolchain
- Builder version
- Backend version

Resume is permitted only when all relevant fingerprints match and the working application still passes `cargo check`.

Otherwise, require complete regeneration.

For the MVP, regeneration is inexpensive. Resume exists primarily to preserve and inspect a failed intermediate state.

Commands:

```text
cargo xtask generate \
  --manifest hardware/nucleo-f401re-blinky.toml

cargo xtask generate \
  --manifest hardware/nucleo-f401re-blinky.toml \
  --resume

cargo xtask clean \
  --application nucleo-f401re-blinky
```

---

# 13. User Feedback

Successful output should resemble:

```text
Loading manifest: nucleo-f401re-blinky
Validating platform configuration ........ PASS
Validating feature blink_led .............. PASS
Checking empty RTIC application ........... PASS

[1/1] Inserting blink_led
Rendering candidate ....................... PASS
Parsing generated Rust .................... PASS
Formatting candidate ...................... PASS
Running cargo check ....................... PASS
Promoting candidate ....................... PASS

Running final release build ............... PASS

Generated application:
generated/nucleo-f401re-blinky/working

Binary:
generated/nucleo-f401re-blinky/working/target/...
```

Failure output:

```text
[1/1] Inserting blink_led
Rendering candidate ....................... PASS
Parsing generated Rust .................... PASS
Formatting candidate ...................... PASS
Running cargo check ....................... FAIL

Compilation failed after inserting:
blink_led

Last valid application:
generated/nucleo-f401re-blinky/working

Failed candidate:
generated/nucleo-f401re-blinky/failed/blink_led

Compiler diagnostics:
generated/nucleo-f401re-blinky/failed/blink_led/cargo-check.stderr
```

Do not hide Rust compiler diagnostics.

---

# 14. Validation Requirements

The MVP must reject:

- Missing fields
- Unknown fields
- Unsupported schema versions
- Unknown feature bundles
- Missing source fragments
- Unknown placeholders
- Unresolved placeholders
- Invalid Rust identifiers
- Invalid compact GPIO pin tokens
- Unsupported GPIO pins or default levels
- Zero toggle period
- Periods requiring silent rounding
- Invalid task priorities
- Duplicate task names
- Duplicate helper symbols
- Duplicate logical resource names
- Duplicate physical pin claims
- Duplicate peripheral claims
- Duplicate interrupt bindings
- Arbitrary Rust expressions supplied through the manifest
- Resume after input changes
- Stale or invalid working checkpoints

`cargo check` remains the authoritative validation for:

- HAL type correctness
- RTIC macro syntax
- PAC interrupt existence
- Ownership and borrowing
- Init return values
- Resource-list correctness
- Feature interactions

The final release build remains authoritative for:

- Final linking
- Linker-script correctness
- Required runtime symbols
- Final binary generation
- Memory-region overflow detected by the linker

---

# 15. Required Tests

## 15.1 Manifest tests

Verify:

- The valid NUCLEO manifest loads.
- Every required field is mandatory.
- Unknown fields are rejected.
- Invalid periods are rejected.
- Invalid priorities are rejected.
- Invalid resource names are rejected.
- Unsupported backend values fail explicitly.

## 15.2 Feature-loader tests

Verify:

- All required files are loaded.
- Missing fragments fail.
- Missing placeholders fail.
- Unknown placeholders fail.
- Declared and actual symbol/resource claims remain consistent.

## 15.3 Rendering tests

Verify:

- Every marker is replaced.
- No undeclared marker is accepted.
- Numeric values cannot inject Rust source.
- Identifier values are parsed as identifiers.
- Rendered Rust parses with `syn`.
- Output is deterministic.
- Repeated generation produces identical files.
- No timestamp appears in generated source.

## 15.4 Collision tests

Verify rejection of:

- Duplicate task names
- Duplicate helper functions
- Duplicate local resources
- Duplicate GPIO claims
- Duplicate timer claims
- Duplicate interrupt bindings
- Conflicting imports

## 15.5 Golden-output test

Store the expected output under:

```text
tests/golden/nucleo-f401re-blinky/
```

Generate in a temporary directory and compare every text file.

## 15.6 Compilation test

The generated application must pass:

```text
cargo check \
  --manifest-path <generated>/Cargo.toml \
  --target thumbv7em-none-eabihf \
  --locked
```

## 15.7 Final-build test

The generated application must pass:

```text
cargo build \
  --manifest-path <generated>/Cargo.toml \
  --target thumbv7em-none-eabihf \
  --release \
  --locked
```

## 15.8 Failure-recovery test

Create an intentionally broken `blink_led` feature.

Verify:

- Candidate compilation fails.
- Working application remains unchanged.
- Failed candidate is preserved.
- Compiler output is preserved.
- State identifies the latest insertion.
- Resume refuses changed inputs.
- Resume works when inputs are unchanged and the failure is corrected.

## 15.9 Hardware smoke test

Document and perform manually:

1. Build the generated binary.
2. Flash the NUCLEO-F401RE.
3. Confirm LD2 changes state every 500 ms.
4. Change only `toggle_period_ms`.
5. Regenerate and rebuild.
6. Confirm the behavior changes without manually editing generated Rust.

---

# 16. Codex Implementation Phases

## Phase 1: Workspace foundation

- Create the Cargo workspace.
- Add `xtask`.
- Pin the Rust toolchain.
- Pin dependencies and create `Cargo.lock`.
- Add the base RTIC template.
- Ensure `xtask` builds for the host rather than the embedded target.

Acceptance gate:

```text
cargo check -p xtask --locked
```

## Phase 2: Strict manifest model

- Implement schema structs.
- Deny unknown fields.
- Avoid defaults.
- Implement semantic validation.
- Add valid and invalid fixtures.

Acceptance gate:

```text
cargo test -p xtask manifest --locked
```

## Phase 3: Feature-bundle loader

- Load feature metadata.
- Load all source fragments.
- Verify placeholders and claims.
- Produce path-specific diagnostics.

Acceptance gate:

The `blink_led` bundle validates independently.

## Phase 4: STM32F4 translation backend

- Translate only declared values.
- Support the exact MVP subset.
- Reject all unsupported values.
- Document all fixed backend behavior.

Acceptance gate:

The declared PA5 and TIM2 configuration resolves into deterministic Rust tokens.

## Phase 5: Renderer and syntax validation

- Render the complete empty application.
- Render the complete application with `blink_led`.
- Reject unresolved placeholders.
- Parse generated Rust with `syn`.
- Produce deterministic output.

Acceptance gate:

Output matches the golden application.

## Phase 6: Incremental checker

- Generate standalone temporary candidates.
- Check the empty application.
- Insert and check `blink_led`.
- Capture command output.
- Promote successful candidates safely.

Acceptance gate:

The valid `blink_led` candidate becomes the working application.

## Phase 7: Failure and resume

- Implement fingerprints.
- Preserve failed candidates.
- Implement Windows-safe promotion and rollback.
- Refuse resume after relevant input changes.

Acceptance gate:

Failure-recovery integration tests pass.

## Phase 8: Final release build

- Run a mandatory final build after all features pass.
- Record binary location and build metadata.

Acceptance gate:

A linked release binary is generated.

## Phase 9: Development-board test

- Flash the NUCLEO-F401RE.
- Verify the declared toggle period.
- Verify manifest-only configuration changes.

Acceptance gate:

The LED toggles as declared without manually modifying generated source.

---

# 17. MVP Definition of Done

The MVP is complete only when:

- The manifest is exhaustive for the LED feature contract.
- The builder contains no hidden hardware selection.
- The STM32F4 backend translates declarations without allocating resources.
- `blink_led` is stored as handwritten RTIC/HAL feature fragments.
- The builder generates the application from scratch.
- PA5, its default level, TIM2, TIM2 interrupt, task priority, clock, and period come from the manifest; the blink LED electrical mode, pull, and speed come from the documented backend contract.
- The empty RTIC application passes `cargo check`.
- The candidate application is checked after feature insertion.
- Rendered Rust is parsed before compilation.
- Successful candidates are promoted without risking the previous working application.
- Failed candidates and compiler diagnostics are preserved.
- Resume is fingerprint-protected.
- Generated output is deterministic.
- The complete application passes `cargo check --locked`.
- The complete application passes `cargo build --release --locked`.
- The NUCLEO-F401RE LED toggles according to the manifest.

---

# 18. What the MVP Proves

The MVP proves:

- Strict manifest parsing
- Translation-only backend behavior
- Handwritten feature-fragment insertion
- Local-resource insertion
- Init-code insertion
- Hardware-task binding
- Incremental compile validation
- Deterministic regeneration
- Failure preservation
- Resume-state handling
- Final embedded linking

The MVP does not prove:

- Shared-resource composition
- Software task insertion
- Task spawning
- DMA ownership
- Static buffer construction
- Multiple interrupt interactions
- Priority-ceiling behavior
- Monotonic scheduling
- Dependency solving
- FerroWasp-scale feature composition

Those must be introduced progressively after the development-board MVP.

---

# 19. Post-MVP Expansion Path

Use the existing Foxeer F405 FerroWasp application as the golden hardware baseline.

Expand methodically:

1. Simple GPIO input/output
2. Periodic hardware timer
3. UART without DMA
4. UART RX DMA
5. UART TX DMA
6. SPI without DMA
7. SPI DMA
8. Static buffers and queues
9. Software tasks and spawning
10. Shared resources
11. IMU acquisition
12. RC input
13. Logging and telemetry
14. Actuator-output path
15. Safety and health-monitoring tasks

For each new feature:

1. Confirm the handwritten implementation in a normal RTIC application.
2. Extract it into a feature bundle.
3. Define an exhaustive manifest contract.
4. Validate it in isolation.
5. Validate it incrementally with prior features.
6. Add a regression fixture.
7. Compare generated behavior against the FerroWasp golden baseline.
8. Pass the relevant development-board or bench acceptance gate before progressing.

---

# 20. Explicit Non-Goals for the MVP

Do not implement:

- UART
- SPI
- DMA
- USB
- Multiple MCU families
- Multiple boards
- Automatic resource allocation
- Automatic pin selection
- Automatic DMA selection
- Automatic priority assignment
- Dependency solving
- Arbitrary Rust supplied through manifests
- Regex-based Rust editing
- Automatic extraction from arbitrary applications
- Full Foxeer F405 reproduction
- Flight-control behavior
- Actuator control
- Safety certification tooling

The first pass should remain deliberately small: a deterministic, exhaustively declared, incrementally validated blinking-LED application on a safe development board.
