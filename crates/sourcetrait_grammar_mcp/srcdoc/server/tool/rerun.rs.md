# rerun.rs

## fn rerun
THE `is_base62` GATE IS PATH-TRAVERSAL DEFENSE IN DEPTH. The nonce is joined onto a filesystem
path, so it must reject `..`, `/` and anything outside `[0-9a-zA-Z]` before that join happens.

NO RE-LINT. The original `run()` already linted this body, and the cache is server-controlled,
so re-linting would only be able to reject something the server itself wrote.

A FRESH nonce is minted rather than reusing the source one, and the rerun caches its OWN body
under it - so any run-family nonce is uniformly a valid handle, and the two calls' logs stay
separate.

`InFlightKind::Rerun` carries the SOURCE nonce so `processes()` can show which body a rerun is
re-firing, which is the one thing a caller cannot infer from the fresh nonce.
