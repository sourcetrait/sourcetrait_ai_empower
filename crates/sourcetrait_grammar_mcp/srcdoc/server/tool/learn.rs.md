# learn.rs

## const NU_SKILL_TEMPLATE
`include_str!` at COMPILE time, so regenerating the live skill requires a REBUILD. The
consequence is operational and easy to trip over: editing the template and calling `learn()`
against a running host renders the OLD body.

## fn generate_skill
Native - no eval dispatch anywhere - which is why this tool needs no nonce, no log dir and no
in-flight entry.

The rendered skill is a GENERATED ARTIFACT: the source of truth is the crate asset, and the
file under the harness dir is output. Editing the output is editing something that will be
overwritten.

## fn learn
Version-stamps with this crate's own version, which is what lets a reader of a generated skill
tell whether it matches the host they are talking to.
