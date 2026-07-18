# Authoring ergonomics

- **Status:** Durable working doctrine
- **Lifetime:** Version-independent; exact syntax belongs to a named version
- **Theme:** Human and empirical
- **Authority:** Evaluation criteria, never accepted grammar
- **Related rubric:** [Design principles](principles.md)

Uhura is authored directly by people and software agents. Its source is not an
incidental serialization of an editor model; it is a primary user interface.
Readability, learnability, compactness, diagnostics, and defaults are therefore
first-class language-design and product-quality concerns, not polish to apply
after the engine works. A default additionally becomes part of program
semantics when observable behavior depends on it.

## Readability

Readable source lets an author predict behavior with bounded local context.
It should exhibit:

- **role-expressive names:** syntax makes a construct's semantic role visible,
  for example whether it declares an owned fact, derives presentation, names a
  cause, or requests work across a boundary;
- **visible ownership:** a reader can tell which component, provider,
  renderer, or driver owns a fact;
- **local causality:** the path from a declared cause to owned changes and
  requested consequences does not require reconstructing hidden global
  callbacks;
- **regular grammar:** similar concepts use similar shapes and different
  concepts do not differ by invisible convention;
- **one canonical form at each abstraction level:** formatting and equivalent
  spellings do not create irrelevant variation, while deliberately distinct
  high- and low-level contracts remain possible;
- **bounded indirection:** imports and reuse help compression without turning a
  small interaction into repository archaeology; and
- **diagnostic proximity:** errors point to the author's mistaken concept and
  explain the violated contract.

Human readability and agent readability overlap but are not identical. A
familiar keyword may help a person while an unambiguous grammar helps a parser
or model. Uhura should test both populations rather than treating “AI-friendly”
as a synonym for verbose or “human-friendly” as a matter of taste.

The [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/)
put clarity at the point of use ahead of brevity. That priority transfers even
though Uhura is a language rather than an API: remove ceremony aggressively,
but not the names or structure that explain ownership and behavior.

