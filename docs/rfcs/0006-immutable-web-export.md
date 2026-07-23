# RFC 0006: Immutable Web export

- **Status:** Accepted
- **Implementation:** Implemented
- **Decision date:** 2026-07-23
- **Scope:** Host-agnostic static export of one checked Editor/Play generation
- **Depends on:**
  [RFC 0002](0002-model-driven-editor-live-updates.md) and
  [RFC 0005](0005-web-application-topology-and-ui-composition.md)
- **Supersedes:** None
- **Does not select:** A hosting vendor, deployment workflow, public URL,
  provider authority, or replacement development runtime

## 1. Decision

Uhura can export one checked project generation as an immutable directory of
ordinary Web files:

```sh
uhura export [path] --out <directory>
```

Optional publication topology is selected when exporting:

```sh
uhura export [path] \
  --out <directory> \
  --mount /products/uhura/ \
  --play-entry /orders/100
```

The result contains the canonical Editor and Play browser application, the
checked Editor state, Play artifacts, Wasm runtime, admitted provider module,
fonts, and captured assets. It contains no watcher, compiler, event stream, or
Uhura server process.

This is an **export** feature. Serving the result inside a documentation site,
artifact viewer, product page, or standalone origin is downstream publication
behavior. No one such embedding defines the feature.

The export format is hosting-vendor agnostic, but a materialized artifact is
mount-specific. Moving it from `/products/uhura/` to another path requires a
new export. Uhura does not emit Vercel, Netlify, nginx, or other vendor
configuration.

## 2. Why export is a separate runtime boundary

Native Editor and Play are development surfaces. Their host observes source,
checks coherent captures, publishes replacement revisions, and announces
changes through server-sent events.

An exported artifact has a different and deliberately smaller lifecycle:

- it starts from one already-checked project capture;
- every browser session uses the same Editor revision and Play generation;
- it cannot observe later source changes;
- it runs the Play machine in browser Wasm;
- it may be served below a path owned by a larger site; and
- freshness means exporting and publishing a replacement directory.

This does not restore the retired static Canvas or create a second renderer.
Export freezes the inputs of the same Editor/Play application used by the
native host.

## 3. Distribution profiles

One Uhura package carries two builds produced from the same Web source:

| Packaged directory | Profile | Consumer |
| --- | --- | --- |
| `share/uhura/web/` | `live` | `uhura editor` and `uhura play` |
| `share/uhura/web-export/` | `export-template` | `uhura export` |

The live profile retains the origin-root behavior of the native host. The
export template uses relative generated chunks and an explicit runtime host
configuration point. It is not itself a publishable project bundle.

At export time, the CLI:

1. locates the packaged export template and Wasm distribution;
2. checks one coherent project capture;
3. snapshots the current Editor and Play artifacts;
4. validates and materializes the requested mount and Play entry;
5. records the materialized Web topology;
6. inventories the exact payload bytes; and
7. stages and activates the output directory with rollback on activation
   failure.

Node, pnpm, and Vite are package-build dependencies. They are not dependencies
of `uhura export`, and export never rebuilds or overwrites the live Web
distribution.

## 4. One current immutable snapshot

Export uses the ordinary stable project-capture and candidate-build path. It
does not read source independently or define a weaker checker.

Publication requires:

1. a current renderable Editor revision;
2. a successful current Play generation;
3. equal Editor and Play publication revisions;
4. the recognized export-template Web profile;
5. the complete browser Wasm runtime; and
6. safe, non-conflicting output paths.

The host rejects a stale last-renderable Editor revision and a retained
last-good Play build after a failed current publication. Therefore one export
cannot silently combine artifacts from different source revisions.

The snapshot includes:

- the compiled Web application and local chunks;
- the Wasm JavaScript loader and Wasm binary;
- complete Editor state and icon fonts;
- Play IR, inspection data, configuration, stylesheet, and icon fonts;
- the admitted provider module, when present; and
- captured Play assets.

It excludes language source, compiler state, native process state, and Editor
or Play event endpoints.

## 5. Browser host configuration

`index.html` contains one typed runtime record:

```json
{
  "protocol": "uhura-host-config/0",
  "mountPath": "/products/uhura/",
  "mode": "static",
  "playEntry": "/orders/100"
}
```

The export template defaults to live root values only so it can be built and
validated. The exporter replaces that record and the template's controlled
entry-asset references before any output is published. Generated dynamic
chunks remain relative to their module URL, so emitted JavaScript is not
searched or rewritten.

Static mode is explicit. The browser does not infer it from missing event
streams, failed requests, the current URL, or a particular hosting platform.

Mounts use one canonical grammar:

- `/` or an absolute origin-local path;
- exactly one slash between segments;
- a trailing slash in the materialized form (the CLI also accepts an omitted
  trailing slash and adds it);
- raw ASCII RFC 3986 path-segment characters, with other valid UTF-8 bytes
  percent-encoded;
- uppercase percent escapes in the materialized form, decoding only escaped
  unreserved bytes while preserving escaped reserved bytes as route identity;
  and
- no query, fragment, backslash, control character, dot segment, encoded dot
  segment, or encoded path separator.

`--play-entry` is an origin-local application path, optionally with query and
fragment. It is relative to the logical application root and is prefixed by
the selected mount. Path components and structured query pairs use the same
canonical codecs as the checked Router; fragments remain browser-only and are
not delivered to Router ingress. The entry must select Play rather than a
reserved Editor entry, transport namespace, compiled asset namespace, or real
exported file.

