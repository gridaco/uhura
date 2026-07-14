# RFC 0002 — Live rebuilding of static Editor previews

- **Status:** Draft
- **Scope:** Saved-project change detection, checked static-preview rebuilds,
  last-known-good Canvas activation, diagnostics, and Editor chrome continuity
- **Supersedes:** None
- **Related work:** [RFC 0001](0001-project-foundation.md),
  [Uhura specification](../spec/README.md),
  [Instagram spike design](../working-group/instagram-spike-design.md), and
  [read-only preview provenance](../working-group/referential-example-data-and-read-only-provenance.md)

## 1. Proposal

Make the read-only `uhura editor` Canvas update automatically after a saved
project change without restarting the command or manually refreshing the
browser.

The Editor will build a complete static Canvas candidate away from the active
document. A clean candidate becomes one immutable active Canvas generation. A
candidate containing an error is rejected in full: the Editor continues to
show the exact previous valid Canvas and reports diagnostics for the rejected
source revision. Once a later revision checks cleanly, its complete Canvas
generation is activated.

On activation, the browser checkpoints the small amount of Editor shell state
and reloads the complete self-contained Canvas document. The new document
restores its camera, selected tool, chrome visibility, search, and semantic
preview selection. The preview frames remain inert, static projections. Live
rebuilding does not turn them into running Uhura sessions.

This RFC deliberately does not introduce state-preserving reload for `uhura
play`.

## 2. Motivation

Static examples are the primary feedback loop in the read-only Editor. Today,
bare `uhura` first writes and reads a self-contained `canvas.html`, then holds
those bytes in memory for the lifetime of the process. The shared host watches
saved source for Play, but the Canvas does not consume those events. Authors
must restart the command after every preview change.

That restart is unnecessary. Canvas projection is already deterministic and
does not deliver effects:

```text
checked project
  -> resolved examples
  -> semantic views
  -> inert preview frames + inspector metadata + compiled styles + assets
  -> static Canvas
```

There is no running application state to migrate, no provider to reconcile,
and no pending command to retire. The only continuity requirement belongs to
the Editor shell itself. This makes static preview rebuilding a smaller and
safer problem than hot-swapping a running Play session.

Derived-example checking may perform bounded, pure transition replay and
record the commands that would have been emitted. Canvas rendering and
activation do not deliver those commands or execute live effects.

## 3. Scope and non-goals

This RFC covers:

- saved changes to every declared input used by Canvas generation;
- whole-Canvas rebuilding and validation;
- immutable candidate and active Canvas generations;
- last-known-good behavior for invalid candidates;
- diagnostics and recovery, including an initially invalid project;
- automatic browser convergence on a clean active generation;
- continuity of non-semantic Editor shell state; and
- one shared Canvas builder for hosted Editor and `uhura project` export.

This RFC does not cover:

- unsaved editor buffers or a versioned document overlay;
- incremental parsing, checking, or per-preview rebuilding;
- editable Canvas values or source mutation;
- execution of transitions, commands, providers, or network operations inside
  a preview;
- Uhura runtime checkpoints or migration of `U` and `X`;
- provider cancellation, generation fencing, or effect reconciliation;
- state-preserving Play reload or module HMR;
- redesigning which language/example errors gate a Play build; or
- live rebuilding of the Editor's own TypeScript/Rust implementation.

Those concerns may use some of the same scheduling vocabulary, but they do not
enter this proposal by implication.

## 4. Terminology and invariants

**Source revision** is the conceptual equality identity of one observed
project tree: canonical corpus-relative paths paired with content digests. It
does not claim that a general-purpose filesystem provides an atomic multi-file
snapshot.

**Canvas source snapshot** is the immutable, corpus-relative path-to-bytes map
captured for one attempt. The Editor builds only from those captured bytes and
verifies their content fingerprint is still current before activation.

**Candidate Canvas generation** is the attempted result of checking and
projecting a source revision. Its numeric generation is a process-local
ordering label, not a durable content identity.

