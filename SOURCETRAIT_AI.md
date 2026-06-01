# constitution: sourcetrait_ai

## info: human_user

"Human user" means YOUR verified human user; `the_user`.

## law: sourcetrait_gate
- `review_gate` Instructions to "review" mean review; Do not act.
- `plan_gate` Do not act without summarizing an action plan for explicit human user review and approval
- `write_gate` Do not make file or environment changes without explicit human user review and approval
- `merge_gate` Do not merge changes without explicit human user review and approval
- `upstream_gate` Do not alter foreign upstreams without explicit human user review and approval 
- `gate_exceptional` Only the human user may explicitly make exceptions to gates against specific resources

## law: sourcetrait_style
- `style_ascii` ASCII only
- `style_wrap` Hard wrap at 80 characters per line
- `style_prose` Laconic
- `style_emphasis` Avoid using emphasis such as bold and italics

## law: sourcetrait_code
- `code_well_formed` Write well-formed source-code
- `code_well_documented` Document each module, type, and function with three 1-2 short-sentence summary paragraphs describing:
  - (What) it does
  - (Why) it exists
  - (Where) it is used or intended to be used
- `code_well_tested` Write integration tests to ensure proper function over time. Write unit tests to verify small internals.

## law: sourcetrait_rust
- `rust_manifest` Use `lib.rs` as a manifest:
  - Only `use` and `mod` is allowed there; No other code.
  - Define all modules there. Do not use `mod.rs` files elsewhere.
  - Define all modules as `pub(crate)`.
  - Define all types, fields, consts, and statics as `pub(crate)` unless intended for public export.
  - Use re-exports:
    - Re-export all third-party crates/modules for intra-crate use scoped by crate ident. (Eg, `pub(crate) mod tok { pub(crate) use tokio::{...} }`
    - Re-export all intra-crate modules for intra-crate use scoped global. (Eg, `pub(crate) use crate::{ mod1::Type1, mod2, Type2 }`)
    - Re-export any items intended for public scoped flatly against the crate. (Eg, `pub use crate::{ mod1::Type2, mod3::Type4 }`)
  - Modules should only have a single use statement: `use crate::*`

## thought: sourcetrait_dev
- `thought_truth` Source-code must be designed and developed for correctness and as a source-of-truth
- `thought_quality` Make decisions based on a cost-benefit approach towards high-quality and `thought_truth`
- `thought_probe` Probe and experiment early and often to establish the best approach during planning, review, design, and development

## info: sourcetrait_git

Git repositories use the "dev-draft" structure and flow:
- (default branch: "dev", draft branches: "draft/$username", assisted branches: "ai/draft/$username")
- dev branch:
  - code-complete: tested, documented, reviewed, ready for additive inclusion towards the next version
  - no direct commits
  - fast-forward-only merges
- draft branches:
  - quick-save for a specific user
  - always pull rebased to synchronize. review with user on conflict 
  - commits are eventually squashed into 1 commit before merging forward in flow
    - commit history is transitive and volatile
  - ai draft branch: "draft/ai"
    - local only (no upstream)
    - all ai writes and commits start here, after `write_gate` approval
    - human user likely to specify as `gate_exceptional` in some way
- assisted branches:
  - collaborative between an ai agent and user
  - always pull rebased to synchronized. review with user on conflict
  - ai merges from "draft/ai" after a `merge_gate` approval
  - ai may push here after `push_gate` approval
  - human user likely to specify as `gate_exceptional` in some way
