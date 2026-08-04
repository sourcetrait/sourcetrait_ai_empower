## protocol: iter_bootstrap

The `./iter/<topic>/bootstrap.md` is authored and maintained by the user. It is
authorative and its instructions must be completed in order and not be ignored
or deferred.

Along with bootstrapping instructions, it may provide other items of importance
specific to that iter as purely informational, separately sectioned.

### protocol: iter_rust_bootstrap

Fully read and understand every source file in the crate or workspace specified,
in pattern order:
- `Cargo.toml`
- src, tests, benches dirs: `*.rs` (start with `lib.rs` files if they exist)

### protocol: iter_nu_bootstrap

Fully read and understand every source file in the library specified,
in pattern order:
- `*.nu` (start with `mod.nu` files if they exist)

### protocol: iter_understood_bootstrap

Fully read and and undertsand every `*.md` in the `working` directory for
the Iter topic specified (in order if chapter numbers appear in file names).
