# RFC 0002 — Model-driven Editor and saved-source live updates

- **Status:** Accepted
- **Implementation:** Implemented
- **Scope:** Read-only Editor projection, browser delivery, saved-source
  observation, diagnostics, and Editor/Play web topology
- **Supersedes:** The experimental static Canvas export and document-reload
  lifecycle
- **Related work:** [RFC 0001](0001-project-foundation.md),
  [RFC 0003](0003-source-comments-docs-and-annotations.md)

## 1. Decision

Uhura's read-only Editor is a browser application backed by a complete,
versioned native read model. The native process captures a coherent saved
project revision, checks and evaluates its examples, and publishes one
immutable `EditorState`. The browser decodes that state and owns every part of
Editor presentation and interaction.

Editor and Play are routes of one web application:

```text
Uhura source
  -> native capture, check, and evaluation
  -> uhura-editor-state/4 carrying uhura-view/1 projections
  -> HTTP state + SSE revision notification
  -> canonical projection renderer
  -> Editor at / or Play at /play
```

Rust does not generate Editor HTML, CSS, selectors, controls, or UI text. The
browser does not parse or evaluate Uhura. The two surfaces share semantic
node-to-DOM mechanics, while explicit policies keep Editor read-only and Play
interactive.

The former `uhura project` command, `--out`, `renders/canvas.html`, Rust HTML
renderer, Editor IIFE, and whole-document reload/checkpoint path are removed.
There is no standalone or no-JavaScript Canvas compatibility surface.

## 2. Motivation

The static Canvas experiment proved that checked examples can become useful
design previews and that saved changes can retain a last-known-good result.
It also put browser responsibilities on the wrong side of the boundary. Rust
assembled page chrome, HTML, CSS, selectors, and an injected controller; every
source update replaced the document, so ordinary shell state needed explicit
checkpoint and reload machinery. Play independently implemented the semantic
DOM renderer.

That topology creates two renderers, two web products, and a presentation
contract hidden inside generated markup. It makes live updates harder rather
than safer. A typed read model gives the native checker a stable, testable
output and lets the browser update preview content without remounting the
Editor shell.

This is saved-source live rebuilding, not JIT compilation or state-preserving
Play HMR. Play session migration remains a separate, lower-priority problem.

## 3. Responsibility boundaries

### 3.1 Native language and model layers

Parsing, checking, lowering, static example evaluation, structured
diagnostics, semantic view trees, and renderer-neutral identities stay native.
The Editor model builder transforms one checked result into deterministic
preview groups, preview identity and content, example data and provenance,
interaction summaries, compiled application CSS, and assets.

It emits semantic data only. Prepared DOM, browser layout, and Editor chrome
are forbidden in this layer.

### 3.2 Native host

The host owns coherent filesystem capture, debounced observation, candidate
ordering, model construction, last-renderable retention, atomic publication,
the Editor event stream, Play artifacts, provider endpoints, and static serving
of the built web application.

It serves the same entry document for `/` and `/play`; browser routing chooses
the surface. APIs live below `/api/editor/*` and `/api/play/*` so they cannot
collide with application routes.

### 3.3 Browser application

The browser owns routing, Editor shell and panels, frames, search, camera,
tools, selection, inspector presentation, diagnostics, complete-state
replacement, Play chrome, and runtime controls. The application remains
mounted during a model update, preserving camera and selection state without a
reload checkpoint.

Concrete icon geometry is browser-renderer data. Native model and host layers
may carry checked icon-name tokens inside semantic content, but never SVG
paths, font codepoints, or glyph-family tables.

The canonical projection renderer exposes two policies:

- **Editor:** static realization, inert controls, no binding delivery,
  no provider effects, and safe static media.
- **Play:** keyed reconciliation, runtime binding delivery, focus,
  scrolling, surfaces, media, and other existing effects.

The Editor mounts the projection in an inert subtree and supplies no runtime
binding delivery path.

## 4. `EditorState` contract

