# namepath.rs

## const PATTERN_CURRENT
Parsed HERE so the grammar is complete, but RESOLVED by purview, which is the only
thing that knows what it names. That split is deliberate and has a consequence worth
holding: an unresolved `.` matches NOTHING rather than silently matching everything.

## enum NamepathStr
CLASSIFICATION IS NOT VALIDATION, and the distinction is the whole point of the split. A
bare author is exact in SHAPE but names nothing addressable, so it classifies as
`Namepath` here and then fails `validate` - the exact parser's strictness is entirely
unchanged by the pattern arm existing beside it.

### fn covers
A PATTERN covers a SET; an EXACT namepath covers only ITSELF. Purview values are allowed
to be either - a whole author, or one specific call - so answering the same question for
both is what lets a purview hold a mixed list without any caller branching on which kind
it got.

## enum NamepathPattern
`/` DESCENDS the tree and `:` selects the level BELOW, which is exactly what each
separator already means in an exact namepath. That is why the two module forms differ:
`lib:mod/` is that module's whole subtree, `lib:mod:` only its calls.

### fn parse
The single-segment-with-`/` arm rejects `author/rig/`: after a RIG the separator is `:`,
because everything past a rig is reached that way. Catching it here gives a legible
reason rather than an unhelpful "not a pattern".

### fn matches
THE PER-SIGNATURE PRIMITIVE: one namepath, asked whether this pattern covers it. The
renderer walks the index putting exactly that question to every signature, and purview
filtering is the same question over a SET of patterns - so `covers` wraps it for both and
there is one answer rather than two implementations.

`Current` returns false, which is the unresolved-`.`-matches-nothing rule made concrete.

## fn Namepath::validate
The `::` form - a root function - is rejected with its own message rather than falling
through to a generic module-path error, because it is a shape an author will actually
reach for and the reason it is banned is not guessable.

`main` is rejected as a function name here as well as in the validator, so a namepath
naming it cannot even be constructed.

## fn at_or_below
Compared on SEGMENT boundaries, so `a` covers `a/b` but never `ab`. A plain prefix test
would silently pull in a sibling whose name merely starts the same way.
