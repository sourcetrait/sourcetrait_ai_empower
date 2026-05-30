PROJECT: sourcetrait_empower
================================================================================

## Code 

### Basic

ASCII only. No em-dashes; use "-" instead of "--".

80 character hard wrap.

Use terse language in documentation.

Use well-formed idiomatic and re-usable source-code.

Use 4-space indentation for code.

Default define `pub(crate)` for everything within the crate.

`lib.rs` exists purely as a manifest for `mod` and `use`.

Modules should only `use crate::*`.

Prefer to re-export external crate modules in `lib.rs` and use them instead of
directly calling the type. Eg, `process::ExitStatus` instead of `ExitStatus`.

Public re-exports are explicitly defined in `lib.rs`.

### External Libraries

Prefer the `indoc` crate's tools for formatting multi-line source or text
content. Easier to read and maintain.

Use `sourcetrait_testing` for integration tests. Prefer integration tests
over unit tests when possible.

## Tools

The `LICENSES-EXTERNAL.txt` can be generated with:
```nu
^cargo license --color never | save -f LICENSES-EXTERNAL.txt
```
