---
name: uhura-lang
description: Create, modify, check, format, trace, render, play, and debug current Uhura declarative experience projects, including `.uhura` pages, components, surfaces, examples, port contracts, fixtures, scripts, providers, Canvas previews, and Spock-backed Play integration. Use when translating product experience requirements into Uhura, changing an existing Uhura project, investigating diagnostics or traces, or verifying UI-session behavior and authority boundaries.
---

# Uhura Language

Build deterministic, checkable experience programs while preserving the boundary between disposable UI-session state and authoritative product truth.

## Select the current authority

- Treat `docs/working-group/instagram-spike-design.md` and the executable Instagram corpus as the current implemented design authority.
- Treat `docs/spec/README.md` as the product boundary and conceptual model, not a frozen source grammar.
- Run `uhura check` and `uhura fmt --check` instead of assuming draft prose is accepted.
- Inspect the checker, syntax crate, or existing corpus when a diagnostic or grammar detail is ambiguous.
- Do not invent syntax from open decisions, deferred registers, or conceptual state-IR proposals.

Read the bundled references only when needed:

- Read [references/source-language.md](references/source-language.md) before authoring or substantially changing `.uhura` source.
- Read [references/project-and-providers.md](references/project-and-providers.md) when working on manifests, locks, ports, fixtures, examples, providers, or Spock integration.
- Read [references/workflows.md](references/workflows.md) when creating a project, modifying an existing project, repairing a failure, or deciding what evidence proves completion.
- Read [references/tooling-and-limits.md](references/tooling-and-limits.md) for CLI workflows, tests, Editor, Play, Canvas, browser assets, or current limitations.

## Start with ownership

Before writing source, classify every fact:

- Put durable records, authorization, accepted mutations, transactions, shared workflow, and cross-device truth in Spock or another authority.
- Put selected tabs, local drafts, pending flags, optimistic overlays, notices, logical navigation, and mounted surfaces in Uhura.
- Leave pixels, layout measurement, physical scroll, pointer mechanics, native playback state, clocks, URL/history execution, network, files, and device I/O to renderers or explicit host/provider seams.
- Never make the same fact authoritative in both Uhura and its provider.

If discarding the UI session could corrupt product truth or another actor's reality, move that state out of Uhura.

## Follow the implementation workflow

1. Locate the project root and read `uhura.toml`, `uhura.lock`, the relevant page/component/surface, its examples, imported port contracts, fixture slices, and trace scripts. Run the healthy baseline before editing when one exists.
2. Decide the authoritative projections and commands before authoring UI behavior. Add or change a port contract only when the experience genuinely requires a typed external seam.
3. Choose the correct source kind: page for a route, component for reusable presentation, or surface for a modal/sheet/popover-style mounted experience.
4. Define semantic events and typed props first. Add only reconstructible UI-session state to page or surface `store` blocks.
5. Write guard-complete handlers. Make pending, optimistic, accepted, refused, unavailable, retry, dismissal, and navigation behavior explicit.
6. Render through catalog semantics and stable keys. Keep CSS responsible for layout and appearance, not product or interaction truth.
7. Add pinned examples for meaningful static states and derived examples for reachable interaction states. Preserve `from`, projection inputs, events, and notes as useful provenance.
8. Run the formatter and checker. Fix the first deterministic diagnostic before interpreting later failures.
9. Run a focused trace for every changed interaction path. Inspect guards, writes, commands, intents, drops, and final view/state hashes.
10. Build or serve the read-only Canvas to inspect all affected previews, then use Play with the intended provider for live behavior.
11. For Spock-backed work, verify both sides: authoritative data/commands in Spock and disposable experience behavior in Uhura.
12. Report exact commands, diagnostics, preview/trace changes, provider used, and any deferred runtime gap exposed by the task.

## Preserve deterministic semantics

- Use path-defined files and explicit `use` imports. Keep the import graph acyclic and do not import pages.
- Keep component behavior as typed emits over props. Put state machines in pages and surfaces, not hidden renderer callbacks.
- Use only the five current store statements: `set`, `send`, `open-surface`, `dismiss`, and `navigate` variants.
- Dispatch one external event per core step. Do not invent internal queues, lifecycle callbacks, timers, randomness, ambient I/O, or host-language escape code.
- Use guards to suppress duplicate commands. Treat a command as pending until its typed outcome arrives.
- Model optimism as an authored overlay over projection truth. Clear or roll it back on settlement; never treat the optimistic value as accepted authority.
- Handle command `.ok` and `.err` paths and projection `loading`, `failed`, and `ready` availability where required.
- Keep logical navigation in Uhura. Use push, `replace`, and `back` according to product meaning; do not fake peer-tab navigation with deep push history.
- Mount a surface with `open-surface` and close it with `dismiss`. Keep its opener, ownership, modality, and focus restoration visible in traces.
- Keep semantic element events eligible according to the catalog. Do not attach arbitrary handlers to renderer-specific markup.

## Validate proportionally

At minimum for source changes:

```sh
cargo run --locked -p uhura-cli -- fmt examples/instagram-uhura --check
cargo run --locked -p uhura-cli -- check examples/instagram-uhura --deny-warnings
```

For a behavior change, also run a focused trace:

```sh
cargo run --locked -p uhura-cli -- trace examples/instagram-uhura \
  --script=like-refused --expanded
```

For Canvas or Play work, verify the actual rendered surface after check/trace. Do not use a screenshot as the only behavioral proof.

## Keep the current product honest

- The Editor is a read-only deterministic Canvas; it does not edit source or live Spock data.
- Examples, fixtures, and scripts are deterministic design/test artifacts, not alternate product authority.
- Live Instagram Play uses the configured Spock provider. Restarting Play resets the Uhura session, not Spock data.
- Uhura does not prove backend security, service availability, aesthetics, or product desirability.
- If a requested behavior needs a deferred feature, name the gap instead of simulating support with CSS, fixture lore, or renderer-only state.
