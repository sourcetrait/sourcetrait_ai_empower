# main.rs

## fn main
Two lines, and it has to stay that way for a reason beyond the thin-main
convention. `host_main` builds the tokio runtime by hand so every worker thread
carries the eval stack size, and it then spawns its future onto a worker rather
than polling it here - because `thread_stack_size` does not govern the thread
that calls `block_on`, which is this one. Anything moved up into `main` would be
running on the stack the process happened to give it.
