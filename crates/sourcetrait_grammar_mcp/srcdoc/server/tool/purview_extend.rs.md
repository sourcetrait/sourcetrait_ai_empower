# purview_extend.rs

## struct PurviewDeltaEnvelope
The two halves are ASYMMETRIC on purpose: extending normally only ADDS, so a `hidden` field
would report an empty list that reads like a result.

## fn purview_delta
`revealed` is `info()`'s block rendered for the patterns that were newly ADDED - NOT a diff of
two whole blocks. So when the view already carries `*`, what it shows was visible before as
well: the PATTERNS are what changed, and the block says what they name.

The delta is computed on EXPANDED values on both sides, so a `@ref` to something already in
view reveals nothing.

## fn purview_extend
AN UNKNOWN ID IS REFUSED rather than ignored. Contributing nothing in silence would turn a
typo into a view that simply fails to widen, which looks identical to a purview that is
genuinely empty.