The initial browser-facing protocol was `uhura-editor-state/0`. The RFC 0003
implementation advanced the contract to `uhura-editor-state/1`, adding
render-owned authoring metadata, explicit preview documentation references,
and source-target occurrences. Version 2 removed concrete icon geometry from
the native read model; glyph realization belongs entirely to the browser
renderer. Version 3 added tagged render content, nullable per-preview evidence,
and nullable machine inspection. The current `uhura-editor-state/4` removes
the retired snapshot/fragment payloads and structural path anchors. Every
preview now carries exactly one `uhura-view/1` projection plus its
projection-source sidecar, and every provenance anchor is an opaque
rendered-node key. EditorState remains an Editor read model, not canonical
language IR.

```text
EditorState
  protocol
  sourceRevision
  diagnostics
  render?
    revision
    freshness: current | stale
    application
    groups[]
      previews[]
        identity, kind, content
        example data and provenance
        interaction summaries
    stylesheet
    assets
```

The following invariants apply:

1. `sourceRevision` identifies the candidate that produced `diagnostics`.
2. `render.revision` identifies every value inside the render payload.
3. A renderable candidate normally has equal source and render revisions and
   `freshness: current`.
4. A rejected candidate may carry its diagnostics together with the previous
   render, explicitly marked `freshness: stale`.
5. An initially invalid project has `render: null`; the web application still
   loads and presents its diagnostics.
6. A response is one complete state. The contract does not split or
   patch independently mutable fragments.
7. Ordering and serialization are deterministic so fixtures can enforce the
   boundary.
8. Icon tokens may occur in semantic preview content, but glyph paths,
   codepoints, and family assets never occur in EditorState.

The browser rejects an unknown protocol or malformed state before replacing
the currently displayed model.

## 5. Publication and recovery

Each coherent source capture receives a monotonically increasing revision.
Build work may overlap, but an older result may never activate after a newer
candidate. Publication swaps a complete state atomically.

The Editor first fetches `/api/editor/state`. An SSE notification from
`/api/editor/events` announces that a newer candidate exists; it does not carry
model fragments. The browser fetches, decodes, prepares, and then swaps the
complete replacement.

The required lifecycle is:

- valid to valid: new previews replace old previews without process or
  document restart;
- valid to invalid: old previews remain, are identified as stale, and current
  diagnostics appear separately;
- invalid to valid: a complete current render replaces the stale or absent
  render automatically;
- rapid saves: only the newest eligible result activates.

Editor replacement does not restart or mutate a Play session.

## 6. Build and distribution

`web/src/` is authoritative. Vite produces one application build for both
routes, and generated `web/dist/` output is ignored by Git. CI installs the
pinned frontend toolchain, checks and builds the web source, and then runs
native integration tests. Release packaging builds the web application and
Wasm before placing them alongside the native executable.

Node and Vite are build-time dependencies only. A packaged native host serves
the compiled files unchanged. A source-built CLI may report a clear missing
web-assets error when a browser command is requested, but language-only
commands must continue to work without Node or a running frontend server.

During web development, Vite owns the browser port and proxies `/api` to the
native host. The native process remains authoritative for filesystem and
language work.

## 7. Non-goals

This RFC does not introduce:

- source editing, mutation, undo, or persistence;
- incremental compiler or model-patch protocols;
- collaborative or persistent Editor state;
- server-side rendering or hydration;
- standalone static Canvas export;
- Play runtime-state migration or HMR;
- new Uhura syntax, evaluation semantics, or checker rules; or
- a requirement to adopt a JavaScript UI framework.

## 8. Conformance

Implementation acceptance requires tests proving:

- deterministic contract serialization and runtime protocol rejection;
- revision consistency, current/stale/null-render states, and all preview
  kinds and inspector data;
- shared platform structure under both renderer policies and complete Editor
  inertness;
- valid → invalid → valid host recovery and resistance to out-of-order builds;
- Editor shell state surviving complete model replacement;
- hard-load and client navigation for `/` and `/play`;
- preserved Play fixture/configured-provider behavior; and
- absence of production references to `canvas.html`, `render_canvas`,
  `node_html`, `canvas-chrome.js`, tracked `web/dist`, or the removed CLI
  surface.
