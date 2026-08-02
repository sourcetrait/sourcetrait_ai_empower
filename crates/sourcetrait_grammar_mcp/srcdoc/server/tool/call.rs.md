# call.rs

## fn call
NO FILESYSTEM PATH IS RESOLVED FROM THE NAMEPATH. The traversal defense is the ident rules in
`Namepath::validate` plus the INDEX as the authority: callability is checked against
`.meta/rig.nuon` via `index_node` and function-name membership, and the template then drives
the module-qualified name - no filesystem path is composed from the namepath at all.

The rig's READ lock is held across the whole dispatch, so a concurrent `commit` cannot swap
the canonical tree out from under a running call.

`None` is passed for the cache body: call targets are NOT rerunnable, because the body is the
committed rig's own and the handle would point at something the agent does not own.

Arity is checked HERE rather than in the parser: `validate` happily returns a Rig or Module
namepath, and only this tool knows that a call requires a Function.
