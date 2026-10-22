# Requirements Design: Tracing per Source File

Section 20 of [the requirements design](design.md), which maps every section
to its chapter.

## 20. Tracing per source file

This is the agreed design; `reqtool` does not implement it yet. Until it does,
evidence is fingerprinted as [section 4](evidence.md#4-fingerprints)
describes, and [remaining work](../remaining-work.md) carries the migration.
It replaces section 4's crate-closure fingerprints, the crate as the boundary
of evidence, `allocated_to`, mandatory linking of `pub` items, and the
verification modes of [section 5](evidence.md#5-verification-modes-and-timing).
Levels, gates, variants and computed status (sections 2 and 3) stand, as do
the priority and bus rules of [section 8](architecture.md#8-safety-critical-and-non-critical-code).
The requirements that make its rules checkable are `TRACE-001` to `TRACE-007`
in `tracing.yaml`.

**Three roles.**

| Role | What it is | Where it lives |
|---|---|---|
| Requirement | what shall hold | YAML under `project_meta/requirements/`, or a library's catalog |
| Module | a source file that implements requirements | the file declares which |
| Test | what verifies a requirement | independent of any one module |

A module states in its module doc comment which requirements it implements
and whether it is safety-critical:

```rust
//! Requirements: SAFE-001, SAFE-002
//! Safety-critical.
```

A crate is queried for its requirements by reading its files, the
declaration moves with the file, and readers of the code and of its rustdoc
see it first. Two modules that
implement the same requirements are interchangeable: either one passing the
requirements' tests is proof for that module.

**Tests are independent of implementations.** Where a requirement describes an
interface, its test is a generic function over a trait, and each
implementation's test calls it with itself. Where the shape varies between
implementations, the test takes a closure adapter that drives the
implementation. Either way the test is ordinary code, and the call from the
implementation's test module is the link.

**What a module's identity is.** A hash of the file's tokens, ignoring
whitespace and comments, doc comments included. Reformatting or documenting a
file leaves its evidence standing.

**What evidence covers.** Evidence for a module covers its own identity and
the identity of each thing it imports directly:

- another module: that module's identity;
- an external crate: its pinned version.

Nothing propagates past that one level. When a module that A imports
changes, A's evidence goes stale; what that module itself imports does not
matter to A, because the imported module's own evidence re-proves it. A
change to an import that only A uses re-owes only A. A module that imports a
third-party crate is that crate's verification boundary: bumping the version
re-owes the modules that import it, and nothing else.

**What an import is.** A `use` declaration or a path in the file, resolved to
the file that defines what it names. Where a path does not resolve to one
file, the resolution widens and never narrows: a glob import covers every file
the glob can reach, a macro the file that defines it, and a trait the files
holding its implementations for the types the module names. Imports behind a
`#[cfg]` count only in configurations that enable them.

**Safety-critical is per file.** Only safety-critical modules owe evidence:
the safety core does, the OSD does not. A safety-critical module imports only
other safety-critical modules and external crates pinned to one version
(`=x.y.z`, or a git `rev`), and declares at least one requirement. `reqtool`
checks both for each file, at the `analysis` level.

**Configurations.** A firmware image is a collection of module identities. A
configuration is identified by:

- the source files compiled into it, read from the compiler's dep-info;
- its Cargo feature set;
- the versions of its external crates;
- the Rust toolchain;
- its RTIC scheduling table: each task's priority and binding, with task
  names excluded, so renaming a task in the app shell changes nothing.

Timing evidence depends on scheduling as much as on code, which is why the
scheduling table is part of the identity (section 4 has the bench run that
showed it). Runtime variants, such as the RC protocol, the telemetry source,
serial bindings and the loop rate, are not part of a configuration's
identity: each evidence record lists the variant values it ran with, and
[the test plan](../testing/README.md) decides which variants a configuration
owes.

**Evidence records.** One record per run, holding what ran, the requirements
it claims, with their text, the identity of every module and the external
versions in the configuration it ran in, and the result. Editing a
requirement's statement or verification makes its evidence stale: the claim
changed, so the old run no longer proves it. A unit run counts for any
configuration where the module and its direct imports have the recorded
identities. An integration run counts for any configuration whose identities
all match those it recorded.

**Maturity per configuration.** For each configuration, `reqtool` reports
every requirement declared by its compiled-in safety-critical modules as
`verified`, `stale`, `failed` or `unverified`. A Cargo feature that pulls in
a module with no evidence shows that module's identity as unverified, so an
image built only from verified modules reports clean even when its crates
hold unverified ones. A release run reports maturity and does not block;
blocking is added per gate as coverage grows. During development the report
is run by hand.

**Requirement relations.** A requirement still derives from W1-W7 or from a
parent, and the tree is reported. Each requirement is verified by its own
evidence; a parent is not proved through its children, nor a child through
its parent's run.

**Catalogs.** A library ships a catalog of requirements and their tests with
each version. A project either maps its own requirements onto the library's,
or keeps its own and ignores the catalog. A project that imports another
project and keeps its requirements reruns that project's tests in its own
configuration.

**Crates.** A crate that exists only so its evidence survives changes to its
neighbours is folded into a module of a larger crate where that reads better.
A crate reused on its own stays a crate.

**Migration.** `allocated_to` becomes declarations in the files. Fingerprints
become identity sets, recomputed from git at each record's own revision, so
no evidence is lost; this is possible because identities are derived from
source, never written by hand. The requirements that exist to make mandatory
linking pass are reviewed and kept only where they state something testable.