**Active Canvas generation** is the most recent candidate successfully
activated by the Editor.

**Active Canvas build identity** is the content-derived SHA-256 identity of
the base self-contained Canvas HTML, before Editor-only host metadata and
styles are injected. The separately transported warning envelope is not part
of that hash. Browser convergence uses this value, not the process-local
generation number. A byte-identical successful candidate may advance the
numeric generation and replace its warnings without requiring a document
reload.

**Last-known-good Canvas** is the active generation retained while a newer
candidate is building or rejected.

**Editor shell state** is host-only interaction state such as pan, zoom, tool,
chrome visibility, search, and selection. It is not Uhura program state.

The following invariants are mandatory:

1. An active Canvas always comes from one completely successful candidate.
2. A rejected candidate never mutates any part of the active Canvas.
3. Markup, styles, assets, navigator entries, inspector metadata, and build
   identity shown together always belong to the same active generation.
4. Only the newest relevant source revision may activate. A slow or stale
   result cannot replace a newer result.
5. Canvas rendering and activation execute no transitions, deliver no
   commands, and perform no provider I/O, timers, randomness, or network
   activity. Bounded pure replay during checking remains permitted.
6. Editor transport and diagnostics do not become observable Uhura language
   semantics.

This feature is called **live static preview rebuilding**. It is not JIT and
does not claim stateful hot reload or module HMR.

## 5. Baseline before this RFC

Before this proposal, `uhura editor`:

1. invokes the build-only `uhura project` path;
2. writes `renders/canvas.html` unless another output directory is selected;
3. reads the generated document back into memory;
4. starts the shared Editor/Play host with those fixed Canvas bytes; and
5. tells the author to restart the command to rebuild Canvas.

The shared host separately watches a hard-coded set of `.uhura`, `.toml`,
`.css`, and `.js` files for Play. A valid Play candidate causes a full browser
reload, while an invalid candidate leaves Play on its last-good IR. The Editor
Canvas is not part of that activation path.

An invalid initial Canvas build exits the Editor process, so correcting the
source does not recover without another invocation.

## 6. Canvas build product

Canvas construction is extracted from command-side file output into a
reusable operation with this conceptual shape:

```text
build_canvas(source_snapshot) -> CanvasCandidate

CanvasCandidate =
  Ready(CanvasBundle, warnings)
  | Rejected(diagnostics)
```

`CanvasBundle` is immutable, is built from one exact source snapshot, and
carries directly or by content digest:

- its immutable Canvas build identity;
- all preview frame markup;
- compiled application styles;
- inlined assets and their manifest metadata;
- navigator and inspector metadata, including value provenance;
- stable semantic preview keys; and
- enough document metadata to produce a deterministic self-contained export.

The semantic preview key is the tuple of preview kind, qualified subject, and
example name. Generated array position or `preview-frame-N` is not identity.

The bundle may be represented internally as typed data, rendered fragments,
or one complete HTML document. That representation is an implementation
choice so long as activation preserves the invariants above.

`uhura project` and hosted Editor must consume the same successful bundle.
`uhura project` writes the deterministic self-contained HTML document and
exits as it does today. `uhura editor` keeps the bundle in memory and does not
need to write into `renders/` before hosting it.

## 7. Rebuild and activation lifecycle

The server-side build lifecycle is:

```text
idle -> building -> rejected (active unchanged)
                 \-> ready -> active
```

Each open Editor document independently converges on that state:

```text
current
stale -> checkpointing -> reloading -> current
```

Operationally:

1. A saved dependency change advances the source revision.
2. Changes are coalesced until observed file activity settles. One identified
   set of input bytes is collected for the attempt.
3. The complete Canvas candidate is checked and rendered from those bytes. If
   relevant input changes during collection or building, the result is stale
   and a newer revision is scheduled.
4. An error rejects the candidate and publishes diagnostics. The active
   generation is unchanged.
