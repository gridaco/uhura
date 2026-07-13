# Uhura documentation

- **Status:** Incubating
- **Master specification:** [Uhura specification](spec/README.md)
- **Foundational proposal:** [RFC 0001](rfcs/0001-project-foundation.md)
- **Research and stewardship:** [Uhura Working Group](working-group/README.md)
- **Historical requirements input:** [Frame stress-test handoff](working-group/frame-stress-test-handoff.md)

This directory is the documentation root for the standalone Uhura project.
The project landing page provides orientation; the living specification defines
the current target; accepted RFCs record durable decisions and rationale.

## Authority

Uhura does not yet have an accepted language or runtime version. Until it does,
all normative-looking statements are proposed requirements.

Once versioned work begins, authority will follow this order:

1. Accepted RFCs establish durable decisions.
2. A versioned specification and executable conformance suite define observable
   language and runtime behavior.
3. Implementations conform to those artifacts; implementation behavior is not
   specification by accident.
4. Renderer, host-driver, widget-catalog, message, Spock-binding, and bundle
   contracts remain independently versioned even when maintained here.
5. NCC links compatible artifacts but cannot override their semantics.

Examples are non-normative unless a specification explicitly promotes them to
conformance fixtures.

## Planned document families

Only the foundation is present today. Future RFCs may establish:

- source grammar, module rules, and canonical formatting for `.uhura` source;
- the checked intermediate representation and extraction model;
- UI machine semantics and event processing;
- the headless core runtime ABI and snapshot format;
- semantic widget catalogs and renderer conformance;
- host capabilities and effect-driver protocols;
- Spock contract imports and linking;
- messages and localization, including whether to adopt MessageFormat 2;
- bounded static projection for NCC's infinite canvas; and
- implementation language, packaging, compatibility, and release policy.

These topics are intentionally not resolved merely by creating a directory.
