# Current Uhura implementation map

- **Status:** Live, non-normative contributor guide
- **Lifetime:** Replaceable with the implementation topology it describes
- **Language authority:** [Uhura 0.4 incubation candidate](../spec/drafts/0.4/)
- **UI contract:** [Uhura 0.4 checked UI catalogue](../widgets/drafts/0.4/)
- **Conformance:** [Uhura 0.4 implementation gates](../spec/drafts/0.4/conformance.md)

This page answers one maintenance question: where should a change live in the
current repository? It does not define language behavior, freeze crate
boundaries, or make an implementation detail part of the 0.4 candidate.
Observable behavior remains owned by the versioned language and UI documents
above.

## Data flow and ownership

```text
filesystem and host policy
  -> source capture, manifest, lock, and resource admission
  -> pure syntax and project compilation
  -> Program
       MachineProgram          source-neutral executable core
       presentations/evidence application artifacts
  -> deterministic machine runtime
  -> pure semantic projection
  -> host/editor read models and Wasm protocols
  -> browser reconciler
  -> per-primitive browser adapters
```

The authored module graph may be split across files and packages. Compilation
resolves that graph into one semantic program. `MachineProgram` is the
I/O-free state-machine artifact used by typed validation and execution.
`Program` owns that machine artifact plus presentation, evidence, and routing
material. Its flattened `uhura-ir/1` serialization is the current aggregate
wire protocol; the flattened bytes do not erase the implementation ownership
boundary.

| Layer | Current owner | Owns | Must not own |
|---|---|---|---|
| Shared foundation | [`uhura-base`](../../crates/uhura-base/) | Exact values, canonical serialization and hashing, spans, and diagnostics | Source grammar, filesystem policy, browser behavior |
| Source frontend | [`uhura-syntax`](../../crates/uhura-syntax/) | Lexing, source-spanned ASTs, parsing, UI phrase recognition, and formatting | Element availability, type checking, runtime effects |
| Static semantics and lowering | [`uhura-check`](../../crates/uhura-check/) | Name resolution, types, machine checks, the current UI catalogue, lowering, provenance, and the pure 0.4 project compiler | Filesystem discovery, HTTP, DOM mechanics |
| Machine core | [`uhura-core`](../../crates/uhura-core/) | `MachineProgram`, deterministic reactions, typed values, receipts, checkpoints, evidence execution, and pure projections | Parsing, project discovery, browser APIs |
| Foreign boundary | [`uhura-port`](../../crates/uhura-port/) | Typed contract admission and standard route/port vocabulary | Provider I/O or an adapter implementation |
| Host and CLI adapters | [`uhura-host`](../../crates/uhura-host/), [`uhura-cli`](../../crates/uhura-cli/) | Filesystem/resource admission, coherent builds, deployment selection, last-good publication, transport, and commands | A second parser, checker, runtime, or widget catalogue |
| Editor read model | [`uhura-editor-model`](../../crates/uhura-editor-model/) | Versioned browser-neutral inspection and preview data derived from checked artifacts | Source evaluation or machine execution |
| Wasm adapter | [`uhura-wasm`](../../crates/uhura-wasm/) | Lossless browser boundary around the canonical machine runtime and projection | A browser-specific execution model |
| Browser application | [`web/src/`](../../web/src/) | Protocol decoding, Editor and Play UX, DOM reconciliation, browser mechanics, and styling | Language admission or semantic recovery from invalid source |
| Cross-layer acceptance | [`uhura-tests`](../../crates/uhura-tests/) | Tests that prove the maintained frontend, checker, core, host, and browser-facing contracts compose | An alternate fixture runtime |

### Boundary health

| Seam | Current strength | Maintenance consequence |
|---|---|---|
| Machine/application | Explicit owned `MachineProgram` inside `Program`; runtime and typed-value APIs target the machine artifact. Both artifacts still share `uhura-core`, `ir.rs`, and a flattened wire protocol. | This is an ownership-visible, compiler-enforced consumer seam inside one crate. Machine-only code accepts `MachineProgram`; application code accepts `Program`. |
| Core/UI frontend | Core grammar and UI phrase parsing are separate modules; UI admission uses one current checker catalogue. | UI vocabulary can evolve without changing the reaction runtime. A change to shared expressions or lowering still crosses both frontend layers. |
| Checker/browser catalogue | Rust owns semantics; a small JSON list and explicit TypeScript registry prove adapter coverage. | Primitive work has one reviewable cross-language seam instead of scattered element switches. |
| Compiler/admission | CLI and host share one pure 0.4 compiler after their own source and resource capture. | I/O policy remains adapter-owned while parser/checker drift is prevented at the language boundary. |
| Host/editor/browser | Protocol and ownership boundaries are explicit, but host orchestration remains physically concentrated. | Keep new semantics out of the host; split host modules only as behavior-preserving maintenance. |

### Compiler boundary

[`compile_v04_project`](../../crates/uhura-check/src/v04_compile.rs) is the
canonical pure 0.4 frontend service after source admission. It receives an
already admitted manifest, exact dependency captures, and source bytes; it
parses, resolves, checks, lowers, and returns deterministically ordered
diagnostics and provenance. CLI and host code may differ in how they capture
I/O, but they must converge on this service rather than copy the language
pipeline.

