# App Builder V3

App Builder V3 is an isolated, compilable authoring model and renderer for
STM32F4 RTIC applications. It validates declarations and reusable task bodies,
expands hardware endpoints, resolves ownership and interrupts, and renders the
selected RTIC application.

The separately reviewed physical-actuator design and its still-mandatory
evidence gates are recorded in
[`ACTUATOR_ARCHITECTURE_REVIEW.md`](ACTUATOR_ARCHITECTURE_REVIEW.md).

The active source is organized by responsibility:

- `xtask/src/hardware_definitions/` contains reusable hardware types,
  STM32F4-specific hardware endpoints, and HAL-specific hardware tasks;
- `xtask/src/rtic/` contains the task, component, and application-composition
  declaration model;
- `xtask/src/tasks/` contains reusable HAL-agnostic task bodies;
- `xtask/src/target/board.rs` declares the selected board hardware;
- `xtask/src/target/app_composition.rs` selects components, task instances,
  resources, constants, priorities, spawns, and interrupt bindings.
- `xtask/src/target/platform_config.rs` assigns portable services to physical
  endpoint slots at boot. Endpoint declarations never embed service roles.
- `generated/src/main.rs` is the generated RTIC application and must not be
  edited directly.
- `generated/src/prelude.rs` contains the generated crate-local imports used by
  the RTIC application and must not be edited directly.
- `generated/src/platform_config.rs` contains the boot-time service assignments
  lowered from the selected target configuration and must not be edited directly.
- `generated/SAFETY_SPINE.md` is the deterministic task, resource, interrupt,
  priority, spawn, channel, safety-authority, and source-state-pinned golden-app
  semantic reconciliation report and must not be edited directly.

Generated endpoint resources and hardware tasks are named for their connector
(`serial1`, `serial2`, `spi1`). Boot initialization selects each connector's
profile and transfers its portable handles to service-named resources, so a
serial service can move between compatible serial endpoints without changing
the RTIC interrupt topology.

Board transport declarations use one `HardwareEndpointDeclaration` registry.
Serial, SPI IMU, ADC observation, SPI NOR, and USB CDC are typed variants of
that API; future transports should be added as further endpoint variants
instead of separate board fields.
Physical IMU installations are nested below their owning SPI endpoint with
`.with_imus(...)`. Each installation has a stable numeric ID and a validated
sensor-to-body orientation; boot configuration selects that ID when assigning
the IMU service to the endpoint. Common orthogonal mounting transforms are
available as `ImuOrientation::ROTATE_Z_90`, `ROTATE_Z_180`, `ROTATE_Z_270`, and
`UPSIDE_DOWN_X_FORWARD` so board declarations do not need raw axis arrays.
The SPI parser applies the selected installation rotation exactly once to
acceleration, floating-point gyro rates, and raw gyro values. Its exported
sample is therefore in the Forward-Right-Down physical body frame. A control
consumer must apply only the explicit `BODY_RATE_TO_RATE_CONTROLLER_MAP` to
body rates for the legacy controller convention; it must not repeat the board
rotation. Estimator gravity conversion separately negates the already rotated
body-frame specific force.

The selected board declares scheduling and initialization resources `tim2`,
`tim4`, `tim5`, and `tim6`, alongside the physical DShot, ESC-telemetry, ADC1,
SPI2 NOR, and OTG_FS resources
described below. The application binds `tim2` as its centralized 1 MHz RTIC
monotonic, so async
delays, IMU event timestamps, SPI transaction deadlines, and timeout recovery
share one wrap-extended clock. It binds `tim4` explicitly as the 800 Hz
periodic-control scheduler. The generated synchronous `TIM4` task runs at
priority 14, acknowledges every update, and reaches the 400 Hz control cadence
on every second tick. It exclusively drains the valid-SBUS and decoded-IMU
safety channels, retains bounded RC freshness state, and calls the reusable
HAL-independent Foxeer control step. The application separately binds `tim5`
as a general synchronous initialization resource; it is not owned by the SPI
endpoint. TIM5 can be retired from that role when sensor initialization moves
into an asynchronous state machine.

Persistent task-local values are declared through versioned
`TaskStateSchema`s and closed `TaskStateRecipe`s. The selected
`foxeer_control_state` version 2 schema requires the controller, rate filter,
angle estimator, body-to-controller map, gyro-bias calibrator, freshness and
cadence counters, sequence counters, retained RC input, and explicit RC-link
and arming-boundary state. Resolution
assigns every value exclusively to the generated `control_loop`; it rejects
missing or duplicate roles, incompatible recipes, unresolved or multiple
owners, and a cadence value that disagrees with the periodic-control timer.
The backend owns every Rust type and constructor expression. Application
metadata cannot inject initializer source.