## 6. Mount ownership

A root export owns the ordinary Uhura topology:

```text
/
/play
/<application route>
/api/editor/*
/api/play/*
```

An export mounted at `/products/uhura/` owns only:

```text
/products/uhura/
/products/uhura/play
/products/uhura/<application route>
/products/uhura/api/editor/*
/products/uhura/api/play/*
```

The browser application strips the mount before route selection and restores
it when producing browser URLs. Editor/Play links, Uhura APIs, Wasm, fonts, and
captured assets stay below that mount.

Same-origin links outside the mount are not intercepted by Uhura. Programmatic
Uhura navigation also rejects outside-mount destinations. Provider-owned and
site-owned URLs are not generally rebased; only the
`/api/play/assets/` namespace published by Uhura is treated as an Uhura asset
route. In static mode, encoded hierarchy separators in that captured-asset
identity are emitted as ordinary path separators, so the publisher does not
need special encoded-slash routing.

## 7. Static browser behavior

Editor fetches and renders the exported complete state through the canonical
projection renderer. It does not create an `EventSource`, and it does not show
a disconnected-live-preview warning because the artifact never promises live
updates.

Play loads the same IR, inspection, provider configuration, stylesheet, fonts,
assets, and Wasm runtime as native Play. `api/play/static.json` supplies the
pinned generation when ordinary static-file responses do not carry the native
host's generation header. Play remains interactive for the page session, but
does not subscribe to a development-generation stream.

The pinned generation prevents the browser from accepting a known
cross-generation API response inside one coherent publication. It cannot
detect a publisher or intermediary mixing files from separately published
directories, and the browser does not verify manifest hashes. Coherent
directory activation and cache invalidation remain publisher
responsibilities.

Editor, Play, and application routes remain one single-page application. A
publisher must serve `index.html` as the history fallback for missing
document routes inside the declared mount while allowing real API and asset
files to win.

## 8. Bundle records

Three records have distinct identities:

| File | Protocol | Responsibility |
| --- | --- | --- |
| `uhura-web-build.json` | `uhura-web-build/1` | Identifies the packaged profile, then records the materialized asset base, mount, Play entry, and host-config protocol. |
| `api/play/static.json` | `uhura-static-play/0` | Supplies the pinned Play generation for static responses. |
| `uhura-static-bundle.json` | `uhura-static-web-bundle/0` | Describes the exported artifact and its publication contract. |

The bundle manifest records:

- a deterministic bundle identity;
- checked source identity and tool version;
- mount and public Play entry;
- Editor revision and Play generation;
- preview counts;
- `index.html` as the entry document;
- a vendor-neutral history-fallback description; and
- every payload file's relative path, SHA-256 digest, byte length, and content
  type.

The manifest excludes itself from its inventory. It supports external
integrity and publication tooling; the current browser runtime does not verify
the inventory.

## 9. Publication safety

Export writes a sibling staging directory before activation. An existing
destination is moved aside only after the candidate is complete. The candidate
is then renamed into place, and activation failure attempts to restore the
previous destination.

The output refuses root, parent traversal, an existing destination that is
itself a symlink, non-directory replacements, and any path equal to, above, or
below the captured project root. Symlinked ancestors are resolved before the
topology check. Keeping source and publication trees disjoint prevents
replacement of the project and prevents a prior bundle from becoming source
input to the next export. Exported file paths must be relative normal
components, and one path cannot carry conflicting bytes or media types.

This is directory-level local publication safety, not a remote deployment
transaction or crash-consistent filesystem protocol.

## 10. Provider and publisher boundaries

Listenerless means the exported artifact needs no dedicated Uhura process. It
still needs an ordinary Web origin that provides:

- the declared mount;
- correct content types, including Wasm and extensionless JSON; and
- the declared history fallback.

Export packages the provider module admitted by the project. It does not make
an arbitrary provider offline. A provider may use browser-local state, captured
Uhura assets, a same-origin application API, or a remote authority. Export does
not rewrite those choices, synthesize a backend, or weaken provider admission.

## 11. Non-goals

This decision does not introduce:

- browser parsing, checking, source discovery, or source editing;
- live source watching, incremental publication, or runtime migration;
- server-side rendering, hydration, or a service worker;
- persistence policy or multi-tab coordination;
- an offline guarantee for arbitrary providers;
- provider-specific authority behavior;
- hosting-vendor configuration or deployment;
- a relocatable artifact after export;
- a second renderer or static Canvas; or
- a promise that every future native-host capability must be exportable.

Archive formats, signing, delta publication, and deployment adapters are
separate decisions.

## 12. Conformance

The implemented conformance surface proves:

- the packaged CLI exports without a source checkout or frontend toolchain;
- live and export Web distributions are located independently;
- one template materializes at root or a nested canonical mount;
- malformed raw and encoded mount paths are rejected;
- static Editor and Play do not create event streams;
- stale Editor and retained last-good Play states cannot be exported;
- outside-mount links and programmatic routes remain outside Uhura ownership;
- required Web, Editor, Play, Wasm, and configured font bytes are present;
- the manifest inventory matches every emitted payload;
- a rejected replacement leaves the prior export intact; and
- the publisher contract describes history fallback without selecting a host.

Embedding one output in a containing site is useful proof of this contract,
not an additional Uhura feature or dependency.
