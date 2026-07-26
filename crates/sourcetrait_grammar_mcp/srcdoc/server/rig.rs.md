# rig.rs

The rig substrate is a full nu ECOSYSTEM, not merely a `call()` registry. Call targets are
the indexed "bins", but EVERY export - call target or plain helper - is importable from a
body and from external nu scripting.

## const META_FILE
This file is ALSO the rig-detection marker: an `<author>/<name>` subdir under `rig/`
carrying it IS a rig. That is why changing its NAME forced a migration rather than making
one optional.

## const LEGACY_META_FILE
The index was the last JSON we persisted, against the house rule that anything persisted is
NUON. It exists only so `migrate_meta_to_nuon` can find a namespace written by an older
host and retire it.

## fn is_valid_rig
EXACTLY `<author>/<name>`: two parts, each a valid ident, neither `main`. A BARE rig with no
slash is REJECTED - a hard cutover with no default-author back-compat, so there is one
spelling of a rig everywhere.

## struct RigLocks
The outer mutex guards only MAP MUTATION; the per-rig `RwLock` is what serializes work.
Call takes a READ lock, and commit / new / install / uninstall take WRITE, so different rigs
are fully parallel.

The rigs git repo has no host-side lock of its own - git's index locking serializes the
subprocess commits - and namespace isolation per `(id, namespace)` means there is no
cross-process race to guard either.

### fn hydrate_from_disk
Pre-creates a lock per registered rig at startup, keying on the meta file's presence, so a
`lookup` during serving never has to decide whether a directory is a rig.

## fn ensure_keypair
`allowed_signers` is rewritten from the current pubkey on EVERY startup rather than only at
creation, so it cannot drift out of step with the key it authorizes.

## fn ensure_rigs_repo
The empty init commit exists so HEAD is valid immediately - a fresh repo with no commit
makes the first `git status --porcelain` comparison meaningless.

`configure_repo` runs on every startup, not just creation, so a moved keypair path
re-applies rather than leaving a repo that silently cannot sign.

## fn configure_repo
PER-REPO configuration only. Nothing here leaks into the user's global git config, and
`commit.gpgSign` being set locally is what makes every lifecycle commit signed implicitly
rather than by remembering a flag.

## fn establish_rig
Writes an establish-time index recording `source_dir` BEFORE any validation, because that
recorded path is what every later `commit` re-reads and what `check_source_dir`
cross-checks. The rig exists as an addressable empty thing first, and gains content on its
first commit.