5. A clean candidate becomes ready only after every bundle member is present.
6. The server atomically makes that complete bundle the active generation and
   announces both candidate and active identities.
7. A browser showing an older bundle identity checkpoints Editor shell state and
   reloads once. The server returns the one complete active document.
8. The new document restores its shell checkpoint and reconnects. A connection
   reporting the same active bundle identity does not reload again.

Warnings do not reject an otherwise valid candidate. They remain associated
with the generation that produced them. A successful candidate whose base
Canvas HTML is byte-identical to the currently active document still advances
the process-local active generation and replaces its warning status. Its build
identity remains unchanged, so connected browsers update status over the
Editor event stream without reloading the document.

A transport such as SSE may announce status and generation changes, but the
wire endpoint and message encoding are non-normative. Each served document
contains both its process-local generation ordinal and content-derived active
bundle identity, so an event missed between response and stream connection is
detected and a restarted host cannot collide with an old document at numeric
generation `1`. Editor events must be distinguished from Play events so a
Canvas candidate cannot accidentally restart Play, and a Play-only candidate
cannot replace Canvas.

## 8. Editor shell continuity

The complete Canvas document reloads on activation. Immediately before that
reload, the Editor stores a tab-local shell checkpoint, for example in
`sessionStorage`. The next document restores it before accepting interaction.
This is a controlled host lifecycle, not migration of an Uhura program
session.

The Editor preserves:

- canvas translation and zoom;
- Cursor or Hand tool selection;
- visible or hidden chrome preference;
- navigator search text; and
- the selected preview's semantic key.

After reload, a selected preview with the same semantic key is selected again
without automatically moving the camera. If that preview no longer exists,
selection is cleared deterministically and the camera remains where it was.
An in-progress pan, pinch, or pointer gesture is not checkpointed.

This continuity is intentionally small. No state from a static preview frame
is captured because frames are inert and have no Uhura session.

## 9. Diagnostics and recovery

The canonical recovery sequence is:

```text
source A valid   -> activate Canvas A
source B invalid -> keep Canvas A; show diagnostics for B
source C valid   -> clear B errors; activate Canvas C
```

While B is rejected, the Editor must make both facts clear: the visible Canvas
is still usable, and it does not represent the latest saved source. Suggested
plain-language status is **Previewing the last valid version**.

Diagnostics retain their standard machine-readable envelope and source spans.
The Editor may summarize them in chrome, but must not rewrite their meaning.

If the first source revision is invalid, `uhura editor` still starts a stable
shell with no active Canvas, presents the diagnostics, and continues watching.
The first later valid candidate activates without restarting the command; the
diagnostic document reloads automatically into that Canvas.

Last-known-good retention is process-local in this version. An old exported
`renders/canvas.html` is not silently treated as active after a new Editor
process starts against invalid source.

## 10. Change and dependency boundary

The initial Canvas observer conservatively watches the project root rather
than relying on a permanent extension allow-list. It must detect creation,
modification, deletion, and replacement of every file that can affect Canvas
construction, including as applicable:

- `uhura.toml` and imported `.uhura` sources;
- `uhura.lock` when present;
- stylesheets and inline-style source;
- widget catalogs and imported port contracts;
- fixtures, scripts, and example data;
- asset manifests; and
- referenced image or other static asset bytes.

Tool-owned root output directories, VCS metadata, and dependency directories
are excluded from broad traversal so a rebuild cannot trigger itself. An exact
declared dependency overrides that exclusion, including a declared input under
root `build`, `renders`, or `target`. A custom `--out` path inside the observed
corpus remains visible when it is already an observed dependency; after a
successful write, the host refreshes only pre-authorized, structurally unchanged
logical aliases and exact paths created by that write, using the known artifact
digest. A new or retargeted alias, an intervening source save, and any bytes that
do not match the artifact remain observable. Rejected candidates and failed
writes do not patch the baseline. Export failure is reported but does not reject
an otherwise valid in-memory Canvas. A nested source directory is not excluded
merely because its basename is `build`, `renders`, or `target`.