Authoritative control-to-actuator data uses an explicit
`SafetyChannelDeclaration` in `AppComposition`. Each bounded channel has one
task-local producer and one task-local consumer owned by different tasks; both
owners must be marked `.safety_critical()`. Validation rejects missing,
duplicated, shared, noncritical, or type-incompatible handles before rendering.
The renderer allocates private `SafetyChannel` storage, splits it once in
`init`, and hands the non-cloneable handles to RTIC local resources. The
selected target uses these channels for safety-qualified SBUS snapshots and
decoded body-frame IMU samples entering the Foxeer control task. Another
channel carries observation-only IMU readiness, bias-calibration, and
freshness evidence from control into safety; this evidence cannot grant
actuator authority. Three further exclusive paths carry fresh safety-owned
actuator authority, pre-arm completion/abort reports, and serialized physical
service faults. The target also declares the golden-capacity
control-to-actuator path with three usable slots (`SafetyChannel<MotorCmd, 4>`),
owned solely by the control producer and a priority-15 safety-critical consumer
endpoint. A typed
`ActuatorCmd::ApplyLatestThrottle` spawn edge wakes that endpoint after a
successful queue insertion. The reusable control step preserves the golden RC
channel map, 16.4-count gyro conversion,
body-to-controller pitch compatibility, startup bias policy, 0.55 rate filter,
sequenced stored/menu tuning with reviewed fallback gains, Quad-X mixer, and
identity Foxeer motor order. It rejects
stale or non-finite input and out-of-range motor requests. Its publication
adapter constructs bounded, timestamped, sequenced `MotorCmd` values and
exposes explicit queue-full and wake-rejected results.

The generated priority-16 safety master exclusively owns the boot-routed SBUS
reader and discontinuity observer plus a versioned `FoxeerSafetyMaster` state.
Its bounded policy rejects parser errors, DMA and transport discontinuities,
frame-lost and failsafe flags, and a 100 ms receive timeout. Recovery requires
three healthy frames and a fresh arm-low observation before a held low-to-high
arm request can qualify. Arming guard order remains permit, link, switch,
throttle, IMU readiness, bias calibration, then IMU freshness; control supplies
only health observations and typed actuator requests.

The physical path is declared and compile-checked through existing shared
backends. It owns TIM1_CH1/PA8/DMA2 Stream1 Channel6,
TIM8_CH4/PC9/DMA2 Stream7 Channel7, TIM8_CH3/PC8/DMA2 Stream2 Channel0, and the
complementary TIM1_CH3N/PB15/DMA2 Stream6 Channel6 output. Four priority-16 DMA
handlers feed the all-lanes completion fence; a priority-13 service owns frame
cadence, lease expiry, timeout, and fault shutdown. Receive-only USART1 on PA10
uses DMA2 Stream5 Channel4 and exact priority-5 DMA/IDLE handlers. The bounded
priority-4 ESC manager preserves request identity and supplies fresh physical
output observations to the priority-15 sole command adapter.

This candidate nevertheless remains output-disabled by two independent gates:
the safety state is constructed with `FoxeerSafetyMaster::output_inhibited()`,
and the physical component's `output_enabled` configuration is false. In that
configuration the DShot service never starts even stop frames and the ESC
manager never requests telemetry; the backend-initialized pads remain low. The
compiled but unreachable preparation path preserves the 100 ms stop hold,
DShot command 65, and three fresh 3000–10000 eRPM samples from every physical
output before reporting qualification to safety. Missing or stale authority,
commands, telemetry, queue capacity, DMA completion, or leases force stop and a
safety fault. Compilation is not electrical, waveform, props-off, target, or
flight evidence; enabling either gate requires the separate review recorded in
the architecture document above.

The mandatory non-authoritative service suite is generated in the same graph.
ADC1 observes voltage on PC0/channel 10 and current on PC1/channel 11 through
DMA2 Stream4 Channel0 every 100 ms; MSP and USB suppress battery values once an
observation is older than 500 ms. The priority-3 UART4 MSP DisplayPort task
handles bounded receive/reply traffic, overlay telemetry, and a disarmed-only
tuning menu while the endpoint's priority-4 worker owns bounded TX completion.
The priority-1 SPI2 NOR owner on PB12/PB13/PC2/PC3 recovers append-only logs and
copy-on-write configuration, applies only whitelisted disarmed changes, and
verifies saved configuration before publication. OTG_FS on PA11/PA12 exposes
bounded CDC diagnostics and forwards only those storage commands; it has no
safety or actuator handles. A two-second heartbeat schedules status output,
and the priority-9 TIM6 8 kHz watchdog performs bounded SPI1 deadline recovery.
Control publishes rate records through a fixed-capacity queue and accounts for
overflow explicitly. These services do not change either disabled actuator
gate and are not target evidence.

From this directory, run:

```text
cargo check --workspace
cargo test --workspace
cargo run -p xtask -- check
cargo run -p xtask -- generate
```

The generated firmware package is isolated from the host-side workspace. With
the `thumbv7em-none-eabihf` target installed, check it using:

```text
(cd generated && cargo check)
```

For Rust completion and diagnostics, open `tools/app-builder-v3` in its own
VS Code window. Its local editor configuration loads only the host-side
`xtask` package rather than the repository's embedded firmware workspace.
