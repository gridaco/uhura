# Mission and identity

- **Status:** Durable working doctrine
- **Lifetime:** Version-independent; explicitly revisable by evidence
- **Theme:** Philosophical and technical
- **Authority:** Mission and design center, never syntax or runtime law
- **Review rubric:** [Design principles](principles.md)
- **Human model:** [Authoring ergonomics](authoring.md)

## Thesis

Uhura is a frontend-dedicated, user-facing builder system built on a
standalone deterministic state-machine language.

The product is optimized for authoring Web interfaces. The core language does
not require presentation, a renderer, or a widget catalogue in order to define
and execute a complete program. Web UI remains Uhura's first-class application
domain through an explicit profile. This distinction is fixed by
[RFC 0004](../rfcs/0004-standalone-machine-core-and-source-composition.md):
product focus and core-language dependency are different design decisions.

Uhura's durable design hypothesis is that an interactive experience can be
understood through explicit state, causes, transitions, declared boundary
interactions, and derived presentation. In product language, **Uhura is a
state-machine builder**. The hypothesis should be tested through each language
generation, not protected from contrary evidence.

Uhura's identity rests first in the programs and behavior it admits, not in
inventing distinctive spelling. Concrete syntax and implementation layers are
means rather than the product identity. In every named version, authored
observations, state changes, and boundary interactions must be admitted only
through specified, checkable forms governed by that version's semantic
contract.

The implementation language is not the authoring language. Lowering a closed
program to JavaScript or another general-purpose target does not grant that
target's ambient powers to authors. Conversely, removing an explicit script
block does not establish determinism when mutation, effects, callbacks, or
host authority remain available elsewhere.

This does not require a finite state enumeration, one particular statechart
notation, or one event-processing algorithm. Nor does it imply that authors
should manually encode every gesture sample or animation frame as a
transition. A named version must supply the exact operational model; doctrine
requires that the model be explicit, deterministic over its declared inputs,
checkable, and honest about external nondeterminism.

When a program includes presentation, it is computationally downstream of
experience state and declared inputs. It matters enormously to the product,
but it is neither a prerequisite of the core language nor an independent
behavior authority. A version may change its view syntax or rendering protocol
without changing this separation.

## The enduring separation

| Plane | Enduring role | Boundary |
|---|---|---|
| **Owned experience behavior** | Holds the experience facts for which Uhura is responsible and explains how declared causes may change them | Does not acquire authority over external product truth |
| **Derived presentation** | Describes what the current experience means to a renderer and how semantic interaction returns | Does not become an independent mutation or effect authority |
| **Explicit boundaries** | State what the experience requires or requests from systems outside itself | Do not absorb the mechanics or authority of those systems |

**Behavior** and **view** are useful conceptual names for the first two planes,
but doctrine does not reserve source sections, keywords, or IR kinds. Style,
interfaces, ports, commands, intents, projections, and widgets are possible
version-level realizations, not permanent doctrine taxonomy.

## The product bet

Uhura makes five connected bets:

1. An explicit behavioral model makes interface behavior more checkable,
   replayable, portable, and understandable.
2. A closed machine model plus an explicit frontend profile may express
   recurring intent more compactly and checkably than a general-purpose
   language plus libraries.
3. Declarative presentation can remain independent of DOM, native-widget, and
   canvas object models.
4. Good defaults can make the shortest program accessible and operationally
   credible without making the underlying contract mysterious.
5. Canonical source and structured diagnostics can serve both human authors
   and software agents.

The language therefore optimizes for **fast, truthful authoring**. A prototype
should be quick to express, but its source must not claim semantics the runtime
cannot preserve or conceal product authority the frontend does not own.

## Formal honesty and frontend ergonomics

Every substantial design is reviewed on two axes.

### Formal honesty

- Is behavior deterministic for complete declared inputs?
- Does that guarantee follow from accepted-program semantics rather than an
  implementation language or author discipline?
- Is every change causally explainable from declared inputs and semantics?
- Is each fact owned by one semantic authority?
- Are external effects and platform mechanics kept behind explicit
  boundaries?
- Can the construct be checked, lowered, replayed, and tested?
- Is the smallest sufficient contract independent of one renderer or host?

### Frontend ergonomics

- Does the source express the user's intent directly?
- Is the common correct path short and locally understandable?
- Does the abstraction remove recurring bookkeeping rather than merely hide
  it?
- Does the system safely default what it can, require the accessibility and
  localization information only the author knows, and leave product-specific
  latency or failure policy explicit?
- Can a static design select a meaningful experience state without simulating
  incidental mechanics?

Neither axis automatically wins. Requiring authors to encode a swipe as raw
pointer transitions may be mathematically possible and still be bad language
design. Hiding a network mutation inside a visual property may be convenient
and still be semantically dishonest. Uhura's work is to find the smallest
honest semantic concept that also matches how interfaces are actually built.

## Small core, domain-aware surface

Uhura should prefer the least powerful core that can express its mission. The
[W3C Rule of Least Power](https://www.w3.org/2001/tag/doc/leastPower) argues
for choosing the least powerful language suitable for a purpose because more
power makes information harder to analyze and reuse.

This does not require a primitive authoring surface. A high-level construct is
welcome when it:

1. captures a recurring frontend concept;
2. has a precise static and runtime contract in its version;
3. has a deterministic explanation in the version's core model or an explicit
   external capability;
4. removes conceptual repetition for authors; and
5. preserves an escape path where real platform differences matter.

The result may belong in source syntax, a reused or adopted model, a reusable
capability, a pattern, an external contract, or no Uhura-owned feature at all.
Adding syntax is not the default definition of progress.

## Precedent is evidence, not authority

Uhura should learn from formal work and shipping UI ecosystems without
confusing their evidence. Mealy's
[sequential-machine model](https://doi.org/10.1002/j.1538-7305.1955.tb03788.x),
Harel's [statecharts](https://doi.org/10.1016/0167-6423(87)90035-9), the
[W3C SCXML model](https://www.w3.org/TR/scxml/), and
[The Elm Architecture](https://guide.elm-lang.org/architecture/) support the
state-and-transition design hypothesis without fixing Uhura's operational
semantics. [Authoring ergonomics](authoring.md) records how notation, product,
and adoption evidence should be evaluated.

A source can demonstrate formal soundness, implementation feasibility,
product availability, ergonomic value, or measured adoption. It rarely proves
all of them. Popularity can justify studying a workflow; it does not prove that
another system's syntax, ownership model, or runtime boundary transfers to
Uhura.

## Non-goals

Uhura is not intended to become:

- a general-purpose application language;
- an embedded JavaScript replacement;
- an authoritative backend or database model;
- a renderer, layout engine, or device-driver API;
- a collection of unrelated UI conveniences with no common semantics; or
- a perfectly minimal calculus that makes ordinary interface work painful.

The doctrine succeeds when Uhura remains explainable as one coherent system
while letting authors express real interfaces with unusually little,
unusually clear source.