The initial implementation may rebuild the whole Canvas after any
non-excluded project change. This deliberately includes changes that later
prove irrelevant; correctness takes priority over a dependency graph in this
version. Exact dependency reporting and dependency-based invalidation are
deferred optimizations.

Declared paths are resolved inside the captured corpus keyspace. Lexical `.`
and contained `..` segments are normalized; absolute paths and paths escaping
the corpus are rejected. Case aliases are honored only when the backing
filesystem is itself case-insensitive, so one-shot export and hosted Editor
share the host filesystem's lookup semantics. A file or directory read failure
is fingerprinted as observer state. It rejects the candidate when the failed
path is a source or declared Canvas dependency; an unrelated observer failure
remains visible to change detection without invalidating an otherwise complete
Canvas. Safe file and directory symlinks whose targets remain inside the
project are captured under their logical paths. Relevant dangling links,
cycles, and project escapes reject with diagnostics and remain observable so a
later repair recovers automatically.

Filesystem notifications or metadata may identify possible activity, but the
source revision and Canvas bundle identities derive from content. Modification
time and byte length alone are not sufficient identities.

## 11. Concurrency and artifact coherence

Implementations may build serially or cancel superseded work. Either way:

- every attempt materializes and builds from one exact set of captured input
  bytes;
- edits observed during collection or building schedule a newer revision and
  prevent the stale result from activating;
- a result checks that it is still current before becoming ready;
- stale ready results are discarded;
- activation names one immutable bundle identity; and
- browser fetches cannot combine HTML, CSS, assets, or metadata from different
  bundle identities.

An internal build discarded by the stability check receives no candidate
ordinal. The process-local candidate generation advances only when a
stability-validated success or rejection is settled and published.

One practical implementation is to retain immutable bundles by digest until
the browser acknowledges activation, then retire older inactive bundles. The
RFC does not require that storage scheme.

## 12. Implementation topology

This feature belongs at the static projection and Editor-host boundary. It
does not require a new runtime or language feature.

Expected responsibilities are:

- `uhura-check`: continue producing checked IR, resolved previews,
  diagnostics, and compiled stylesheet without owning watch behavior;
- `uhura-project`: render the final deterministic Canvas HTML from complete
  preview frames, styles, and assets without performing host I/O;
- `uhura-cli::cmd::project`: adapt one captured source snapshot into checker
  input, orchestrate complete static projection, and expose the shared
  in-memory Canvas artifact used by hosted Editor and one-shot export;
- `uhura-cli`: own filesystem observation, source revision scheduling,
  candidate/active state, diagnostics publication, export I/O, and artifact
  serving;
- Editor TypeScript controller: subscribe to Editor generation events,
  checkpoint shell state, reload on a different active bundle identity, and
  restore the checkpoint; and
- Play host/runtime: remain unchanged by this RFC.

The Editor and Play may share a filesystem observation service, but they keep
separate active-generation state and event channels. Canvas acceptance must
not replace or restart Play, and a Play-only provider artifact is not a Canvas
dependency. Existing checker rules about which source and example errors gate
Play remain unchanged by this RFC.

A future in-memory source service may replace filesystem snapshots without
changing the candidate/active protocol established here.

## 13. Implementation sequence

The proposed delivery sequence is:

1. Extract a pure reusable Canvas builder and stop making hosted Editor depend
   on writing and rereading `renders/canvas.html`.
2. Add independent Editor candidate and active-generation state, complete
   dependency observation, and cold-invalid recovery.
3. Add Editor-specific status transport and last-known-good diagnostics.
4. Add generation-aware automatic document reload plus shell checkpoint and
   semantic-selection restoration.
5. Add adversarial conformance tests for invalid, rapid, and cross-artifact
   changes.

Each step preserves `uhura project` as the deterministic static export path.

## 14. Alternatives considered

### Restarting the command

