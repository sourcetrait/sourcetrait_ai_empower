SourceTrait AI Grammar: Nushell MCP
================================================================================
[![Crate Badge]][Crate] [![Docs Badge]][Docs] [![License Badge]][License] [![AI Badge]][AI]

*Nushell engine MCP server*


Usage
--------------------------------------------------------------------------------

The server documents itself. Its `learn()` tool writes a complete guide - the
tool surface, the type vocabulary, and the rig ecosystem - to
`<harness_dir>/skills/nu/SKILL.md`, stamped with the version that produced it.
Generate it against the server you are actually running, rather than reading a
copy that may have drifted.

`info()` is the live index of what a running server offers: its versions, its
loaded plugins, and every callable currently in view.


Installation
--------------------------------------------------------------------------------

Build from source and register the `grammar_mcp` binary as an MCP server over
stdio:

```nu
^cargo install --path .
```

The embedded nushell engine is pinned by the workspace; the binary reports the
version it was built against through `info()`.


AI
--------------------------------------------------------------------------------

This workspace is iteratively AI authored. It is essentialy tooling for AI, by
AI.


Repository
--------------------------------------------------------------------------------

Found a bug? Let us know! Upvote an existing issue on GitHub or create one if it
doesn't exist.

### Contributors
Contributors, please review [SOURCETRAIT.md](./SOURCETRAIT.md).  

#### Copyright Assignment Agreement (CAA)
By committing to this repository you agree to assign to
[Asmov LLC](https://asmov.software) all right, title, and interest worldwide in
all copyright covering your contribution.


License (AGPL3)
--------------------------------------------------------------------------------
SourceTrait AI Grammar Nushell MCP: Nushell engine MCP server  
Developed by [SourceTrait](https://sourcetrait.com), a division of **Asmov LLC**  
Copyright (C) 2026 [Asmov LLC](https://asmov.software)  

This program is free software: you can redistribute it and/or modify
it under the terms of the **GNU Affero General Public License** as
published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a [copy](./LICENSE-AGPL-3.txt) of the
GNU Affero General Public License along with this program.
If not, see [https://www.gnu.org/licenses/](https://www.gnu.org/licenses/).



[Docs Badge]: https://img.shields.io/badge/docs-blue
[License]: #License-AGPL3
[License Badge]: https://img.shields.io/badge/license-AGPL3-blue.svg
[AI]: #AI
[AI Badge]: https://img.shields.io/badge/ai-authored-green.svg

[Crate]: https://crates.io/crates/sourcetrait_grammar_mcp
[Crate Badge]: https://img.shields.io/crates/v/sourcetrait_grammar_mcp.svg
[Docs]: https://docs.rs/sourcetrait_grammar_mcp
