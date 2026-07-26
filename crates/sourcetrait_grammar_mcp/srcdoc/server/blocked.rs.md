# blocked.rs

## struct BlockedDecl
It parses like the real builtin - a catch-all `rest` - and errors at run time, so a
body invoking one fails that SINGLE eval rather than terminating the shared host.

In-process there is no worker subprocess boundary to absorb any of this. The
shell-out era relied on exactly that boundary: `exit` calling
`std::process::exit`, or `exec` replacing the process image, killed only the worker
and the host saw EOF.

## const HOST_FATAL_DECLS
A full nu 0.114.1 registered-decl audit found these THREE the only builtins that
terminate the host DIRECTLY in-process from body input. That makes the list the
whole omit set rather than a starting point.

`panic` is in it even though `catch_unwind` catches a panic, because a panic
poisons shared mutexes on the way up - shadowing is cleaner than catching.

THE GUARDRAIL PRINCIPLE: shadow a vector whose LOCAL intent (tear down this eval's
engine state) would be a GLOBAL host teardown. A command with EXPLICIT target
intent stays ALLOWED, which is why `kill <pid>` is NOT here - it names a process
and means it, killing the host's own pid through it is the operator's intent, and
the body runs as the assigned box user with those rights and could signal any pid
via an external regardless. The shadows guard SURPRISE self-teardown, not a trusted
body's deliberate box actions.

## fn shadow_host_fatal_decls
MUST run after the shell command context that defines the real ones, so the shadow
wins name resolution - a later-registered decl overrides the earlier.
