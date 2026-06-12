SourceTrait Empower: Nu Plugin
================================================================================
[![License Badge]][License]

*Nushell plugin supporting SourceTrait Empower*

Commands:
- [peek](#peek)


Usage
--------------------------------------------------------------------------------

### `peek`

Structural queries against markdown files.

#### `peek md find`

Find regex matches in a markdown file. Returns `[[offset, length], ...]`
sorted by appearance. Multiline regex (`^` and `$` match line boundaries).

```nu
peek md find '^## ' README.md
```


Installation
--------------------------------------------------------------------------------

Build from source, install to the nushell plugin directory, register:

```nu
^cargo install --path .
cp ~/.cargo/bin/nu_plugin_empower ~/.config/nushell/plugins/
plugin add ~/.config/nushell/plugins/nu_plugin_empower
```

Requires nushell 0.113 or later.


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
SourceTrait Empower Nu Plugin: Nushell plugin supported SourceTrait Empower  
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
