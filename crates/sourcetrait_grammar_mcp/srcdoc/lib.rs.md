# lib.rs

The manifest carries no prose of its own by convention, so what follows is only
what a reader of it could reasonably get wrong.

## mod schema
There are TWO modules named `schema` and they never collide. `crate::schema` is
the shim re-exporting `schemars::JsonSchema`, reached only through the
`schema::JsonSchema` path on a derive; `crate::server::schema` is the json/nu
typedef converter pair, reached only through the bare re-exports in the
crate-internal `use` block. Neither is ever named the other's way.

## use futures_util
`SinkExt` and `StreamExt`, and `PemObject` beside them, ride at CRATE ROOT
rather than inside a shim. They are TRAITS, so method dispatch needs them in
scope, and a shim would park them at `shim::Trait` where the calls in the
channel hub could not see them. That is the manifest's trait exception rather
than an oversight.

## mod guts
The one `pub mod` that is not part of the public face. The integration tests are
separate crates and therefore see only `pub` items, so the test-support harness
has to be public to be reachable at all; `guts` is the convention's name for
that, chosen to signal both its internal-only status and the bravery of anyone
outside the workspace reaching for it.

## use crate::cli::host_main
The whole public surface. Everything else is crate-internal, which is why the
`pub use` block at the end of the manifest is one line.
