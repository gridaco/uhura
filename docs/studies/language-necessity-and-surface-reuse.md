# Does Uhura require an independent surface language?

- **Status:** Non-normative existential study
- **Lifetime:** Disposable study
- **Method:** Current repository audit plus primary-source comparison against
  the rolling official Svelte 5, Elm, SCXML, and XState v5 documentation
  retrieved July 18, 2026; version-sensitive claims are identified in the text
- **Doctrine:** [Mission and identity](../doctrine/mission.md),
  [authoring ergonomics](../doctrine/authoring.md), and
  [design principles](../doctrine/principles.md)
- **Related evidence:** [Instagram spike design](instagram-spike-design.md),
  [Instagram demo dogfood](instagram-demo-dogfood.md), and
  [client state survey](client-state-survey.md)
- **Boundary study:** [Escape hatches and foreign bindings](escape-hatches-and-foreign-bindings.md)
- **Problem corpus:** [Program harnesses](../../examples/programs/README.md)
- **Authority:** Research only; this document reserves no syntax and accepts
  no implementation architecture

## Outcome

The repository and prior-art audit found no evidence that determinism requires
novel concrete syntax. It also found no evidence sufficient to establish that
a JavaScript-less, Svelte-shaped profile can preserve the desired guarantees,
reuse Svelte's parser or toolchain in practice, or outperform an existing
language plus a library. Those are plausible candidates, not demonstrated
solutions.

Standard Svelte does not provide that guarantee merely by omitting a
`<script>` block. Svelte templates still admit JavaScript expressions and
callbacks, while its state, effect, binding, asynchronous, lifecycle, and DOM
facilities have a different authority model. A deterministic profile must
account for those facilities, whether by restriction, replacement, isolation,
or an explicitly narrower product guarantee.

Any restricted profile would define an accepted-program set and semantics,
even if it reused a familiar file shape, parser, compiler stage, or generated
JavaScript target. The open question is which, if any, layers Uhura must
independently own:

1. a closed frontend semantic contract;
2. concrete authoring notation;
3. parser, checker, formatter, and tooling;
4. runtime kernel and intermediate representation; and
5. renderer-neutral presentation.

Current evidence does not establish an absolute need for an independent Uhura
language, syntax, parser, checker, semantic view, or machine calculus. Each may
remain valuable, but each must be justified against the problem it solves and
the smallest viable alternative.

The first comparison authority is therefore not the current Uhura grammar,
store, IR, runtime, or renderer. It is the language-neutral
[program harness](../../examples/programs/README.md): independent Markdown
problem specifications whose transitions, outputs, invariants, and adversarial
traces must not bend around any candidate. The corpus now has executable Uhura
0.3 and plain TypeScript answers. Their presence supplies evidence; neither
becomes the problem authority.

No shared-kernel or shared-surface experiment is assumed first. Each candidate
may initially answer the programs with its own appropriate model. Controlled
same-kernel or same-surface experiments become useful only later, when the
project needs to isolate a specific source-ownership or kernel-ownership
question.

## 1. Question and non-goals

The question is:

> Can the smallest viable existing-language or checked-profile alternative
> solve the independent program problems and the frontend requirements that
> evidence retains, or does an independent source language create enough
> additional semantic compression and checkability to justify its cost?

This study does not:

- select `store`, `machine`, reducer, statechart, signal, or fact semantics;
- propose syntax, an IR, or a runtime;
- choose a Svelte compiler dependency;
- require JavaScript as a runtime target;
- decide whether renderer neutrality must remain a product goal;
- deprecate the current `.uhura` language;
- require candidates to share the current or any proposed semantic kernel;
- treat widgets or presentation as evidence that a machine language is sound;
  or
- replace future bounded comparative implementations.

## 2. Terms that must not be conflated

### 2.1 No authored JavaScript

Authors cannot write arbitrary JavaScript in scripts, templates, callbacks,
imports, component implementations, or other escape paths. Generated
JavaScript may still be an implementation target.

### 2.2 No JavaScript runtime

No JavaScript executes in the delivered target. Stock Svelte does not satisfy
this condition: its compiler produces JavaScript component modules. This
condition is independent from authoring-language closure and is not required
for semantic determinism.

### 2.3 Standard Svelte

