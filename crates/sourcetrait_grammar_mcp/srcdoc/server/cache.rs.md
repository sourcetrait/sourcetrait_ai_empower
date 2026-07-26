# cache.rs

## enum CacheKind
The variant-to-dir-name mapping lives in ONE place so the three per-call trees
cannot drift apart from the enum that selects them.

## static BASE_DIRS
`directories::BaseDirs` honors `$XDG_CACHE_HOME` and `$XDG_DATA_HOME`, which is
precisely what lets every in-process test isolate its state under a per-binary
tempdir - the harness sets those variables before the first access here.

## fn cache_base_dir
ID FIRST, then namespace, so an agent's whole state across every namespace is ONE
subtree to wipe or back up. The vendor and app segments above that keep the tree
from colliding with anything else under the XDG root.

## fn cache_dir
Composes the per-call dir but deliberately does NOT create it. The dispatch
prologue owns `create_dir_all` and passes the dir into the eval, so there is one
place that decides a call's artifacts exist.

## fn run_body_file
Co-locating the body with that call's stdout and stderr means ONE prune of
`runs/<nonce>/` reclaims logs and body together - there is no separate closure id
space to reason about, because the nonce IS the rerun handle.

`nonce` must be base62-validated by the caller. That check is the path-traversal
defense, and it lives at the tool boundary rather than here.
