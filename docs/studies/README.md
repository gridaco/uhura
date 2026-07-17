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
- [Instagram v0 spike design](instagram-spike-design.md) records one
  implementation-guiding language/runtime topology.
- [Instagram demo dogfood](instagram-demo-dogfood.md) records feedback from
  exercising that topology.
- [Referential example data and read-only preview provenance](referential-example-data-and-read-only-provenance.md)
  records one preview-model experiment.

These entries are an inventory, not a roadmap. The index remains as a stable
router even when its contents change. Removing a leaf means only that the
current tree no longer needs the study; its history remains in Git.