Resource-backed checks deliberately follow pure compilation. For example,
hosts load the checked icon-font registry and run
[`icon_token_diagnostics`](../../crates/uhura-check/src/icon_fonts.rs) against
the returned program before publication or execution. A renderer is not an
error-recovery boundary for an unknown family, unknown glyph, or unbounded
icon name.

### UI boundary

UI phrase structure is parsed under
[`uhura-syntax/src/v04/ui.rs`](../../crates/uhura-syntax/src/v04/ui.rs), while
the finite 0.4 semantic vocabulary lives in
[`uhura-check/src/ui_catalog/elements.rs`](../../crates/uhura-check/src/ui_catalog/elements.rs).
That catalogue owns element availability, attributes, content models, events,
payloads, semantic classifications such as interactivity, and static
constraints including the neutral list-item boundary.

The small
[`resources/ui-catalog/0.4.json`](../../resources/ui-catalog/0.4.json) file
lists only the Uhura-specific primitive adapter IDs crossing the Rust and
TypeScript boundary. It does not duplicate the semantic catalogue. Browser
realization of those IDs is registered in
[`web/src/renderer/primitives/registry.ts`](../../web/src/renderer/primitives/registry.ts);
each adapter owns its physical element, attributes, element-specific event
mechanics, and cleanup. The generic
[`projection.ts`](../../web/src/renderer/projection.ts) reconciler owns keyed
tree lifecycle, common event dispatch, and delegation through that interface.

Shared primitive presentation belongs in
[`primitives/base.css`](../../web/src/renderer/primitives/base.css). Editor-
or Play-only chrome stays in its respective surface. A new primitive should
not introduce another name-based switch in the checker, reconciler, Editor, or
Play shell.

## Change routes

Use the smallest route that covers the semantic change.

| Change | Required owners and evidence |
|---|---|
| Machine semantics or IR | Update the owning 0.4 kernel/source document, checker lowering, `MachineProgram`/runtime code, focused core tests, and native/Wasm conformance where the wire or behavior changes. UI and host code should remain untouched unless their declared interface changes. |
| Core source syntax | Update the 0.4 source document and grammar together, then lexer/parser/AST/formatter, semantic checking, exact diagnostics, and at least one harness or negative fixture. Do not make the host parse syntax. |
| Project composition or identity | Update the 0.4 project/source documents, resolution and canonical compiler service, CLI/host admission adapters, identity/provenance tests, and source-layout equivalence fixtures. |
| UI syntax only | Update the 0.4 application document, the v0.4 UI parser/formatter, and parser/checker tests. Element semantics still belong to the catalogue. |
| Element, attribute, event, or content rule | Update the executable 0.4 checker catalogue, its focused checker tests, this version's catalogue page, and conformance coverage. Add or change a browser adapter only when physical realization changes. |
| Uhura browser primitive | In the same patch, update the checker catalogue realization class, browser parity JSON, one adapter under `web/src/renderer/primitives/`, shared primitive CSS when needed, and Rust/TypeScript parity plus behavior tests. |
| Native HTML realization | Keep semantics in the checker catalogue and generic projection. Do not create an adapter merely to repeat the platform element without an Uhura-owned lifecycle. |
| Host or provider capability | Keep authority and I/O in the host/provider boundary, use typed port contracts, and cover admission plus failure behavior. Do not add ambient authority to `use` or the machine core. |
| Editor inspection or UX | Derive a versioned read model from checked artifacts, update `uhura-editor-model`, host serialization, protocol decoding, and Editor tests. Do not evaluate source or duplicate the runtime in TypeScript. |
| Diagnostic behavior | Register stable identity in [`uhura-base/src/codes.rs`](../../crates/uhura-base/src/codes.rs), preserve source spans and structured notes/fixes through syntax/check/CLI/host, and assert the public diagnostic envelope. |

## Maintenance rules

- Treat `Program::machine_program` as the core/application seam. Runtime and
  typed-value operations belong on `MachineProgram`; presentation, routing,
  and evidence orchestration belong on `Program` or an application layer.
- Keep filesystem traversal, symlink policy, file reads, HTTP, and DOM access
  outside the syntax, checker, and machine core.
- Keep semantic validation before publication. Browser adapters may assert a
  checked contract defensively, but they must not make invalid source appear
  valid.
- Keep one authored frontend and one project admission path. Historical source
  belongs in Git history, not a hidden parser, compatibility mode, or test-only
  executable path.
- Keep protocol documents and catalogues one-way: executable owners implement
  them; transport parity files enumerate only what must cross languages.
- Do not modernize historical v0 or Relay documents into current guidance.
  Add current behavior to 0.4 owners and leave historical pages labeled as
  evidence.

[`uhura-host/src/lib.rs`](../../crates/uhura-host/src/lib.rs) currently
contains build orchestration, Editor publication, Play artifacts, and
transport in one large module. That is a known physical concentration, not
permission to add language semantics there. New language work should deepen
the compiler/core boundaries above; host-only refactors may split the module
without changing observable behavior.

## Validation by boundary

- Pure Rust and cross-layer changes:
  `cargo test --locked --workspace --all-targets`.
- Rust linting: `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- Browser and parity changes: `corepack pnpm@10.11.0 -C web check`.
- Full application changes: run the relevant L0–L2/A0 or Instagram command
  from the repository README in addition to the workspace gates.

The [conformance plan](../spec/drafts/0.4/conformance.md) owns required
language evidence. This guide only routes contributors to the implementation
layers that must supply it.
