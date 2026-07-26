# cycle.rs

## const CYCLE_REPEATS
A legitimate tree registers each file ONCE, so three is already impossible without
resolution doubling back on itself.

## fn detect_import_cycle
nushell HAS a circular-import detector - `FileStack::push` - and it does not fire
here. It compares raw `PathBuf`s, and an interior `..` is never folded away,
because folding it lexically is unsound the moment a symlink is involved: `a/..` is
the parent of the TARGET, not of the link. So a module reaching back into the
cascade it lives under presents a textually new path on every hop
(`i/../i/../i/...`), the check never matches, and resolution runs away until the
accumulated path stops resolving. A `.`-based self-import IS caught, because
`Path::components()` does normalize `.`. That boundary is the whole bug.

WHY THIS KEYS ON NEITHER PARSE ERROR: the error that finally surfaces is not
stable. A bare cycle reports `ModuleNotFound("../mod.nu")`, while the same cycle
carrying a couple of `export def`s reports `ModuleMissingModNuFile` with a
several-thousand-character path. Matching either one silently misses the other.

What IS invariant is the RUNAWAY itself - the same file registered over and over
under different spellings. Canonicalizing collapses them, and we can do that
soundly where nushell cannot, because we ask the FILESYSTEM rather than folding
text.

`baseline` is `num_files()` taken BEFORE the parse, so only this parse's own
registrations count. Without it the interact lane, whose working set accumulates
across calls, would eventually look like a cycle on its own.

A synthetic name - the submitted body - canonicalizes to nothing and drops out,
which is what leaves only real files on disk to be counted.

The sort is most-repeated first, then by path, so one cycle always renders the same
way. The message carries the FILES ALONE: the kind (`module::circular_import`)
already says what happened, so restating it would only put prose on the wire, and
`<a> uses <b>` is also nushell's own phrasing for this condition where its detector
does fire.
