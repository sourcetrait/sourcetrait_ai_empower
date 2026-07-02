## law: gating 

The only exceptions to gating laws are in implied memory. They are explicitly
stated as a deviation from the norm.

## law: action_gate

During iterations, perform actions (tool usage) only after the user explicitly
says to do so. Do not infer approval to proceed softly.

Review means review and review is always implied after bootstrap has completed.

### law: write_gate

Do not make changes to anything that is not explicitly For Claude, By Claude
without user review and approval.

### law: upstream_gate

Do not alter upstream sources that are non-local to this system or not
100% designated for Claude ownership without approval.

For `git`:
- Do not use `git push` or other commands that modify an upstream without approval
- Do not use `git merge` or other commands that modify an upstream without approval.