This is current behavior. It is deterministic but breaks the primary
authoring loop and cannot recover automatically from an initially invalid
project.

### Patching the current Canvas DOM

Deferred. The existing Canvas is one self-contained document containing frame
markup, compiled styles, asset variables, navigator and inspector metadata,
and a one-shot controller. Correct patching must replace and rebind every one
of those regions without exposing a mixed generation. Whole-document reload
already provides that atomic boundary. DOM patching is justified only if
measured reload latency or flicker later becomes unacceptable.

### Mutating the visible Canvas while building

Rejected. Streaming partially checked frames or styles can display a
generation that never existed as one valid static projection.

### Rebuilding only affected previews

Deferred. It requires a trustworthy dependency graph and introduces no new
observable capability. Whole-Canvas construction is the correctness baseline;
incrementality is a later latency optimization.

### Delegating source rebuilds to Vite

Rejected. Vite owns development of the TypeScript shell, not the `.uhura`
language, examples, fixtures, catalogs, contracts, or static projection
pipeline. The native checker remains authoritative.

## 15. Consequences

Expected benefits:

- saved source changes appear in Canvas without manual restart;
- malformed intermediate edits do not destroy useful previews;
- initially invalid projects can recover in place;
- the self-contained export and hosted Editor cannot silently drift;
- shell interaction remains stable during authoring; and
- the candidate/active model provides a narrow precedent for future tooling
  without promising Play state migration.

Expected costs:

- Canvas generation must become a reusable in-memory build product;
- Editor and Play need distinct build status despite sharing one host;
- dependency discovery must include fixtures and assets rather than only the
  current watched extensions;
- browser activation causes a controlled document reload and may have minor
  visual flicker; and
- generation ordering and last-known-good behavior require dedicated tests.

No checked-IR, Core state, provider, Spock, or renderer-protocol change is
required.

## 16. Deferred decisions

This RFC leaves open:

- unsaved-buffer synchronization and ownership;
- persistent incremental syntax trees and compiler query graphs;
- per-definition or per-preview invalidation;
- separate checker validation targets for Canvas and Play;
- explicit authored hot-reload identity in the Uhura language;
- editable preview data and fixture/reference mutation;
- checkpoint encoding and state migration;
- state-preserving Play reload;
- provider effect disposal and generation fencing; and
- browser history or device-mechanic reconciliation for Play.

These require separate proposals. Acceptance of this RFC is not evidence that
they are solved.

## 17. Acceptance criteria

This RFC can be accepted with executable evidence that:

1. A valid saved `.uhura` change updates the hosted Canvas without restarting
   the process or manually refreshing the browser.
2. A rejected candidate does not reload and leaves the exact active Canvas
   bundle, camera, tool, search, and selection unchanged while diagnostic
   chrome displays its errors.
3. A valid revision after an invalid revision activates and clears the stale
   error state.
4. An initially invalid project remains hosted and activates its first valid
   Canvas after repair.
5. Rapid A -> B -> C edits can never activate stale B after C.
6. All displayed bundle members carry one active build identity; mixed
   CSS, assets, frames, navigator entries, or inspector data are impossible.
7. Changes, additions, removals, and renames of declared styles, fixtures,
   catalogs, contracts, asset manifests, and referenced asset bytes trigger
   the appropriate rebuild.
8. A selected preview is retained by semantic key when it survives and is
   cleared without camera movement when removed.
9. A different active bundle identity causes exactly one reload; a stream
   connection or reconnect reporting the same identity causes none, including
   across a host restart, and camera, zoom, tool, search, chrome visibility,
   and surviving selection restore afterward.
10. `uhura project` remains deterministic and emits one self-contained static
   document from the same Canvas builder.
11. Canvas rendering and activation execute no transitions, deliver no
    commands, and perform no provider calls, network I/O, or other runtime
    effects; bounded pure derived-example replay remains checker work.
12. Dedicated `uhura play` behavior is observably unchanged.
