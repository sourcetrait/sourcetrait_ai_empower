# inspect.rs

## struct InspectEnvelope
EXACTLY ONE FIELD. The namepath is not reprinted because the caller supplied it, and the
per-kind shapes carry nothing in common worth hoisting beside it.

## fn inspect
CLASSIFICATION IS BY SHAPE AND HAPPENS FIRST. A trailing hierarchy character, or a bare `*`
or `.`, makes this a PATTERN; anything else validates as the exact namepath it always did,
with the exact parser's strictness unchanged.

`.` IS RESOLVED HERE, not in the parser, and that placement is the design: purview is the
only thing that knows what `.` names, so the parser leaves it unresolved and an unresolved
`.` matches nothing. Resolving it at this one call site is what makes `inspect(".")` render
the current view rather than an empty block.

A PATTERN SPANS RIGS, so the per-rig READ locks are taken inside the renderer rather than
around one lookup here. That is why the pattern arm returns early instead of falling through
to the lock below.
