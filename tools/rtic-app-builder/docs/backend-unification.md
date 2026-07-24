# Canonical backend unification boundary

The RTIC App Builder lives at `tools/rtic-app-builder` in the FerroWasp
monorepo. This note records the compatibility-first boundary for sharing
FerroWasp backend crates with the generated NUCLEO-F401RE examples.

## Adopted

- MSP v1 parsing, response generation, telemetry types, and protocol constants
  come directly from `crates/ferrowasp-mspv1`.
- The former `compat/ferrowasp-mspv1` source copy has been removed.
- The builder remains a nested Cargo workspace, so this dependency does not
  add the generated examples or their F401 HAL selection to the root workspace.

The serial/OSD adapter uses a repository-relative dependency path. Its
authoritative, buildable location is therefore
`tools/rtic-app-builder` inside FerroWasp. A standalone checkout remains useful
for builder-core development, but the OSD adapter and generated OSD firmware
must be built from the monorepo location.

## Intentionally deferred

No FerroWasp root workspace manifest, canonical shared crate, or handwritten
application is changed by this unification step.

- `ferrowasp-stm32f4`: its current ARM dependency selection is F405-specific.
  Adding it to the F401 example can activate conflicting HAL/PAC device
  features. Defer until the crate has an explicitly validated F401-compatible
  feature boundary.
- `ferrowasp-io-core`: its RX chunks require timestamps, completion causes,
  stream generations, discontinuity state, and UART-error metadata. The
  prototype's `SerialChunk` does not yet carry that contract. Defer until the
  generated endpoint produces the complete metadata rather than fabricating
  it or dropping it.
- `ferrowasp-tasks`: its OSD task is coupled to flight tuning and telemetry
  behavior that is not a drop-in match for the validation-only NUCLEO
  consumer. Defer until a protocol-only facade exists or the generated
  component can adopt the canonical behavior without changing its safety
  scope.
- `ferrowasp-core`: the validation example has no flight-domain need for it.
  Do not add a dependency solely for nominal backend uniformity.

## Gate for later phases

A deferred migration may proceed only when it:

1. leaves root workspace dependency selection and handwritten application
   source unchanged, or receives an explicit separate review for those
   changes;
2. preserves the NUCLEO architecture contract, including bounded channels,
   overflow reporting, task ownership, and validation-only arming semantics;
3. compiles and links both generated NUCLEO examples;
4. passes the root FerroWasp formatting, checking, linting, and test gates; and
5. records any HAL/PAC feature change explicitly before implementation.
