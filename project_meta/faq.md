# FAQ

Why the obvious alternatives lost. An entry exists only where the
alternative is tempting enough that someone, or an agent, would propose it
again; the decision itself lives in the requirement or the spec section the
entry names, never here.

Every entry has the same shape, and `reqtool validate` checks it: a
"Why not …?" heading under a topic, one plain paragraph of at most 60 words
saying what was chosen and why the alternative lost, and a last line
`See:` naming the requirement or the document section that holds the
decision. No lists, tables, code, dates, PR or ADR numbers, names of who
decided, or history.

## Links between tasks

### Why not one shared request channel to the coordinator?

One queue per requester means a requester that floods fills only its own
queue, and the queue a request arrives on says who sent it. A shared channel
would let one requester crowd out the rest and would make the sender name
itself in the message.

See: [the coordinator](requirements/architecture.md#11-the-coordinator)

## Tracing requirements to code

### Why not trace evidence by crate closure?

A crate boundary is sound but coarse: keeping each piece of critical code's
evidence apart would need a crate per piece, multiplied across every
peripheral and chip family. Tracing per source file, with each file's direct
imports frozen by identity, keeps evidence as narrow as a crate split would,
without the crates.

See: [tracing per source file](requirements/tracing.md#20-tracing-per-source-file)

### Why not follow the call graph?

A call graph from rust-analyzer or rustc names exactly what each function
reaches, but it is heavy, ties the tool to compiler internals and breaks when
they change. Imports resolved to files are read from source alone and widen
where a path is unclear, so they never miss a change.

See: [tracing per source file](requirements/tracing.md#20-tracing-per-source-file)

### Why not list each module's requirements in a manifest?

A manifest beside the code drifts from it: a moved or renamed file leaves its
entry dangling, or pointing at the wrong code. A declaration inside the file
travels with it, and a safety-critical file that declares nothing is found by
the check.

See: TRACE-001

### Why not attach a requirement's tests to implementations with a macro?

A macro that stamps tests onto each implementation hides what runs, and ties
the tests to the macro's expansion. A generic test function over a trait, or
one taking a closure adapter, is ordinary code the implementation's tests
call, and that call is the link.

See: [tracing per source file](requirements/tracing.md#20-tracing-per-source-file)

### Why not let evidence depend on every transitive import?

A change deep in the tree would re-owe every module above it, which is the
crate closure again. A module's evidence covers its own code and the identity
of what it imports directly; each imported module's own evidence keeps
proving the requirements the importer relies on.

See: TRACE-004
