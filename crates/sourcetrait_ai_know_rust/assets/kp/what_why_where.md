# What-why-where: the kp thought exercise

A thought exercise for documenting one source-code pick, NOT a fill-in
template. It forces you to consider every dimension relevant to an item
before composing prose. Classification happens AFTER you produce the map;
the final summary does not enumerate every slot.

You are handed a pick (a pattern, its seed span, and its one-hop carry
list). Read the source, decompose the pick per its category below, then
compress per the kp-authoring guide.

## The five item categories

Pick the category first, then think through its slots.

1. Functional - operations / actions / transformations. Functions,
   methods, behaviour-interface traits, sometimes macros (when they
   expand to operations).
2. Data-modeling - shape / representation. Structs, enums, unions,
   data-shape traits (Iterator, Index), type aliases.
3. Labeling - static values labeling something. Consts, statics, marker
   traits (Sized, Send, Sync).
4. Organizing - containment. Workspaces, crates, modules, sometimes
   macros (when they expand to multiple items).
5. Configuring - declarative directives that wire a plain type into a
   framework's machinery via compile-time codegen: configured-via-
   attributes derives (#[derive(Component)], #[derive(Serialize)],
   #[derive(FromValue)]) and configuring attribute-macros (#[tokio::main],
   #[wasm_bindgen]). The item is neither the data (the struct it sits on)
   nor a static label - it SYNTHESIZES an integration. None of the other
   four name it cleanly. Mechanical analysis marks only the broad
   "configuring" group; the fine reading is yours to supply here.

## Per-category decomposition slots

### Functional
What: operation (verb) + parameterized input + existing internal state +
existing external state + parameterized output + future internal state +
future external state. Pure functions are the degenerate case (empty
state slots).
Why: rationale for the operation; alternative rejected; invariant enforced.
Where: call sites + composition contexts + pipeline chaining + seams
crossed.

### Data-modeling
What: broad category + key sub-categories + sub-representational operation
(slice / get / projection) + transformative operation (into / to /
convert) + intended external functional use.
Why: rationale for the shape; efficiency / invariant the shape enforces.
Where: construction sites + consumers + data dependencies.

### Labeling (three-way)
- load-bearing - establishes relational context at callers
  (SCHEMA_VERSION; bumping cascades).
- considered-but-arbitrary - has rationale, invisible at call sites
  (PAGE_SIZE = 4096).
- broadly insignificant - no rationale beyond "needed a constant here".
Why: rationale for the value (when load-bearing or considered).
Where: use sites + load-bearing-vs-incidental treatment.

### Organizing
What: means of containment (workspace / crate / mod) + broad category of
items maintained + parent category + position in the parent / self /
children tree + structural shape.
Why: rationale for the containment + decomposition.
Where: parent context + sibling relationship + consumers.

### Configuring
What: the framework / seam it registers into + the generated surface
(emitted impl, associated types, hooks, required-components) + the
sub-attribute config vocabulary (#[component(...)], #[serde(...)],
#[arg(...)]) that is the load-bearing knob + codegen-time rejections /
footguns.
Why: rationale for integrating via derive / attr vs a hand-written impl;
what the framework guarantees once the type is registered.
Where: the registration seam (where the generated impl plugs in) + the
config sites. Deliberately loose - this category absorbs configured
derives AND configuring attr-macros; expand the slots from what the
specific pick actually does.

## Form vs role

An item has a syntactic FORM (kind by language construct) and an
architectural ROLE (semantic place in the workspace). Most align. When
they diverge, surface BOTH - form gives the syntactic landing, role gives
the architectural purpose. Examples of divergence:
- fn to_string(&self) -> String - form functional, role data-modeling.
- struct WorkerPool { ... } - form data-modeling, role functional.
- trait Plugin { fn build(&self, app: &mut App); } - form organizing-ish,
  role functional.

Trait sub-case (form ambiguous): behaviour interface (Plugin / Future /
Visitor) -> functional; data-shape interface (Iterator / Index / Deref)
-> data-modeling; marker (Sized / Send / Sync / Copy) -> labeling.
Default heuristic: count (fn methods : assoc types : empty body).
Mostly-fn -> behaviour; mostly-assoc-type -> data shape; empty -> marker.

## Pass discipline

1. Obvious pass: read source + doc-comments + the map; write what is
   obvious from those. Do not guess on gaps; leave unfilled slots terse.
2. Return pass: re-read against source. Mark gaps as UNRESOLVED rather
   than backfilling speculation. UNRESOLVED is a guardrail applied PER
   ITEM.

## Reduction through inference

Subtract what is inferrable from convention + the consumer-visible
vocabulary; KEEP the non-inferrable residual.

Inferrable (by convention): syntactic form; casing convention; idiomatic
verb shape (parse_ / encode_ / from_ / to_ / try_from_); common type
patterns (Result = fallible, Option = optional, &T / &mut T = borrow).
Inferrable (by context): workspace naming patterns once established;
recurring trait / type names once introduced; the pattern's kind + source
crate once laid down.
NOT inferrable: rationale; alternatives rejected; gotchas / edge cases the
name does not expose; internal-vs-external state effects beyond the
signature; call-site context / composition / seams crossed; workspace
invariants; intended-use vs technically-possible use; disambiguation when
a name collides (workspace Process vs std::process::Process).

Inference scope = the union of tokens surfaced in the consumer-visible
context. Tokens not referenced there are not inference-eligible.

## Signature visibility

The consumer sees only `<pattern> - <score meta> - seed <file:line>`; the
signature is NOT in their visible context. For function-shaped picks
(functions / methods / method-ref family / macro arms / proc-macro entry
points / configured derives that emit method sigs), surface the signature
in the kp header alongside the seed; the inference set then includes the
signature and the prose carries the residual. Type-shaped picks
(structures, marker derives, trait-def side) keep the principle that the
structural shape is inferrable from the name + vocabulary.

## Disambiguation

When introducing an ambiguous name for the first time, disambiguate if any
std / common-ecosystem type shares the name. After first introduction,
later references can drop the disambiguation if context holds.
