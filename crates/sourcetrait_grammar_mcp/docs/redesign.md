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
be combined into a single tool: `nu`.

The call will pass the entire `nu` signature and how we actually perform
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

The signature and body will be passed as a single `nu` paramter,
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
@category grimm
@search-terms 'grimm::tool'
@example 'write a shm file' {
    grimm scratch shm r#"This is data.\nThis is more data."
} --result '/dev/shm/box/ai/myai/j4azJxladj/mNa35HuiozO.md'
@example 'touch a tmp file' {
    grimm scratch tmp
} --result '/home/box/tmp/ai/myai/j4azJxladj/mNa35HuiozO.md'
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
- `grimm remote channel`
  - `grimm remote channel send`
  - `grimm remote channel send_with`
- `grimm control ...` (interact only)
  - `grimm control channel open`
  - `grimm control channel close`
  - `grimm control remote channel open`
  - `grimm control remote channel close`
  - `grimm control remote channel list`
  - `grimm control purview ...`
  - `grimm control rig ...`
  - `grimm control rig install`
  - `grimm control rig uninstall`
  - `grimm control rig commit`

### Nu Attributes for grimm
- @category Always 'grimm'
- @search-terms Always 'grimm::category::category::category'
- @example Always formatted as seen above. Results are not necessarily testable.

Grimm documentation can then be iterated through either by category or search-term.

Search terms are consts in code.
- grimm::utility
- grimm::tool
- grimm::control

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

## Nu Gaps

The following are gaps in 'nugap' that have been observed during review of this
document and are provided to close them.

### Attributes
Observe that the following is fully idiomatic and completely valid.
```nu
export alias "attr myattr" = echo

# This is a summary line.
#
# These are details.
# @notattr no workie
@category demo
@search-terms 'demo::subcategory'
@example 'shows shm' {
  demo_attrs shm 6
} --result { kind: shm, some: 6 }
@example 'shows tmp' {
  demo_attrs tmp 7
} --result { kind: tmp, some: 7 }
@myattr foo 8 'Something flies here' {k: 'keyed', v: 'valued'} [[field_a field_b]; [hey 1] [there 2]]
export def demo_attrs [
  kind: string@[shm tmp] # The kind
  some: int # The some
]: nothing -> record<kind: string, some: int> {
  { kind: $kind, some: $some }
}
```

The `help` nu command is user facing and not appropriate for parsing.
To programmaticaly parse a command:
- `scope commands | where name == 'demo demo_attrs' | first`
- `scope commands | where search-terms == 'demo::subcategory' | first`

The result of either, passed via '| to nuon', for the above nu command is:
```nuon
{
  name: "demo demo_attrs",
  category: demo,
  signatures: {
    nothing: [
      [parameter_name, parameter_type, syntax_shape,                      is_optional, short_flag, description, completion,                               parameter_default];
      [null,           input,          nothing,                           false,       null,       null,        null,                                     null],
      [kind,           positional,     string,                            false,       null,       "The kind",  [
          shm,
          tmp
        ], null],
      [some,           positional,     int,                               false,       null,       "The some",  null,                                     null],
      [null,           output,         "record<kind: string, some: int>", false,       null,       null,        null,                                     null]
    ]
  },
  description: "This is a summary line.",
  examples: [
    [description, example,            result];
    ["shows shm", "demo_attrs shm 6", {
        kind: shm,
        some: 6
      }],
    ["shows tmp", "demo_attrs tmp 7", {
        kind: tmp,
        some: 7
      }]
  ],
  attributes: [
    [name,   value];
    [myattr, [
        foo,
        8,
        "Something flies here",
        {
          k: keyed,
          v: valued
        },
        [
          [field_a, field_b];
          [hey,     1],
          [there,   2]
        ]
      ]]
  ],
  type: custom,
  is_sub: false,
  is_const: false,
  creates_scope: false,
  extra_description: "These are details.
@notattr no workie",
  search_terms: "demo::subcategory",
  complete: null,
  decl_id: 732
}
```
  
