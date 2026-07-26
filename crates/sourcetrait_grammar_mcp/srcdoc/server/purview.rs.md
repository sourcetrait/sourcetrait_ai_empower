# purview.rs

## const META_DIR
NEW with purviews. The namespace carried only `keypair/`, `rigs/` and `host.lock`
before this, so a reader of an older namespace will not find it.

## const PURVIEW_DEFAULT
THERE IS NO UNCONFIGURED DEFAULT, which is the invariant the whole subsystem rests on.
See `ensure_default_purview`.

## struct PurviewRow
`namepath_patterns` is the DESIGN's own column name, so the persisted table reads as it
was specified rather than as an implementation happened to name it.

## fn purviews_to_nuon
Through the serde Value bridge - the same one the rig index uses - so ONE bridge covers
the whole shape and the file cannot drift from the struct as it changes.

## fn load_purviews
A DECODE FAILURE IS A LOUD ERROR, never a silent empty. The rig index made the opposite
mistake once, where a missing meta file reads as "no rigs" rather than as an error, and
this file must not repeat that shape - an empty purview table resolves to "sees nothing"
while looking configured.

## fn is_valid_purview_id
Path-LIKE but never a path: a leading `/` or `./` is rejected outright, because a label
that looks like a filesystem path invites being read as one. A purview id is ARBITRARY,
unrelated to any namepath or file on disk.

## const PURVIEW_REF
`@` is unambiguous as a sigil because `is_valid_ident` never admits it, so no author,
rig, module or call can begin with one. That is what makes the reference form
non-colliding by construction rather than by convention.

## fn is_valid_purview_ref
`@.` and `@*` are REFUSED. Both are derived rather than stored, so they name no row and
can reference nothing - leaving them legal would mean deciding whether they resolve to
everything or to nothing, and neither answer is defensible.

## fn expand_values
REPORTS STAY RAW; only the FILTER path expands. That is why this is separate from
`resolve_patterns` rather than folded into it: a caller that DISPLAYS configuration
shows what was written, and a caller that MATCHES against it expands first.

CYCLES FLATTEN rather than lock up. A purview already visited on this walk contributes
nothing the second time, so `a -> @b -> @a` terminates with the union of both and
`a -> @a` terminates with a's own patterns. Writing a cycle is legal; it simply cannot
buy anything on the revisit. No configure-time rejection is possible anyway - `a -> @b`
is legal until someone later sets `b -> @a`.

## fn ensure_default_purview
Materializing `default` once at startup makes "default has a row" an INVARIANT rather
than a fallback every reader would have to remember, and everything downstream is
simpler for it: `resolve_patterns` has no absent-default arm, `is_nameable_purview`
special-cases only `*`, rig install just appends to the rows it finds, and rig removal
has no "is this namespace configured yet" question to ask.

It is also why `purview_configure("default", [])` RESETS to `['*']` rather than
deleting: `default` has no not-existing state, so the delete operation everywhere else
puts it back to what startup would have written.

## fn resolve_patterns
An id with no row contributes NOTHING rather than everything, because a typo must NARROW
the view rather than silently open it.

## fn parse_patterns
A stored pattern that no longer parses is treated as ABSENT rather than fatal: a purview
file is agent-authored data, and one bad row must not take `info()` down with it.

## fn pattern_requires
The `@id` check comes BEFORE parsing, because `@id` is not a namepath and must never
reach the namepath grammar at all.

## fn prune_dangling
Pruning is by REGISTRATION, not by emptiness: a rig with no calls yet still satisfies its
own pattern, and a purview pointing at it should survive until the rig actually goes
away.

The id set is taken BEFORE the mutable walk so a reference is judged against the WHOLE
table - a purview referencing one defined beside it survives, and so does a cycle, since
both ends have rows.

A row pruned down to NOTHING is dropped rather than kept as an empty purview, because an
empty pattern list is already the DELETE operation on the configure tool. A surviving
empty row would be a state the tool surface cannot otherwise produce.

## struct CurrentPurview
SESSION-RESIDENT by design - it is a VIEW rather than a configuration, so it belongs in
memory and dies with the host. Confirmed across a real restart, where the configured rows
persisted and the view reset.

### fn set
An EMPTY list means `default`, so the view always names at least one purview and there is
no looking-at-nothing state to reason about. That is also what subsumed the retired
`purview_reset`: resetting is just setting the view to nothing in particular.

### fn retain_known
So a rig uninstall or a purview deletion cannot leave the session pointing at something
gone.
