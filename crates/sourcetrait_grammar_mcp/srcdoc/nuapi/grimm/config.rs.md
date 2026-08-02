# config.rs

The record these decls return mirrors the TOML - the same two tables, the same key
names - so ONE shape describes the file, the `.nutype` model beside it, and what a
body sees.

The argument-owned values (`--id`, `--namespace`, `--workdir`) are deliberately
absent: they are not config-file settings, and a body already reads them ambiently
as the `EQUIP_*` trio.

VALUES ARE LIVE RATHER THAN AS-LAUNCHED. The channel's spam thresholds come from
the channel handle, which `config_channel` mutates, and the supervisor lines come
through the pin layer - so what a body reads is what is actually in force rather
than the startup seed. A startup seed nobody is using would be a lie with a
timestamp.

## const KEY_SEP
The reason `get_config_all | get <key>` and `get_config <key>` agree: the key is a
PATH into the record the other decl returns, not a parallel naming scheme.

The walked form needs a LITERAL cell path, which is the trap here. A string
variable passed to `get` binds as one member containing the dots, so it looks for a
column of that whole name - splitting on dots is the source parser's job, not
`get`'s, so a `$key` string variable fails with
`Cannot find column 'supervisor.cpu_warn_fraction'`.

## fn config_record
Reads through `effective_supervisor()` rather than `config().supervisor`, which is
what keeps a pin from being something the watchdog honors while a read reports the
old value.

## fn config_keys
Derived from the record rather than restated beside it, so the vocabulary an error
message needs cannot drift from the vocabulary the record actually has.

## fn unknown_key
THE VOCABULARY GOES IN THE TITLE, NOT ONLY THE LABEL. `GenericError` renders its
title through Display, and that is all the eval envelope's `message` carries - a
label reaches a human reading a rendered diagnostic and never reaches the agent.
Listing the valid keys IS the entire value of this error, so keeping them only in the
label would name a problem and withhold the answer. A test asserts the list reaches
the message.

## fn value_type
Spelled as a union rather than `any`, because the set is CLOSED and naming it lets
a caller's own annotation be strict too.

## struct GrimmGetConfigAll

### fn signature
An OPEN record on purpose. The shape is documented by the `.nutype` model beside
the defaults, and restating it here would be a second place to keep in step.

## struct GrimmGetConfig

### fn run
An unknown key is an ERROR rather than a null, so a typo cannot read as
"configured to nothing". The two are indistinguishable at the call site and only
one of them is a bug the caller wants to hear about.

## struct GrimmPinConfig
Only the `[supervisor]` warning lines are pinnable. `[channel]` is excluded so
`config_channel` stays the sole mutator there - a value with two mutators is one
whose effective setting depends on which surface you happen to ask. Everything else
in the config is not a runtime value at all.
