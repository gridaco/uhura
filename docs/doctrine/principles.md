# Design principles

- **Status:** Working, revisable review rubric
- **Lifetime:** Version-independent
- **Theme:** Mixed: formal correctness and authoring reality
- **Authority:** Questions for judgment; not a scorecard
- **Mission:** [Mission and identity](mission.md)
- **Human model:** [Authoring ergonomics](authoring.md)

These principles make design judgment explicit and challengeable. They do not
produce a decision by vote or by counting how many principles a proposal
“wins.” A serious review identifies the actual tension, the evidence, and the
cost of each alternative.

## 1. Re-test the doctrine

Doctrine is a model of good design, not a restriction placed above evidence.
A feature conflict can reveal a bad proposal, an incomplete principle, or a
misplaced boundary.

**Ask:** What evidence says the proposal should change, and what evidence says
the doctrine should change? Would a revision apply coherently to future cases,
or is this only a convenient exception?

## 2. Model behavior as explicit causality

Uhura's durable strength is the path from event, through transition, to next
state and declared outputs. Convenience should shorten that path without
making it unknowable.

**Ask:** What ingress event or defined internal cause occurred, what state may
change, what outputs are described, and can the runtime explain and replay the
result from complete inputs?

## 3. Give every fact one semantic owner

UI-session state, renderer mechanics, host observations, and authoritative
product truth have different lifecycles and trust boundaries. Duplication may
exist as an observed copy or provisional overlay; authority may not be
ambiguous.

**Ask:** Who can change this fact, who only observes it, and what declared
mechanism or versioned contract reconciles copies?

## 4. Keep the core small and the surface domain-aware

The core should use the least sufficient computational power. The authoring
surface may provide higher-level frontend concepts when their lowering and
ownership are precise.

When ownership of a layer is materially at issue, compare independent
ownership with the smallest viable reuse, adoption, or no-new-layer
alternative in proportion to the decision's scope. Independent ownership must
justify its marginal lifetime cost; reuse must preserve the relevant product
and semantic guarantees rather than import ambient authority.

**Ask:** Could a smaller declarative construct, existing semantic model,
library, or checked profile serve the demonstrated problem? What measurable
value does Uhura ownership create, and does it justify the marginal lifetime
cost? If the proposal is high-level, what exact semantic runtime, reusable
capability, renderer, host, or external-boundary contract does it denote?

## 5. Compress semantics, not spelling

Compact source is valuable when it exists because Uhura understands more of
the domain. Short syntax that hides state, order, failure, or authority moves
complexity rather than removing it.

**Ask:** Which repeated concepts or invalid combinations disappear? What
behavior becomes implicit, and can an author still inspect, override, and
diagnose it?

## 6. Read locally and canonically

Human and agent authors should encounter one regular grammar, canonical
formatting, stable names, explicit scope, and structured diagnostics. Similar
forms should have similar meaning.

**Ask:** Can a reader predict this construct from nearby source and the core
taxonomy, or must they recover hidden global context and special cases?

## 7. Make every concept and owned layer pay for its topology

A new construct or project-owned layer affects grammar, semantics, static
representation, runtime, renderers, hosts, tooling, ecosystem coupling,
teaching, and compatibility. Its value must be considered against the total
system, not only the demonstration that motivated it.

**Ask:** Is this one orthogonal concept or several bundled together? Does it
replace more conceptual weight than it adds, and where else could it live?
Which requirement belongs to the semantic contract, source, compiler,
runtime, renderer, or host?

## 8. Make the common correct path the shortest

Uhura exists to author interfaces quickly. Defaults should include the
system-owned accessibility and interaction behavior needed for a credible
result. The checker should require information only the author can provide,
and product policy should remain explicit.

**Ask:** As applicable, what does the smallest realistic example do for
keyboard and touch input, accessibility, localization, latency, failure,
cancellation, and reduced motion? Which concerns can the system safely
default, which information must the checker require, and which product
decisions need explicit source?

## 9. Keep physical mechanics at the boundary

Layout, hit testing, pointer arbitration, frame interpolation, caret and IME,
physical scrolling, and device I/O need specialized runtimes. Uhura should
promote their semantic meaning, not absorb their implementation loops.

**Ask:** Which observation changes experience behavior and must cross into
the semantic runtime? Which high-frequency or platform-specific mechanics
remain owned by the renderer or host?

## 10. Treat prior art as transferable evidence

Formal work, standards, shipping implementations, convergence, ergonomic
studies, and measured adoption answer different questions. Product
documentation proves availability, not use or preference. A popular feature
proves neither semantic correctness nor fit for Uhura.

**Ask:** What problem did the precedent solve, under what ownership and runtime
assumptions, what failed or was abandoned, and which of those assumptions
transfer here?

## 11. Prefer explicit limits to false universality

Renderer-neutral does not mean every renderer behaves identically. Checked
capabilities, fallbacks, and unsupported states are more honest than silent
degradation.

**Ask:** What can the program rely on across targets? How is absence detected,
what fallback preserves meaning, and when should checking fail?

## Evidence expected for a substantial change

A language proposal should contain, in proportion to its scope:

1. a demonstrated problem and explicit non-goals;
2. realistic examples and at least one adversarial counterexample;
3. ownership and lifecycle analysis;
4. a formal or operational model when behavior is involved;
5. its lowering or boundary contract;
6. static checks, diagnostics, and runtime consequences;
7. viable alternatives at the relevant ownership and boundary layers, in
   proportion to the proposal's scope;
8. primary prior-art sources with claims classified as formal, product or
   implementation, convergence, ergonomic, adoption, or transfer evidence;
9. readability and semantic-compression evidence on representative tasks;
10. accessibility and static-preview consequences;
11. compatibility, migration, and capability-negotiation consequences; and
12. conformance cases that would distinguish correct implementations.

The evidence can begin in a disposable study. A durable design decision
belongs in an RFC; observable behavior belongs in named version documentation
and its conformance suite.

## Review outcome

A review should conclude with one of these explicit outcomes:

- no change is justified;
- clarify documentation or diagnostics;
- provide a reusable pattern or capability;
- add or change a renderer or host contract;
- continue a bounded study or prototype;
- propose an RFC for a language or runtime change; or
- revise the doctrine because the evidence changed the design center.

“Useful” does not automatically mean “core language,” and “not core language”
does not mean “not valuable.”
