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