A `.svelte` component as accepted by the Svelte compiler, including its
JavaScript expression language, runes, bindings, effects, lifecycle, actions,
attachments, stable promise-oriented
[`{#await}` blocks](https://svelte.dev/docs/svelte/await), the current opt-in
experimental [component `await`
facility](https://svelte.dev/docs/svelte/await-expressions), and DOM-oriented
runtime.

### 2.4 Svelte plus a machine library

Standard Svelte renders a machine owned by a library or external runtime. The
machine may be deterministic while the whole component language remains open.

### 2.5 A checked Svelte-shaped profile

A deliberately restricted language or project profile that reuses some Svelte
spelling or tooling while defining additional accepted-program and behavior
rules. Its guarantees belong to the profile, not to standard Svelte.

### 2.6 Semantic determinism

For the same program, initial state, and complete ordered declared inputs, the
specified observations and requested consequences are the same.

This is not pixel determinism. Fonts, layout engines, media clocks, device
input, frame scheduling, and physical animation remain renderer or host
concerns unless a named observation crosses the semantic boundary.

### 2.7 Surface and semantic language

The surface language is what authors write. The semantic language is the set
of accepted programs and their meaning. Two surfaces can lower to one
semantic language; familiar surface spelling does not import the host
language's semantics.

## 3. Program-first comparison boundary

### 3.1 The problem is held constant, not an Uhura architecture

The [program harnesses](../../examples/programs/README.md) are the first common
comparison boundary. They are pure, standalone Markdown specifications with no
candidate implementations. Their state, admitted inputs, ordered outputs,
transition behavior, observations, invariants, and traces are authoritative.

A candidate is not required to reproduce the current grammar, `store`, checked
IR, runtime decomposition, semantic-view protocol, or implementation language.
It may use an existing language and library, a checked profile, independently
owned source, an adopted machine notation, generated code, or another coherent
answer. It must not change a program to make that answer appear complete.

The first comparison therefore holds observable program behavior constant
while allowing source and semantic topology to differ. A shared kernel would
answer a narrower question about source surfaces; it would not answer which
kernel best models the programs.

### 3.2 Invariants and mechanisms are different kinds of claim

The current program corpus requires candidates to account for these behavioral
invariants:

- fixed declared inputs have one specified state, classification, observation,
  and ordered-output result;
- state changes and requested consequences have explicit causes;
- external reports are correlated with the work that admitted them;
- ambient clock, randomness, storage, network, or host mutation cannot be
  silently used to satisfy a trace;
- each admitted step performs finite work and completes atomically where the
  problem requires atomicity; and
- the same initial state and ordered inputs can be replayed to the same
  observations and outputs.

Those invariants do not select one enforcement mechanism. A candidate might use
a restricted expression language, pure functions, reducer discipline,
statecharts, effect descriptions, capability types, a sandbox, static
analysis, runtime validation, or a combination. The comparison must record
which properties follow from the accepted language, which follow from a
library, which depend on project discipline, and which are not guaranteed.

In particular, this study does not infer from determinism alone that a
candidate must forbid all functions, require a total expression calculus, use
transition-only mutation, expose one machine, serialize one particular
intermediate form, or adopt the current Uhura event loop.

### 3.3 Frontend product obligations are separate evidence

Pure machine conformance does not establish the value of:

- renderer-neutral semantic presentation;
- navigation, page, surface, or component instance lifetimes;
- static design projection;
- canonical source and structured diagnostics;
- closed authoring across a complete frontend application;
- accessibility and platform-capability checking; or
- a built-in widget catalogue.

Those are consequential frontend questions, but they require their own
lifecycle, product, authoring, and widget evidence. A candidate must not receive
credit for them merely because the current Uhura implementation has them.
Conversely, a machine candidate must not fail the pure program comparison
because it does not implement a widget system that the problem never asked
for.

## 4. What the Svelte audit does and does not establish

The audit makes one negative distinction clear: removing a `<script>` block
from a standard Svelte component does not remove every JavaScript expression,
callback, binding, effect, lifecycle, promise, context, or DOM capability that
can affect component behavior.

The audit does not demonstrate a working deterministic Svelte profile. It does
not show that such a profile can reuse the Svelte parser, checker, formatter,
editor tooling, or component ecosystem at useful scale. It also does not show
that independently owned syntax would perform better. At most, the audit does
not rule out a restricted or isolated Svelte-shaped candidate.

Generated JavaScript is orthogonal to authored authority. It neither proves nor
disproves determinism by itself; the observable guarantees of a candidate
depend on the complete accepted-program and execution contract.

### 4.1 What Svelte might mechanically contribute

Svelte's [public compiler
interface](https://svelte.dev/docs/svelte/svelte-compiler) exposes compilation,
parsing, modern AST types, printing, and preprocessing hooks. Its `preprocess`
API can transform raw component source before compilation, but a custom
machine body still needs its own parser or transformation before Svelte can
accept it. Svelte's parser, AST, and printer can apply only to syntax that
survives as valid Svelte or after custom syntax has been removed.

The printer converts modern Svelte AST nodes back into valid Svelte source; it
does not establish another profile's formatting or compatibility contract. A
future bounded spike could measure how much parser, AST, diagnostic, formatter,
editor, and component reuse survives a specific restriction. No such spike has
been performed here.

Even successful mechanical reuse would not by itself select an architecture.
Any candidate claiming language-level guarantees must define how accepted
programs, checking, diagnostics, formatting, integration, and conformance
produce those guarantees. Parser reuse and generated web output remain
separate questions.

## 5. Why standard Svelte is not the same system

Svelte is not defective for failing these requirements; it solves a different
problem.

- `.svelte` scripts contain ordinary JavaScript or TypeScript, and template
  attributes, event handlers, and text admit JavaScript expressions.
- `$state` exposes mutable values and deeply reactive proxies.
- `$derived` constrains direct state changes but does not by itself close
  arbitrary JavaScript calls or ambient reads.
- `$effect` is designed for browser work, third-party libraries, canvas, and
  network requests, and runs relative to DOM updates and microtasks.
- `bind:` installs mutation-producing listeners and can call getter/setter
  functions.
- stable [`{#await}` blocks](https://svelte.dev/docs/svelte/await) observe
  promise lifecycle. The rolling Svelte 5 documentation retrieved for this
  study also describes opt-in experimental [component `await`
  expressions](https://svelte.dev/docs/svelte/await-expressions), enabled
  through `experimental.async`, whose updates may overlap; the documentation
  says the flag will be removed in Svelte 6.
- [lifecycle](https://svelte.dev/docs/svelte/lifecycle-hooks),
  [actions](https://svelte.dev/docs/svelte/use),
  [attachments](https://svelte.dev/docs/svelte/%40attach), window bindings,
  DOM measurements, and component implementations admit host authority.

Removing `<script>` therefore removes one source of arbitrary computation, not
the semantic model surrounding the template. A particular Svelte application
can follow deterministic discipline; standard Svelte cannot make the
language-level claim that every accepted application does.

## 6. Prior systems reduce the novelty claim

| Precedent | What transfers | What remains outside it |
|---|---|---|
| [The Elm Architecture](https://guide.elm-lang.org/architecture/) | Typed model, messages, pure update, derived view, effects as commands interpreted by a runtime | Renderer-neutral semantic widgets, static design projection, provider contracts, frontend-specific async compression |
| [W3C SCXML](https://www.w3.org/TR/scxml/) | State configurations, hierarchy, parallel regions, event processing, run-to-completion semantics, published interpretation algorithm | Closed typed data model, termination of every macrostep, semantic view, ports, compact builder notation |
| [XState](https://stately.ai/docs/transitions) | Machine and [actor](https://stately.ai/docs/actors) precedent, [pure transition APIs](https://stately.ai/docs/pure-transitions), [invocation lifecycle](https://stately.ai/docs/invoke), snapshots, [inspection](https://stately.ai/docs/inspection), and [persistence](https://stately.ai/docs/persistence) | Closed JavaScript-free functions, renderer-neutral view, typed external authority, static design examples |
| [Svelte](https://svelte.dev/docs/svelte/svelte-files) | Familiar single-file markup, blocks, keyed iteration, scoped authoring, compiler and preprocessing infrastructure | Closed behavior, explicit effect authority, renderer-neutral semantic output, deterministic replay contract |

Elm demonstrates pure, deterministic `update` and derived `view` functions for
fixed `Msg` inputs, with external work delegated as
[commands and subscriptions](https://guide.elm-lang.org/effects/). It does not
prove deterministic end-to-end scheduling or results for external effects.
SCXML shows that Uhura need not invent statechart execution from first
principles. Neither supplies the whole builder product.

The defensible Uhura hypothesis is therefore not “state machines have not
existed before.” It is that a deliberately closed frontend model can combine
semantic compression, explicit authority, static design, semantic widgets,
and canonical agent-facing source better than the viable alternatives.

## 7. Candidate realizations

### A. Existing language plus a machine library

Use an established general-purpose or frontend language with a reducer,
machine library, or disciplined program structure.

This may maximize ecosystem reuse and minimize new syntax. The comparison must
record whether the required program behavior is guaranteed by the language or
library, checked by project tooling, or maintained only by author discipline.

This is a genuine candidate, not merely a control. If it expresses the program
corpus clearly and its weaker guarantees are sufficient for the validated
product mission, no independent or new Uhura language may be needed.

### B. Checked profile over familiar notation

Define a restricted profile over Svelte-shaped or another familiar notation,
with whatever static or runtime enforcement its claimed guarantees require.

Its value depends on what it actually guarantees and how much parser, editor,
component, knowledge, and tooling reuse survives the restrictions. If nearly
every semantic facility is replaced, compatibility may be primarily visual.

### C. Independently owned Uhura source

Own the source grammar, source checker, formatter, and source diagnostics while
borrowing proven notation and machine semantics where useful. It may choose the
semantic model that best answers the programs; independent source syntax does
not imply that every compiler or execution layer must also be invented.

This may provide strong direct control and one canonical explanation. It also
has substantial language-engineering, teaching, tooling, and compatibility
cost at the source layer. It must demonstrate value beyond viable smaller
alternatives.

### D. Adopt an existing semantic language

Use Elm, an SCXML profile, an XState-compatible serializable model, or another
existing language as the behavior source.

This may minimize machine invention. Later frontend evidence must determine
whether adjacent lifecycle, presentation, or authoring facilities remain
coherent or recreate an Uhura-shaped language through constraints and
libraries.

## 8. Program-first research sequence

This is a non-binding sequence for producing evidence. It currently stops at
language-neutral Markdown problem specifications; there are no candidate
solutions to compare.

### 8.1 Phase zero: freeze independent problems

Review and freeze the
[program harnesses](../../examples/programs/README.md) before candidate
implementations are used to select a design. Each problem must remain useful
and intelligible without Uhura, Svelte, a renderer, or a proposed machine
model. Its Markdown specification is the authority.

The freeze covers the program's:

- configuration and initial state;
- admitted inputs, classifications, and ordered outputs;
- exact transitions and derived observations;
- safety invariants and liveness assumptions;
- canonical and adversarial traces; and
- invalid and stale input behavior.

If a candidate cannot express a requirement, preserve the requirement and
record the gap.

### 8.2 First comparison: independently appropriate answers

Give every candidate the same frozen problems and boundary information. Let
each candidate choose the source and semantic model it claims is appropriate.
The first comparison does not require a shared syntax, kernel, IR, runtime,
checker implementation, or generated target.

An existing language plus a library is eligible on equal terms with a checked
profile, independently owned source, or adopted semantic language. The current
Uhura implementation is prior evidence, not the oracle; it may participate only
through answer sheets that leave the problems unchanged.

Program conformance is pass/fail. A candidate must reproduce the specified
state, classifications, observations, ordered outputs, and invariants for every
required case. It must also state whether that result is enforced by the
language, a checker, a library, runtime validation, tests, or author
discipline. Weaker enforcement does not silently become a stronger guarantee.

Only semantically conforming candidates enter readability, compactness,
diagnostic, ecosystem, and maintenance comparison.

### 8.3 Later controlled comparisons

After the independent answers expose real differences, a narrower experiment
may control one variable:

- hold a semantic model constant to compare source surfaces; or
- hold an authoring envelope constant to compare semantic models.

Such experiments answer attribution questions. They must not replace the first
program comparison, privilege the current kernel, or assume that the best
surface and best semantic model can be selected independently.

### 8.4 Separate evidence tracks

Four tracks answer different questions:

1. **Pure machine programs** test state, causality, atomicity, ordered outputs,
   correlation, cancellation, invalid inputs, and replay without presentation.
2. **Frontend lifecycle programs** test navigation, scope, instance identity,
   restoration, late events to destroyed instances, and static inspection.
3. **Product transfer** tests whether a candidate remains practical under a
   real application such as the Instagram harness, including external
   authority and authoring changes.
4. **Presentation and widgets** test semantic elements, accessibility,
   renderer capabilities, layout boundaries, gestures, animation, and visual
   authoring separately from the machine calculus.

A result in one track does not settle another. Pure-machine success does not
prove a frontend language is humane. A strong widget system does not prove the
state language is coherent. A visual demo does not override a failed invariant.

### 8.5 Required observations for the pure program track

Each candidate answer must show:

- exact observable behavior over the same declared inputs;
- deterministic replay where the problem requires it;
- ordered requested consequences and exact correlation behavior;
- the specified handling of invalid, stale, and duplicate inputs;
- no hidden host behavior needed to satisfy the trace;
- stable results for planted adversarial cases; and
- the same controlled change request after initial conformance.

Internal state shapes, trace encodings, and decomposition need not be
byte-identical. The program's observable semantics must be comparable.

### 8.6 Measurements

In proportion to the claim being evaluated, record:

- source, token, and checked-model size;
- number of concepts authors must name;
- duplicated facts and lifecycle bookkeeping;
- valid and invalid states representable;
- cause-to-effect reading distance;
- edits needed for the same controlled change;
- hidden defaults, ambient powers, and unenforced discipline;
- checker success, diagnostic distance, and repair attempts;
- human and agent comprehension or modification results when making an
  ergonomic claim;
- parser, formatter, language-server, adapter, and conformance maintenance when
  those layers are part of the candidate;
- safe ecosystem artifacts actually retained; and
- renderer and host coupling only in the frontend or product tracks where it
  is relevant.

## 9. Falsifiers and decision protocol

Exploration may begin without a full empirical protocol. Before comparative
results are used to select a language direction, however, the relevant
decision rules must be fixed:

| Item | What must be fixed before selection |
|---|---|
| Problem authority | Frozen Markdown program versions, traces, invariants, invalid cases, and controlled change request |
| Candidate scope | Which source, checking, library, runtime, renderer, or product layers each candidate claims to replace or retain |
| Conformance | A rule that every candidate eligible on a program passes all of that program's semantic cases |
| Guarantee audit | Which properties are language-enforced, statically checked, runtime-validated, test-only, or disciplinary |
| Ergonomic claim | Tasks and measurements appropriate to the actual human or agent claim; participant protocols only when such a study is genuinely being run |
| Ecosystem reuse | Named artifacts and workflows counted as retained only when usable without bypassing the candidate's claimed guarantees |
| Cost | Included implementation and maintenance layers, comparison horizon, and excluded sunk cost |
| Product evidence | The separate lifecycle, product, authoring, and widget requirements whose value is being tested |

Program conformance is pass/fail. Readability, compactness, diagnostics,
ecosystem reuse, and lifetime cost are comparative evidence. This study sets no
numerical winner threshold. Executable baselines now exist, but the controlled
human and agent comparison remains unfunded and unrun.

Independent syntax is not justified merely because it can express the
programs. It must create meaningful value beyond a smaller conforming
alternative. Conversely, an existing language plus a library is not rejected
merely because it permits powers the program does not use; the decision must
state whether those powers defeat a product requirement that evidence has
actually retained.

The profile's reuse claim is weakened when it must replace the host expression
language, effect model, imports, component eligibility, checker, formatter,
and conformance contract, while admitting few existing components or tools.
That result may still justify familiar notation, but not an unqualified
ecosystem claim.

The Uhura kernel itself is not justified when an existing machine or language,
plus a small explicit boundary, meets the program requirements and every
separately validated product obligation without recreating another language
through configuration. An independent kernel remains justified only by
evidence that its model, guarantees, ergonomics, portability, or lifetime cost
is materially better.

Lifecycle, presentation, or product evidence may recommend revising the
mission or retaining a strong requirement. An RFC, not this study, decides
either outcome.

## 10. Current conclusions

Supported now:

- The audit found no evidence that deterministic state-machine semantics
  require novel syntax.
- Generated JavaScript does not by itself decide whether authored behavior is
  deterministic.
- Merely removing a Svelte `<script>` block does not provide the required
  guarantee from standard Svelte.
- A restricted Svelte-shaped profile is not ruled out, but no such candidate
  has been implemented or validated by this study.
- The current Uhura repository is evidence about one attempted design. It is
  not the authority for the replacement language.
- The program problems remain Markdown-only authorities. Executable Uhura 0.3
  and plain TypeScript answer sheets now exist as subordinate evidence.
- Current evidence does not prove an absolute need for an independent Uhura
  language, syntax, parser, toolchain, semantic view, or novel machine
  semantics.
- Renderer neutrality, static design, and closed authoring may create product
  value, but that value must be tested separately from pure machine
  conformance.

Still open:

- Can an existing language plus a library express the pure program corpus
  clearly enough that no new language is warranted?
- Which guarantees must hold for every accepted program, and which can remain
  project conventions or tests?
- How much of the actual Svelte ecosystem survives the required restrictions?
- Can imported components be checked without becoming authority escape paths?
- Can a profile provide equally local diagnostics and canonical formatting?
- Does independent syntax produce measurable semantic compression beyond a
  profile?
- Is coupling to another compiler and AST cheaper than owning a parser?
- Can semantic widgets remain renderer-neutral through a Svelte-shaped
  source?
- Which representation is easier for human and software-agent authors?
- Is the current deterministic step the right kernel or only the best current
  experiment?
- Which frontend lifecycle pressures require language support rather than
  user-built composition?
- Would a web-only product satisfy the real mission at substantially lower
  cost?

## 11. Consequence for current language redesign

The earlier machine, signal, and fact-system alternatives remain useful
semantic candidates. None should be selected solely because it makes a better
standalone syntax.

The next bounded work is to review and freeze the language-neutral
[program harnesses](../../examples/programs/README.md), then prepare the
smallest honest answer sheets from materially different candidate families.
An existing language plus a library is a first-class answer, not a deliberately
weaker control. No candidate is required to reuse the current kernel, and no
Svelte-shaped challenger is prescribed as the first implementation.

After pure program comparison, separate lifecycle, product, authoring, and
widget evidence can test whether conforming machine ideas remain suitable for
Uhura's frontend mission. Later controlled experiments may isolate surface or
kernel effects when the evidence gives a concrete reason to do so.

The possible outcomes remain:

```text
existing language plus library
checked Uhura profile over familiar notation
independently owned Uhura language
adopted external semantic model
mission or requirement revision
```

No outcome is privileged by this study.

## Primary sources

These links are rolling official documentation, not frozen source snapshots.
Before an RFC depends on a version-specific API or AST, it must pin framework
versions or official source commits and preserve the relevant conformance
evidence.

- [Svelte `.svelte` files](https://svelte.dev/docs/svelte/svelte-files)
- [Svelte basic markup and events](https://svelte.dev/docs/svelte/basic-markup)
- [Svelte `$state`](https://svelte.dev/docs/svelte/%24state)
- [Svelte `$derived`](https://svelte.dev/docs/svelte/%24derived)
- [Svelte `$effect`](https://svelte.dev/docs/svelte/%24effect)
- [Svelte bindings](https://svelte.dev/docs/svelte/bind)
- [Svelte `{#await}` blocks](https://svelte.dev/docs/svelte/await)
- [Svelte asynchronous expressions](https://svelte.dev/docs/svelte/await-expressions)
- [Svelte lifecycle hooks](https://svelte.dev/docs/svelte/lifecycle-hooks)
- [Svelte actions](https://svelte.dev/docs/svelte/use)
- [Svelte attachments](https://svelte.dev/docs/svelte/%40attach)
- [Svelte compiler API](https://svelte.dev/docs/svelte/svelte-compiler)
- [The Elm Architecture](https://guide.elm-lang.org/architecture/)
- [Elm commands and subscriptions](https://guide.elm-lang.org/effects/)
- [W3C SCXML 1.0](https://www.w3.org/TR/scxml/)
- [XState transitions](https://stately.ai/docs/transitions)
- [XState pure transition functions](https://stately.ai/docs/pure-transitions)
- [XState actors](https://stately.ai/docs/actors)
- [XState invocation](https://stately.ai/docs/invoke)
- [XState actions](https://stately.ai/docs/actions)
- [XState inspection](https://stately.ai/docs/inspection)
- [XState persistence](https://stately.ai/docs/persistence)
