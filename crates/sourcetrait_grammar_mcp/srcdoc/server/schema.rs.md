# schema.rs

A strict, TOTAL, bidirectional converter between the agent's structured JSON schemas and
the nu positional type the eval parses.

TWO PARALLEL HIERARCHIES, one per representation, 1:1 and total between them. The
structural separation ENFORCES the grammar in the type system rather than in checks: top
level is `*ArgsTypedef` / `*ResultTypedef` = `Void | Record`, nested is `*Typedef` with no
Void at all. So "top-level is void-or-record" and "nested is never void" become
unrepresentable otherwise - Void lives only in the top-level enums, Nothing only in the
nested one.

`FieldName` and `ColumnName` are shared newtypes, since a NAME carries no representation
difference, but they stay distinct FROM EACH OTHER because a field and a column are not
interchangeable at any call site.

NO FALLBACK ANYWHERE - it works or it does not. Validity is confirmed at commit, so the
emit parser trusts grammar-conformant input, and a conversion failure on a validated
schema is a BUG rather than a soft path to handle.

## enum JsonScalarTypedef
Two plainly-derived enums rather than one shared type or a macro, written out in full. A
derived type reads far faster than a macro the reader has to expand mentally, and the
duplication is 14 names.

`from_name` rejects `any` EXPLICITLY rather than letting it fall to the unknown-token arm,
because `any` is a thing an author will reach for and deserves to be told it is not a
grammar type.

## const ONEOF_KEY
The literal `oneof<>`, brackets included. Collision-proof - no real field is named
that - and it READS as the nu type it compiles to. A plain field named `oneof` stays
legal, which is the property the brackets buy.

## fn json_typedef_kind
THE ONE PLACE the json-side disambiguation lives, so every grammar denial is a single
classify error rather than a scattered set of checks.

An array holding exactly one non-oneof OBJECT is a Table; anything else single is a List.
That is why a list of records IS the table form: they are the same JSON, and picking one
canonical reading is what keeps the converter total.

`allow_open_record` is threaded from the ARGS entry points only, and the List arm drops it
to false. An empty `record<>` is an OPEN record accepted in args - a call target taking an
arbitrary record needs it - but `list<record<>>` is JSON `[{}]`, which the grammar already
reads as an empty table, so allowing it there would not round-trip. Denying it keeps the
converter bidirectionally total.

## fn parse_json_result
Never allows the open record. A result schema declares what the body PRODUCES, and an open
record there would assert nothing.

## fn split_top_level
Depth-tracked on angle brackets, which is what lets a nested `record<a: int, b: int>` sit
inside a comma-separated list without its own commas splitting the outer one.

## fn render_signature_typedef
THE SIGNATURE RENDERERS are a PARALLEL pair rather than a flag threaded through the full
renderer, so each stays readable on its own - about twenty lines for the whole family.
They share the `Nu*Typedef` AST and the `record_j2n` mapping, so the two can only ever
differ in PRESENTATION, never in what they understand a type to be.

Two differences from the full spelling, both only at the TOP level: the `record<...>`
wrapper collapses to `<...>`, and a void renders `<>` rather than `<nothing>`. Nested types
keep their full spelling, so an open record still reads `record<>` inside one. The
separators lose their spaces throughout.

THE `<>`-FOR-VOID RULE IS LOAD-BEARING, not cosmetic. It is what makes a call line
self-identifying in the signature block - always two groups, void included - which is what
lets the block state no separator characters at all.

## fn args_schema_to_nu
Note what the emit direction does NOT use: `SyntaxShape::to_string()`. nushell's Display
for an empty-field record renders the bare word `record`, which is a lossy view and not
valid arg syntax, so the rig validator reads the args annotation from SOURCE TEXT instead.
Source text also preserves the SyntaxShape-flavored scalars - path, directory, glob,
cell-path - which the parsed `Type` collapses.
