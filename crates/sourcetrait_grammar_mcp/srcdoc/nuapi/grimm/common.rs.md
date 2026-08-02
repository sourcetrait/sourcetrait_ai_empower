# common.rs

## struct NuapiCall
The log dir is a plain Rust value `eval_in_process` already holds. It is NOT in
engine state, NOT in `$env`, and nothing is seeded into the body's environment to
carry it - each decl is registered into the working set that eval is about to parse
with, so the dir rides ON THE DECL and the existing render/merge_delta carries it
at no extra cost.

A stateless eval builds its own working set, so its dir is isolated BY
CONSTRUCTION even under full concurrency. That is also why the dir rides on the
decl rather than in a cell on the base engine: the base is Arc-cloned into every
eval, so a cell there would be shared and raced.

The interact lane is serial and its engine persists, so re-registering SHADOWS the
previous decl each call - one decl of storage per call, the same bounded and
already-accepted growth as its `vars` table, cleared by a lane respawn.

### fn new
The origin falls out of the log dir's own path rather than being threaded through
the eval signatures, because the dir is NAMED for the eval's nonce. The nonce is
random enough that run, call and interact need no distinguishing prefix.

### fn origin
Host-stamped, and a body cannot forge it because it never supplies it.

### fn append_debug
`to nuon` renders compactly - no pretty-printing newlines between fields - but it
does NOT escape a newline INSIDE a string value; it emits the byte raw. Verified
rather than assumed: `{msg: "a\nb"}` comes back out of `to nuon` spanning two
lines, which breaks the one-record-per-line invariant a nuonl reader depends on.
So raw newline and carriage-return bytes are re-escaped here.

That substitution is safe precisely BECAUSE `data` is a record or table and the
render is compact: the only raw newlines the output can carry are inside
double-quoted strings, where `\n` and `\r` ARE the escapes nushell reads back, so
the line still parses to the original value. Backslashes `to nuon` already escaped
are untouched, since only the newline bytes themselves are replaced.

This is the shape of assumption that needs a live probe rather than a read of the
nushell source - the render's behaviour is not what the source suggests.

## fn require_record_or_table
The signature's `SyntaxShape` rejects a bad LITERAL at parse time, but a dynamic
argument (`grimm dbg $x`) reaches `run` unchecked - records are open and a list is
not shape-checked per element - so the guard has to live at run time too. One check
is genuinely not enough.

An empty list PASSES as a table, which matches the schema system's rule that `[]`
satisfies any table type.

## fn data_shape
Both members are column-OPEN: an empty `CollectionColumns` declares no required
columns, so any record or table binds.

## fn register_nuapi
MUST run before `nu::parse`. The parser resolves command names at parse time, so a
decl added afterwards is invisible to the body that needed it.

THIS ONE SITE IS THE ACCESS CONTROL. It is called from exactly one place, inside
`eval_in_process`, so the decls are absent from every engine that is not
mid-eval - not on the stateless base, not on the interact base, and not in the
validator's `ParseEngine`. Off-host nushell cannot resolve the names at all. That
is a stronger trap than a signing scheme, because there is no reachable entry
point to authenticate. A committed rig CAN call them,
but only because its call-target body runs inside an eval.

The config trio carries no per-call state - it reads process-global config and the
pin layer - so it takes no `NuapiCall`. It registers here anyway rather than on
the base, because this site is what keeps the whole family unreachable outside an
eval.

A CLEAN COMMIT PROVES NOTHING ABOUT RESOLUTION: `grimm ...` has its own command
head, so the validator parses it as an implicit external whether or not the decl
exists. Only an actual call is evidence that a committed body can reach the API.