Familiar structure can also lower the cost of entry. Source familiarity and
semantic inheritance are separate decisions. Uhura's view language can borrow
the recognizable shape of HTML and [Svelte](https://svelte.dev/) without
inheriting arbitrary JavaScript, ambient callbacks, or DOM-specific runtime
semantics. It may reuse an established grammar, file envelope, parser, or
toolchain where the admitted subset remains independently specified, closed,
canonical, portable, and checkable.

Novel syntax earns no credit through novelty. Familiar notation earns credit
only for the knowledge, tools, and artifacts that remain safely reusable after
Uhura's constraints are applied. Familiar spelling is a means; a checked and
portable model remains the constraint.

## Learnability: bounded acquisition and transfer

Familiarity and learnability are not the same claim. Familiarity describes
knowledge an author already has. Learnability describes whether an author with
no assumed Uhura knowledge can acquire the language from a bounded canonical
teaching packet, translate language-neutral intent into source that checks and
has the intended semantics, and diagnose and repair mistakes.

Review the learning curve along distinct dimensions:

- **prior familiarity:** which notation, concepts, and tools may already be
  known to the intended author;
- **discoverability:** whether names, structure, diagnostics, and the reference
  make the next valid construct findable;
- **acquisition:** how much canonical instruction is needed before the author
  can form valid programs;
- **transfer:** whether the learned model composes on held-out problems rather
  than only reproducing taught examples;
- **repair:** whether a failed attempt and its diagnostics lead efficiently to
  a conforming program; and
- **retention:** for human authors, whether the model remains usable after time
  away without relearning incidental rules.

Familiar syntax can improve acquisition, but it can also create false friends.
A form that resembles HTML, Svelte, JavaScript, or another language incurs a
cost when authors import state, evaluation, scoping, or lifecycle rules that
Uhura does not provide. A proposal must measure this negative transfer rather
than crediting resemblance by itself.

The canonical teaching packet is part of the language product. It should offer
a compact semantic overview, a closed syntax and concept reference, a small set
of representative examples, and the diagnostic vocabulary needed for repair.
It must not require repository archaeology or knowledge of the current engine.
Its exact size belongs to a versioned evaluation, but competing designs must
receive the same information and comparable reading or context budgets.

An intent-to-source trial starts from a language-neutral description of a
program, not an example tailored to the candidate syntax. After the teaching
packet, test previously unseen tasks and record:

- instruction size and study time or context tokens;
- first-attempt parse and check success;
- semantic conformance, including behavior and declared limits;
- invented constructs and semantics imported from other languages;
- reference lookups, diagnostic quality, and repair attempts;
- unnecessary source and concepts; and
- transfer to new combinations of previously taught concepts.

Human and agent trials are separate evidence. Human experience, study time,
and retention interval are test conditions. For agents, undisclosed training
exposure usually cannot be proved or excluded; the reproducible approximation
is a clean context with a fixed teaching packet, task prompt, tool access, and
reported model version. A plausible completion from one model is not evidence
of learnability.

The Carnegie Mellon
[Natural Programming project](https://www.cs.cmu.edu/~NatProg/) is useful
precedent for treating programming notation as a human-centered empirical
design problem: study how intended authors express and debug tasks, then test
the resulting language. Uhura extends that obligation to software agents
without assuming that human and model learning curves are identical.

## Compactness means semantic compression

The goal is not the fewest characters. The goal is the least source and least
mental bookkeeping needed to state the full intent.

A feature creates semantic compression when the language models a recurring
concept once and consequently removes duplicated state, guards, event
plumbing, correlation, accessibility work, or failure handling. An alias that
merely hides those obligations creates shorter text, not a smaller model.

Character count, token count, line count, and AST size can be useful secondary
measurements. They must be considered beside:

- the number of concepts an author must name;
- the number of places that repeat one fact;
- the number of valid and invalid states exposed;
- the distance between cause and effect;
- the number of edits needed for a realistic change;
- the amount of hidden default behavior;
- checker success and repair attempts; and
- the source needed to express failure, cancellation, accessibility, and
  static-preview states.

The Cognitive Dimensions tradition treats terseness as one dimension among
tradeoffs such as hidden dependencies, viscosity, role expressiveness, and
error-proneness. Green and Petre's
[usability analysis of programming notations](https://doi.org/10.1006/jvlc.1996.0009)
is a useful warning against optimizing a notation on one visual metric.

## Concept and topology budget

Every first-class feature spends more than syntax. Review its cost across:

| Surface | Questions |
|---|---|
| Grammar | Does it add a new form, precedence rule, scope, or exception? |
| Semantics | Does it introduce a new kind of state, event, ownership, or lifecycle? |
| Static model | Can it be checked, lowered, inspected, formatted, and diagnosed canonically? |
| Semantic runtime | Does it change transition ordering, determinism, snapshots, or replay? |
| Renderer and host | Does every target need a new capability, fallback, or negotiation rule? |
| Tooling | Can editors, static previews, traces, and agents understand it? |
| Ecosystem | Which tools and components remain safely reusable, what version coupling is introduced, and what claimed compatibility is only visual familiarity? |
| Teaching | Does it overlap an existing concept or require another mental model? |
| Evolution | What compatibility burden and future interaction space does it create? |

A feature “pays rent” when it reduces the total model authors need for
important work. A frequently used concept may deserve first-class syntax. A
large but coherent widget contract may belong in a catalog. A niche
composition may be better as a pattern. A physical mechanism may belong only
to a renderer. The language should not promise every useful UI object as a
core concept.

### Surface ownership is an ergonomic question

When a proposal materially changes the authoring envelope, evaluate the real
familiarity, tooling, components, and workflows preserved after its
constraints are applied. Independent syntax must justify its teaching,
tooling, migration, and maintenance costs; reuse must justify its coupling by
preserving actual capability rather than a familiar appearance. The detailed
candidate set and comparison method belong to the study or proposal at hand,
not permanent doctrine.

## Good defaults

The shortest path should produce a credible interface or a useful diagnostic
when required product information is absent. “Good default” has three
different cases:

1. **System-owned safe behavior:** Uhura, a catalog, or a renderer can provide
   semantic roles, focus and keyboard mechanics, input-method independence,
   touch-target policy, reduced-motion handling, interruption behavior, and
   declared capability fallback where the contract determines them.
2. **Required author information:** the checker can require an accessible
   name, localized message reference, validation branch, or other information
   the system cannot invent.
3. **Explicit product policy:** copy, business validation, retry policy,
   authorization behavior, and domain-specific failure recovery remain the
   author's or provider's decision.

A default is not a hidden law. It needs a named semantic contract, predictable
override points, and diagnostics for combinations that violate accessibility
or runtime invariants. “Customizable” must not mean that every author rebuilds
basic interaction safety.

## Progressive disclosure

Common correct behavior should require little source. Advanced control should
appear only when the task needs it:

```text
safe default
  -> explicit options
  -> composable lower-level contract
  -> declared target capability or escape boundary
```

This pattern is visible in mature UI systems. Flutter distinguishes
[implicit animations](https://docs.flutter.dev/ui/animations/implicit-animations)
that manage intermediate behavior from explicit animation APIs. Qt Quick
offers both state transitions and property
[behaviors](https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html).
SwiftUI applies animation to state-driven view changes while retaining more
specific controls. The transferable lesson is the layered authoring model,
not any framework's exact API.

Each level should still have one canonical spelling and explanation. Progressive
disclosure adds deliberately different levels of control; it does not justify
aliases whose only effect is surface variation.

## Consistent scalar conventions

Bounded dimensionless proportions should use one consistent normalized
convention. Uhura's doctrine prefers `0..1`: a person should not need to
remember whether one feature chose `0..1`, `0..100`, or an engine-specific
scale. A named version defines accepted literal forms, units, clamping,
extrapolation, and out-of-range diagnostics.

Normalization makes interpolation composable. Flutter's animation foundation
likewise uses a nominal `0.0..1.0` progress value and maps it through curves
and tweens
([animation overview](https://docs.flutter.dev/ui/animations/overview)).
This doctrine does not decide direction, interruption, or the meaning of a
particular property; each versioned capability must specify those separately.

## Static design is a first-class authoring task

Uhura serves a builder, so an author must be able to inspect meaningful
experience states without reproducing the incidental event history that
reaches them. A version should define a preview contract that supplies all
semantically relevant inputs and any physical pose it deliberately exposes.

Static projection must remain honest:

- a selected experience state is not proof that all paths into it are
  reachable;
- an animation pose is not an executing timeline;
- a pending result is not a live network request; and
- physical state is exposed only through a defined preview contract.

Static examples complement traces and conformance tests; they do not replace
them.

## Evidence from prior art and adoption

Prior art should be classified rather than name-dropped:

1. **formal precedent:** the model has defined semantics or proofs;
2. **product or implementation precedent:** a shipping system demonstrates
   availability and feasibility;
3. **convergence evidence:** independent systems repeatedly choose a similar
   concept;
4. **ergonomic evidence:** observation or study supports an authoring claim;
5. **adoption evidence:** dated usage data shows that people actually use and
   retain the concept; and
6. **transfer evidence:** the assumptions still hold for an external,
   renderer-neutral builder language.

Official framework documentation proves product support, not adoption or user
preference. Adoption claims should cite a dated survey, telemetry, repository
corpus, longitudinal study, or similarly inspectable source. Repeated
convergence across independent ecosystems is evidence of a recurring design
problem, but still not proof of the right Uhura abstraction. For example:

- Elm and Android make state-down/events-up topology explicit.
- Qt Quick, Flutter, and SwiftUI all provide domain-level animation
  conveniences rather than requiring authors to update every frame.
- Flutter separates raw pointers from recognized semantic gestures.
- Web standards separately model semantic documents, style, animation timing,
  accessibility, and device input.

The survey paper
[“When and How to Develop Domain-Specific Languages”](https://doi.org/10.1145/1118890.1118892)
describes the potential expressiveness and usability gains of a DSL together
with the domain and language-engineering cost. Uhura should continuously
justify that cost with domain-specific compression.

## Evaluation protocol

Readability and compactness claims should be tested on a versioned corpus, not
settled by isolated snippets. At minimum:

1. choose representative tasks in proportion to the claim; a language-wide
   corpus should eventually cover a small interaction, a form, an asynchronous
   flow, navigation or surfaces, a collection, failure recovery, motion or
   gesture, and restoration;
2. include adversarial cases such as cancellation, stale outcomes, duplicate
   events, missing capabilities, reduced motion, and invalid state;
3. compare the current language, the proposal, and the smallest viable
   alternatives relevant to the claim on the same behavior;
4. record source size, named concepts, duplicated facts, edit distance,
   diagnostic distance, invalid states, and hidden defaults;
5. test comprehension, authoring, modification, and repair separately;
6. run both human and agent trials when the claim concerns both; and
7. preserve examples and checker expectations so later changes can reproduce
   the result.

Agent evaluation should use the bounded-acquisition protocol above and
held-out tasks, not merely ask whether one model generated plausible text once.
Model versions and prompts are test conditions, not permanent language
doctrine.
