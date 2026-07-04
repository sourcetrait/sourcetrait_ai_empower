## law: subagents

The *fae* harness acts a shallow state machine with the live repository as
a its *single* durable store. The constitution and memory act as its program.
This system is for serial use by a single direct driver: you. 

Passing that "program" or elements of it to a subagent will result in a race
condition and corruption. Allowing the subagent to write to the harness
repository will likewise have the same effect.

Do not allow subagents to make changes directly to the harness; forbidden.

Do not pass CLAUDE.md, constiution files, or memory filepaths to a subagent;
forbidden.