## fn scaffold_leaf
EMITS THE CASCADE CONVENTION: a call gets BOTH `export module` and `export use`, while a
module-path segment gets `export module` alone. That is nushell's own prescription from the
0.114.0 release notes (PR #18303, "Submodules are no longer implicitly imported"), not a
house rule - importing a module no longer implicitly imports its exported submodules, and
the documented fix is to keep `export module` and ADD `export use`.

Verified against 0.114.1: with the inline `export module sub` alone, `use foo` then
`foo sub baz` fails; adding `export use sub` restores it.

The LEAF-GUARD refuses an existing terminal namepath, so scaffolding can never write over
authored work. Intermediate module dirs are additively wired and may already exist.

## fn read_summary
A summary is single-line in EFFECT but is hard-wrapped in SOURCE like any other comment, so
the stored text carries the author's line breaks and the wrap has to come back out.

THE SIGNATURE BLOCK STRUCTURALLY REQUIRES IT: an unflattened continuation lands at column
0, where the block's own grammar reads it as an AUTHOR line. That is how a real summary
broke the block while every test fixture passed, because every fixture had a single-line
summary.

Normalizing HERE rather than at each consumer is what keeps the block, the standalone
signature and inspect's `summary` fields agreeing. `details` is prose and keeps its
newlines, which is why it stays on `read_doc`.

## fn registered_rig_names
The plain sort of the compound `<author>/<name>` gives author-then-name ordering for free,
since the author is the leading segment.

## fn with_summary
Shared by the block and the standalone form, so a summary is attached the same way wherever
a signature appears. The ` # ...` is omitted ENTIRELY when undocumented rather than left as
an empty comment.

## fn signature_of
THE TWO MODES OVER ONE IMPLEMENTATION. The TREE form passes the LEAF name, because the
indentation around it supplies the hierarchy; a STANDALONE inspect passes the FULL
NAMEPATH, because nothing around it does. Everything after that first token is identical,
which is the point - the two surfaces cannot drift.

A render failure logs and substitutes `<?>` rather than failing the whole block, so one bad
schema cannot make `info()` unusable.

## fn push_within_signatures
CALLS BEFORE SUBMODULES: the callables at a level are what a reader scans for, and a
submodule pushes the eye deeper. Each group sorts by name.

A module line is emitted when the module's OWN namepath is covered OR when anything below
it was. That second case is what keeps the ANCESTORS of a deep match present, so the
indentation still spells a whole namepath - a subtree without its ancestors cannot be turned
back into one, and the block's entire grammar is its indentation.

A PER-SIGNATURE PREDICATE rather than a rooted walk, because a purview is a SET of patterns:
two of them can root in different rigs at different depths, which no single root expresses,
while "does any of them cover this signature" composes for free. That predicate subsumed an
earlier rooted implementation, producing identical output for every single-pattern case, so
the rooted version was deleted rather than kept beside it.

The buffering is not stylistic: whether a module's own line belongs in the block is not
knowable until its subtree has been walked.

## fn render_signatures_within
THE ONE RENDERER. `info()` shows the current purview through it, a pattern `inspect()` shows
one pattern through it, and the whole namespace is just the `*` pattern - so none of the
three can drift. It replaced the structured `rigs` tree `info()` used to return, which was
roughly 8 KB of nested JSON for two rigs and is the agent's FIRST read after the skill.

NOTHING IS STATED THAT CAN BE INFERRED, and the separators are inferable from shape alone:
depth 0 is an author and depth 1 a rig - a rig is ALWAYS the two levels
`<author>/<name>` - everything deeper is a module UNLESS it carries the two signature
groups, which makes it a call. So a reader joins author to rig with `/`, module segments
with `/`, and puts `:` before the first module and before the call.

TRAILING HIERARCHY CHARACTERS WERE TRIED AND REMOVED, and the reasoning generalizes. They
were introduced so a line could state its own separator rather than have one inferred, and
they failed at exactly the MIXED MODULE, which no single character describes: `a/b:c/` plus
`:call` needs an override rule, and an override rule IS re-inference wearing a costume.
Shape settles every case including that one, because a call is recognizable on its own.

Two further arguments landed with the removal. REUSE: a new kind extends the format by
taking a new SHAPE - a helper would be `name [...]` beside a call's `name <> <>` - rather
than by carving another character out of a two-character vocabulary that is already
overloaded. Separators do not scale; shapes do. And this block is the PROTOTYPE for a
general nu-tree renderer, which argues for keeping it general rather than info()-shaped.

A rig appears when its OWN namepath is covered or when anything inside it is, which is what
lets a bare `author/` list a rig with no calls yet while a pattern naming a module the rig
does not have leaves no orphan heading behind.

PATTERNS MATCHING NOTHING RENDER EMPTY rather than erroring. A pattern is a filter, and an
unresolved `.` matches nothing BY DESIGN, so an empty block is already this format's answer
for "nothing here" - a fresh namespace renders empty for the same reason.

A rig whose index fails to decode SKIPS with a host-stderr note rather than failing the
call, so one corrupt rig cannot take the surface down.

## struct CallDoc
There is NO `summary` field, and its absence is deliberate: SUMMARY IS PART OF THE
SIGNATURE, carried as the trailing ` # ...` exactly as the block does it.

## enum InspectDoc
UNTAGGED, so each member serializes as its own bare shape and schemars renders the set as a
`oneOf`. The caller always knows which member it will get, because it knows whether it
passed a pattern or an exact namepath.

It rides under the envelope's single `doc` field rather than at the root because a
root-level oneOf carries no `type: "object"` and Claude Code rejects it.

The four shapes stay distinguishable by FIELD SET alone, which is what an untagged oneOf
needs: `srcdir` marks a rig, `src` plus `summary` a module, `src` plus `signature` a call,
and a lone `signatures` a pattern.

## fn inspect_impl
Every path composes from the NAMEPATH against the COMMITTED canonical tree, so nothing here
reads `source_path`. One consequence worth holding: with the structured tree gone,
`source_path` is no longer surfaced to the agent at all. It stays load-bearing INTERNALLY -
`commit` re-reads it and `check_source_dir` cross-checks it - so the index field stays; only
the agent-facing exposure ended.

## fn cap_diagnostics
Caps each SEVERITY independently, so a flood of warnings cannot crowd out the errors that
actually block a commit.

## fn extract_doc
The first-blank-line split is nushell's OWN convention for a doc comment - the parser's
`build_desc` does the same thing to produce `description` and `extra_description` - so a
mod.nu leading comment and a `main` doc comment divide identically.

## fn check_summary_length
A WARNING rather than an error, so an over-long summary COMMITS and then surfaces in
`rig(check)`. Only Error-severity rows block.

## const NAME_DENYLIST
These names would collide with conventional directory meanings inside a namespace, so they
are refused at establish and scaffold rather than being allowed to produce a confusing tree.

## struct SelfView
A temp `<tmp>/rig/<author>/<rig>` symlink to the source, prepended to `NU_LIB_DIRS`, so a
rig's OWN rig-prefixed `use` resolves against itself during its FIRST commit - before it has
been placed in the namespace at all.

This REPLACED a retired inline self-reference form, which failed at serve: a call target's
`main` parses DURING the rig's own load, before the rig finishes registering, so the bare
self-reference resolved to nothing and surfaced as "External command failed".

The sequence number is what keeps two concurrent validations from sharing a view directory.

## fn validate_rig_source
Reports ALL findings AND builds the index and docs in the SAME walk, so a commit never
parses the tree twice.

`ValidationResult::is_empty()` means "no ERROR-severity diagnostics" - errors gate a commit,
warnings advise.

## fn validate_walk
A CALL IS AN EDGE MODULE: the walk validates it in place and never recurses into it, which
is what makes `rig::call_leaf` a rule rather than an accident.

Each child is tested against the edges NAMING it, and each check asks for the PRESENCE of a
required kind rather than keying on whichever edge was written first - a child may legally
carry more than one edge, which is exactly the case the convention prescribes.

THE VALIDATOR IS LOOSER THAN THE CONVENTION, deliberately for now: it requires an
`export use` edge on a call and SOME edge on a pure module, but does not require
`export module` on a call nor reject `export use` on a pure module. So a hand-authored call
wired `export use`-only still commits. The namespace is now fully conformant, so tightening
no longer invalidates anything - the ORDER was the whole constraint, since an enforcement
change that turns valid artifacts into violations cannot precede the sweep that makes them
valid.

## fn parse_module_has_main
Classification by nushell's OWN `Module.main` sentinel rather than by grepping for
`export def main`, so the question "is this a call target" is answered by the parser that
will actually run it.

## fn extract_module_edges
A parse failure yields NO edges rather than an error, because the cascade validator reports
that parse error separately - returning edges from a broken parse would produce a second,
misleading orphan diagnostic on top of the real one.

## fn validate_flat_file
The cycle check runs BEFORE reporting parse errors, for the same reason it does in the eval
path: a cycle's own parse errors are unstable in variant and payload and useless in every
form.

## fn scan_reserved_terms
`main` may appear ONLY as a call target's exported sentinel. The scan checks path components
first, then walks `flatten_block` FlatShape tokens.

Why those two shapes and no others: `VarDecl` catches let/mut/const names and `String`
catches def/module/alias names, record keys, cell-path members and barewords - EXCEPT a
`String` immediately after an `export def` InternalCall token, which is the valid sentinel
export. Quoted string VALUES keep their quotes in the token so they never match, and command
references parse as InternalCall or External so they skip. That leaves exactly
identifier-position `main` outside the sentinel.

All three parse sites register the file's ABSOLUTE path as the parse fname AND push it onto
`working_set.files` before parsing. The PUSH is the load-bearing half: `path self` reads
`working_set.files.top()`, not the parse fname - the fname only feeds span mapping - so
without the push a parse-time `const SELF = (path self)` const-resolves at serve and fails
at validate. This mirrors nushell's own module-file loader.

## fn validate_mod_nu_ast
A resolved private `use` or `overlay use` parses to `Expr::ImportPattern` or `Expr::Overlay`
rather than a Call, so it is allowed BY SHAPE rather than by name - which is why those two
arms exist beside the decl-name allowlist.

`let` at module body level is a nu parse error and surfaces before this structural walk ever
runs.

## fn validate_function_file_ast
SCHEMAS ARE READ FROM SOURCE TEXT on both sides, and both parsed forms are lossy in
different ways. `SyntaxShape` Display renders an empty-field record as the bare word
`record`, which is not valid arg syntax, while the parsed output `Type` collapses path,
directory and glob to string. Structure is still VALIDATED against the parsed signature -
positional shape and output Type - so the text is used for the schema and the parse for the
contract.

## fn check_args_record_positional
An empty `record<>` is the UNFLESHED SKELETON and is rejected, which is what stops a
scaffolded stub from committing as if it were finished. `Type::Nothing` is the legitimate
void.

## fn commit_impl
VALIDATES, never regenerates - author content and docs survive a commit untouched.

The canonical subtree is WIPED before the copy, and since `copy_dir_recursive` skips
dotfiles the prior `.meta/` goes with the wipe and is rebuilt from this validation rather
than merged with the last one.

An empty porcelain means idempotent no-op: grouped-but-empty result and NO commit, so
re-committing an unchanged rig does not accumulate history.

## fn install_impl
ATOMIC by rollback: a validation failure wipes the freshly-built canonical and commits the
removal, so nothing stays half-registered. That matters because `establish_rig` has already
created the subtree by the time validation runs.

## fn copy_rig_tree
Mirrors the validator's carried set exactly. Root `.assets/` and `.docs/` copy VERBATIM
including dotfiles, while everything else skips dotfile entries at every level - which is
what keeps a client `.git` out of the signed repo.

`.gitignore` is honored only by commit's `git add` at the STAGING step, so it governs what
enters the repo and is NOT a validation exemption: the walk covers the full filesystem, so
an ignored-but-invalid file still blocks the commit.

## fn migrate_meta_to_nuon
CHANGING THE FILENAME IS WHAT MADE THIS MANDATORY rather than optional, because DETECTION
keys on the meta file: an unmigrated namespace would not error, it would simply report no
rigs and answer `not_registered` to every call.

Idempotent. A namespace with no legacy file is a no-op, and a rig somehow carrying both
keeps the `.nuon` it already has and drops the stale `.json`.

## fn ensure_substrate
The migration runs BEFORE hydration, or an unmigrated namespace hydrates as empty.

A failure to CONVERT is fatal to startup, on the reasoning that a half-migrated namespace
serving an empty rig set is the worse outcome. A failure to COMMIT the conversion is only
logged, since the files on disk are already correct and the next commit sweeps the repo
clean.

`ensure_default_purview` runs AFTER hydration so the prune inside it judges against the real
registered set, and it is non-fatal: purview bookkeeping must not stop the host from
serving.
