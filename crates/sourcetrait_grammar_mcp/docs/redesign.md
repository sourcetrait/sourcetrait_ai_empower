# RedesignGrammarMcp
> REIGN HUMAN

The MCP tools will reduce to:
- info() (shape unchanged)
- nu() (combined run and interact)
- renu() (formerly rerun, shape largely unchanged)
- rigged() (formerly call, shape changed)
- grimm() (new)

## NuTool (DefExecute and DefInteract)
The `execute` and `interact` tools will change form dramatically and will
be combined into a single tool: `def`.

The call will pass the entire `def` signature and how we actually perform
the call (as an execute (run) or as an interact) will be determined by
reading the AST first.

Tool usage will pass an `args` field as a NUON data in the form of either
a `record`, `table`, or `nothing` (nuon: null).

Argument records and tables must be strictly typed.

The `result` returned will also be strictly typed as record, table, or nothing.

Like `args`, `result` will return as NUON, rather than a JSON transformation of
it.

As before, we will quietly allow `any` (undocumented by the skill) for very
specific things, but never for the top-level record/table type.

The signature and body will be passed as a single `def` paramter,
reflecting the tool name. Nothing else is allowed at the top-level.

```nu
def execute [args: <ARGS_TYPEDEF>]: nothing -> <RESULT_TYPEDEF> {
<BODY>
}
```

```nu
def --env interact [args: <ARGS_TYPEDEF>]: nothing -> <RESULT_TYPEDEF> {
<BODY>
}
```

The tool parameters for each will then simply be:
- args: string (valid NUON)
- def: string (valid Nu, constrained by an internal AST check)

Note that the `timeout` field is no longer present. We have Channel and we have
jobs. We will now have a default timeout (30s) and if the execution exceeds
30s will will:
- If Channel is open, push the execution into the background and notify Channel
  on completion.
- If Channel is not open, error and advise(warning) to use a job.

## RiggedTool

The `rigged` tool, renamed from `call`, will change shape to conform to
InlineDef and its constraints:
- `args` will now be passed as NUON.
- `args` and `result` can be typed as record, table, or nothing

## GrimmScratch (plugin)

This is serving as a prototype for how we want to document plugin commands
in general.

```nu
# Creates a unique temporary file
#
# Guarantees unique filename: `random uuid | grimoire nom | append '.' $ext | str join`
@example 'grimm scratch shm r#"This is data.\nThis is more data."' --returns /dev/shm/box/ai/myai/j4azJxladj/mNa35HuiozO.md
@example 'grimm scratch tmp md' --returns /home/box/tmp/ai/myai/j4azJxladj/mNa35HuiozO.md
def 'grimm scratch' [
  kind: string@[shm tmp] # Directory to save to; EQUIP_SHM_DIR | EQUIP_TMP_DIR
  ext: string # File extension, not including the '.'
  content?: string # Fill file contents with
]: nothing -> path
```

## Grimm

The `grimm` tool will do as it implies: run a specific grimm embedded
definition.

Here, instead of `args` we pass `params`, which implies a different
constraint: exactly what the `grimm` command expects as data.

Params look pass as an arguments list:
```nu
grimm scratch shm md r#"This is some data"
# equivalent: `params` = "[shm md r#\"This is some data\"]"
```

## ToolsToolsTools

Our tools will be converted into the embedded `grimm` plugin, dramatically
reducing the tool coverage and taking full advantage of the embedded system.

Any tool that is administrative (grimm control) in nature will be restricted to `interact`
by simply only scoping it there.

`grimm` will take on a more modular interface:
- `grimm info` (matches tool call)
- `grimm rerun` (matches tool call)
- `grimm scratch` (matches tool call)
- `grimm inspect ...` (formerly inspect tool)
- `grimm channel ...`
  - `grimm channel send`
  - `grimm channel remote send`
  - `grimm channel remote send_with`
- `grimm control ...` (interact only)
  - `grimm control channel open`
  - `grimm control channel close`
  - `grimm control channel remote open`
  - `grimm control channel remote close`
  - `grimm control channel remote list`
  - `grimm control purview ...`
  - `grimm control rig ...`
  - `grimm control rig install`
  - `grimm control rig uninstall`
  - `grimm control rig commit`

  ## SkillFiction

  The 'grammar' skill will encompass:
  - Grammar MCP
  - grimm plugin signatures
  - grimoire plugin signatures

  It will no longer need to explain how to use Nu correctly; the 'nugap' skill
  exists for that, which is a pre-requisite for the 'grammar' skill.

  The new grammar skill template will be human reign and will use partial
  liquid templating heavily. At the very end there will be a section a partial
  for ai reign, that will allow new features that have been generated to get
  a first pass at documentation before it is rewritten into human reign at
  a later date. In a perfect world, there would no an empty 'ai reign' partial
  as the entire skill would have been merged by the user.

  For plugin signatures, we will use nu's own help system to derive the data
  to be used in a nucmd.liquid template. Thus, idiomatically documenting
  signatures will be extremely important, as they will be directly rendered
  into the skill document's final product.

  Documentation of plugin command signatures will be slowly taken by human
  reign (marked in the docblock).

  Likewise, we will send tool call signatures to templates as well, as the
  `nu --mcp` crate does (an .md for each call). Those will slowly be taken
  human reign as well.


  
