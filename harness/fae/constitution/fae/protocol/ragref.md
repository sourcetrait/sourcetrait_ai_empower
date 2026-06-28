## protocol: ragref

The *Retrieval-Augmented-Generation Reference* (`ragref`) format standardizes
how cross-knowledgebase referencing in prose is rendered in a more concise
manner than the default Wikilinks usage.

Example:
```md
# Title of Document
## ref
- {adhoc:info:development} {implied:protocol:ragref} {adhoc:info:development:rust} {adhoc:rule:subagent::kp:markdown}
- {mem:something_odd} {mem:another_something} 
- `./docs/user/git.md` {git_user_help}

This is some content pertaining to `{implied:protocol:ragref}` discussing `{git_user_help}`.
```

Each item is either:
- Harness memory: "{implied:law:gating} {adhoc:rule:nu}", space separated, all on one line.
- Generic memory: "{mem:file_stem} {mem:something_stem}", space separated, all on one line.
- Filesystem paths: "`path` {alias}", one per line.

In-file granularity (backed by sub-headers in content) is delimited by
further `:` separators between tokens. Sharded files are delimited by a `::`,
corresponding to `__` in the filename.

If three or more in-file granularities can be conceptually categorized cleanly
under a short parent category token, then that category should exist as its own
parent granularity.

Additionally, an item in a journal, state file, or historical document can use:
- Journal pointer: "{journal:<topic>:<rev>} {journal:nushell_mcp:14}", space separated, all on one line.

A `ref` sub-header is attached tightly to its intended parent header indicating
its context of usage. No newlines between the parent and the ref heading.

Do not include any other information ragref section.

Each item: No hard-wrapping (as an exception to `{law:style}`)

If a `ref` is attached to a sub-section, not an h1, it must indicate the
section as part of the header. Eg: `### ref: REV 12`.

Ragrefs must start any iter, memory, or state document that needs to
cross-reference in prose; using it's `{...}` reference.

### protocol: ragref_memory

The live `MEMORY.md` index must start with a ragref consisting of *every*
implied/adhoc memory available on the first bullet.

It does not include granularities, but it does include shards.

No prose anywhere.

After the ragref section, any memory that should be expanded upon via word/phrase
association is done so using only keywords and key-phrases, comma separated.

Example:
```md
{implied:law:gating} gate, go, action, review, review means review

{adhoc:info:development} git, rust, lib.rs manifest
```

### protocol: infer_value

In the same style as ragref items, `{infer:some_snake}` is used to indicate that
the agent must use inferrence to determine the correct value in a portable and
dynamic way. A registry is not maintained for these; they are fuzzy by design.

A sub-category of value inferrence is the `{env:VARIABLE_NAME}` reference. It 
represents inferrence of OS environment variables. Some of these values can be
determined via `{info:environment}` while others will need to be queried for.
Values are not expected to change during a session.

Among various examples, inferrence values are used in `{info:layout_dir_alias}`
paths.
