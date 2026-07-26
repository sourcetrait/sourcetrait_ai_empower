# purview.rs

## struct PurviewSetEnvelope
ONE field. The caller just SET the view, so it needs telling neither what is in view nor what
left - only what it can now see.

`revealed` being null does NOT mean nothing changed: NARROWING reveals nothing new, and
neither does naming a purview whose rigs were already visible under another id. `info()` is
the check for what is actually in view.

## fn purview
`@fae` and `fae` name the same purview, as they do for `info()`, so a reference copied out of
a configuration works unchanged.

AN UNKNOWN ID IS REFUSED rather than silently dropped. For a tool that REPLACES the view a
typo would otherwise narrow it to something the caller never asked for, which is
indistinguishable from the purview being empty.

BOTH SIDES OF THE DELTA ARE EXPANDED before comparison. Comparing raw values would count
`@ants` as new against a `sourcetrait/ant:` already in view, and re-reveal a block the caller
could already see. That was found by exercising rather than by the suite.

This tool is what retired `purview_reset`: resetting is just setting the view to nothing in
particular, which an empty list already means.
