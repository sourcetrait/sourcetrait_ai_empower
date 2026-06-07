# Knowledge-product (kp) authoring

The kp is the compressed prose paragraph an agent reading the final
orientation consumes per pick. This guide governs Stage C (first draft)
and Stage D (reduction).

## Header shape

Per pick:

```
### `<pattern>`
- signature: `<sig in source form>`   (function-shaped picks only)
- seeds: <file:line>; <optional additional citations>

<prose paragraph; budget per the soft hint>
```

Function-shaped picks include the signature inline (see what-why-where
"Signature visibility"). Configured derives that emit method sigs cite the
canonical method sigs. Derive / macro picks naturally span multiple files
(trait def, derive-macro decl, codegen impl, representative use site);
cite multiple seeds.

## Length budget

2-4 sentences per paragraph (not 2-4 lines per slot). Foundational picks
with rich non-inferrable configuration surfaces legitimately stretch
toward 4 sentences, one of which may be a long semicolon-enumeration. The
per-pick budget hint is a SOFT target; produce near or below it when
content is non-inferrable. Spend the budget on the non-inferrable residual.

## What to drop

### Scaffolding labels
No form: / role: / what: / why: / where: / unresolved: labels in the prose.
Those are the thinking-pass forcing function, not consumer-facing output.
Form-vs-role surfacing stays important when role diverges from form, but
delivered INLINE, not tagged. Example for a configured derive: "a single
derive promotes a plain struct into an ECS citizen by emitting impl
Component" - the divergence (derive form, schema-registration-seam role)
is carried by the verb + the seam framing.

### Picker telemetry
Drop raw counts, inter_count, scoring deltas, is_pub flags, score
annotations - irrelevant to the reader. When SCALE is load-bearing for the
reader's mental model, surface it as a prose-only scale signal ("the
dominant cross-crate authoring vocabulary"; "every domain crate defines
its public shapes through this"), never as a raw number.

### UNRESOLVED markers (mostly)
Thinking-pass UNRESOLVED labels mark source-signal gaps you hit. Most are
tool-internal curiosities a reader never touches - drop them. Exception:
if an absent source signal is CONSUMER-relevant (a documented invariant
the source mentions but does not explain, where not-knowing-why changes
how the reader uses the pattern), keep a brief "UNRESOLVED: <gap>" clause.

## Reduction through inference (applied)

Subtract what the pattern name + signature + surrounding orientation
vocabulary already convey. A good configured-derive kp does NOT say
"Component is a trait that marks data as ECS-eligible" (inferrable from the
name); it DOES surface the attribute matrix (#[component] / #[require] /
#[relationship]), the codegen-fixed associated types, the lifecycle hook
slots, and the codegen-time footgun rejections - the non-inferrable
residual.

## Configuring picks specifically

A configuring pick (configured-via-attributes derive or configuring
attr-macro) earns prose on: what framework / seam it registers the type
into, the generated surface (emitted impl + associated types + hooks), the
sub-attribute config vocabulary that is the load-bearing knob, and the
codegen-time footguns. The mechanical layer marks only the broad
"configuring" group - YOU supply the fine reading. Do not restate that "a
derive generates an impl" (inferrable); surface WHAT the integration gives
the type and HOW the reader configures it.
