# Uhura studies

- **Status:** Stable, non-authoritative research router
- **Lifetime:** Stable index; individual studies are disposable
- **Authority:** Navigation only; contains no language semantics
- **Doctrine:** [Uhura doctrine](../doctrine/README.md)
- **Specification router:** [Uhura specifications](../spec/README.md)
- **Decision history:** [Uhura RFCs](../rfcs/README.md)

This directory holds evidence, experiments, surveys, candidate formal models,
syntax sketches, implementation feedback, and unresolved questions. Uhura has
no standing language committee or working-group authority.

Individual study documents may be revised, merged, abandoned, or deleted.
They cannot reserve syntax, establish doctrine, define current behavior, or
require migration. A durable RFC must summarize the evidence needed to
understand its decision; neither doctrine nor accepted version documentation
may depend on a study leaf remaining available.

## Study method

A substantial study should include, in proportion to its scope:

1. the problem and explicit non-goals;
2. ownership and authority analysis;
3. a small formal or operational model where behavior is involved;
4. realistic examples and adversarial counterexamples;
5. static checking and runtime consequences;
6. renderer, host, and external-authority boundary effects;
7. migration and compatibility consequences; and
8. candidate conformance cases if the work advances.

The [design principles](../doctrine/principles.md) provide the canonical review
questions. A study may challenge them with evidence, but it must not silently
create competing doctrine.

## Language scope and alternatives

- [Language necessity and surface reuse](language-necessity-and-surface-reuse.md)
  asks whether Uhura needs independently owned syntax, a checked profile over
  familiar notation, an adopted kernel, or no new language layer.
- [Machine-first language, opt-in UI, and explicit framework features](machine-first-language-and-opt-in-ui.md)
  recommends that Uhura make its core independently usable as a state-machine
  language without a renderer, then admit web UI vocabulary through an
  explicit, checked extension named `ui` and meta-framework semantics through
  feature-by-feature imports; the exact activation and import syntax remain
  open.
- [Transactional state-machine language prior art](transactional-state-machine-language-prior-art.md)
  compares deterministic Mealy reactions, bounded functional automata, atomic
  write logs, reducer-command architectures, and communicating transitions
  without accepting any candidate model.
- [Visual state-machine authoring and deterministic simulation prior art](visual-state-machine-authoring-prior-art.md)
  treats visual representation, state-machine topology, deterministic
  evaluation, and replayable execution as separate axes across game engines,
  industrial controllers, and simulation runtimes.
- [Escape hatches and foreign bindings](escape-hatches-and-foreign-bindings.md)
  records the established need for explicit foreign integration while leaving
  its taxonomy, syntax, trust model, execution semantics, and implementation
  open.

The Markdown-only [program harnesses](../../examples/programs/) provide the
pure L0–L2 problem corpus. The parallel
[application harnesses](../../examples/applications/) begin with A0 Return
Desk, which tests practical composition with `ui` and explicit application
features. Neither corpus contains accepted candidate implementations or has
language authority.

## Research inputs

- [Application-scale stress-test requirements](application-scale-stress-test.md)
  preserves reusable requirements from an earlier application-scale study
  without preserving its syntax.
- [Database-bound state in client applications](db-bound-state-survey.md)
  surveys how shipping client stores relate to external data authority.
- [Client state architecture in the wild](client-state-survey.md) surveys
  recognizable frontend state patterns and use cases across ecosystems.

## Candidate models and implementation feedback

- [A class-differentiated state IR](state-ir-proposal.md) is an unaccepted
  candidate model derived from the surveys.
- [Instagram v0 spike design](instagram-spike-design.md) records the completed
  historical language/runtime topology that guided the implementation spike;
  it is evidence rather than current redesign guidance.
- [Instagram demo dogfood](instagram-demo-dogfood.md) records feedback from
  exercising that topology.
- [Referential example data and read-only preview provenance](referential-example-data-and-read-only-provenance.md)
  records one preview-model experiment.

These entries are an inventory, not a roadmap. The index remains as a stable
router even when its contents change. Removing a leaf means only that the
current tree no longer needs the study; its history remains in Git.
