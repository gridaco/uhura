# Icon font

- **Status:** Historical snapshot of the implemented pre-v1 foundation
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Shared facet
- **Primary form:** Not applicable
- **Facets:** Integration
- **Availability:** Built-in Lucide family and local opt-in families were implemented in v0
- **Decision:** Font files plus checked name maps are the only pre-v1 icon resources
- **Specification:** Pre-specification
- **Implementation:** Historically implemented across resource checking, host transport, and browser loading
- **Owners:** Foundation, Checker, Host, Renderer
- **Applies to:** [`<icon>`](../elements/icon.md)
- **Supported renderers:** Browser Editor and Play

> Historical scope: present-tense implementation language below describes the
> retired v0 snapshot captured by this document, not Uhura 0.4.

This integration defines how a logical `<icon>` token is backed by a font. It
owns family configuration, name-to-codepoint maps, font validation, content
identity, transport, and renderer loading. The element contract remains in
[`<icon>`](../elements/icon.md).

Before v1, this is the only supported icon-resource mechanism. Uhura projects
cannot supply SVG files, raw paths, raster images, CSS icon classes, JavaScript
modules, ligature strings, or platform-symbol identifiers as `<icon>`
realizations.

## Topology

A local family consists of exactly one WOFF2 file and one JSON glyph map:

```text
client/
├── uhura.toml
└── icons/
    └── brand/
        ├── icons.woff2
        └── glyphs.json
```

Both paths are project-relative. Relative escapes, absolute paths, symlink
escapes, and network URLs are rejected.

`uhura.toml` is a closed supplemental-resource manifest. Machine entry,
presentation, lifetime, configuration, and adapter bindings belong to
`host.toml`; retired app, catalog, port, fixture, and Play-profile sections are
not accepted here. A project with no supplemental resources may omit
`uhura.toml` entirely and still receives the built-in Lucide family.

Foundation families use the same resolved pair but are shipped with Uhura. The
first built-in family is the official Lucide font from `lucide-static`:

```text
resources/icon-fonts/lucide/
├── lucide.woff2
├── codepoints.json        # unchanged upstream provenance input
├── glyphs.json            # deterministic cmap-backed checked map
├── LICENSE
└── PROVENANCE.md
```

Additional Foundation-managed families may be produced through
[`gridaco/icons`](https://github.com/gridaco/icons). SVG may remain an upstream
input to such a font build. It is not an Uhura project resource and never
crosses the icon-font contract.

## Manifest

The project default is a logical family name:

```toml
[icons]
default = "lucide"
```

When `[icons]` is absent, the pre-v1 default is the bundled `lucide` family.
Other Foundation families may later be selected by reserved names without
local paths.

A local family is declared directly beneath `[icons]`:

```toml
[icons]
default = "lucide"

[icons.brand]
font = "icons/brand/icons.woff2"
glyphs = "icons/brand/glyphs.json"
```

`default` is reserved and cannot be a family name. Family names are lowercase
kebab-case. A local family cannot shadow a Foundation family.

Each local family requires exactly `font` and `glyphs`. Unknown fields are
errors. Inline maps, remote addresses, fallback families, multiple font files,
and format negotiation are deliberately unsupported.

## Glyph map

Authors and agents use names. Codepoints exist only in the renderer resource.
The JSON format matches the official `lucide-static` `codepoints.json`: one
top-level object from lower-kebab names to decimal Unicode scalar values.

```json
{
  "heart": 57586,
  "heart-pulse": 57587,
  "logo": 983040
}
```

The map must be non-empty. Keys are lowercase kebab-case names and values are
integer codepoints. Duplicate JSON keys, nested metadata, empty maps,
non-integer values, and raw Unicode characters are errors. Reusing the
upstream shape avoids a second Foundation-only wrapper format.

The bundled Lucide family retains the byte-identical upstream map as
provenance input and uses a deterministic checked `glyphs.json` containing
only the 1,995 entries backed by the published WOFF2 `cmap`. Nineteen stale
upstream names are recorded in `PROVENANCE.md`; Uhura introduces no aliases or
replacement codepoints.

Each value names exactly one Unicode scalar from a Private Use Area:

- `U+E000` through `U+F8FF`;
- `U+F0000` through `U+FFFFD`; or
- `U+100000` through `U+10FFFD`.

Surrogates, sequences, ligatures, control characters, and codepoints outside
those ranges are rejected. More than one name may intentionally map to the
same glyph, but every declared codepoint must exist in the font's `cmap` and
must not resolve to `.notdef`.

Filled, outlined, weighted, or otherwise varied glyphs remain ordinary names:

```uhura
<icon name={if alert then "heart-pulse" else "heart"} />
```

The language assigns no meaning to the suffix.

## Font requirements

Pre-v1 accepts WOFF2 only. The checker/resource loader must verify the WOFF2
signature, decode its OpenType tables, and validate the glyph map against the
font's Unicode `cmap` before Editor or Play starts.

Only monochrome outline fonts are accepted. Embedded SVG, bitmap, color-palette,
and variation mechanisms are rejected before v1; they would reintroduce
multiple realization models through a font container.

The font is an icon resource, not authored text typography. A renderer creates
an internal family name derived from the content digest rather than trusting a
font's embedded family name or exposing a project-controlled CSS family.

The browser realization uses:

- a content-addressed font URL with `font/woff2` media type;
- a generated `@font-face` family scoped to icon rendering;
- `String.fromCodePoint` or an equivalent internal conversion;
- no fallback font;
- `font-synthesis: none` and disabled ligatures; and
- `aria-hidden="true"` and non-selectable glyph hosts.

Size and color remain CSS-owned through inherited font size and
`currentColor`. Arbitrary font metrics are not corrected through per-glyph
configuration before v1; local icon fonts are responsible for coherent em-box
and baseline metrics.

## Resolution and content identity

The checker loads every declared family and validates its closed glyph-name
set. The host captures the JSON and font bytes as immutable project inputs and
serves them through a renderer-resource channel separate from assets and
EditorState.

Pre-v1 resolution is intentionally eager: every declared family is validated,
transported, and loaded before a render becomes usable, even if the current
view does not reference it. This keeps resource coherence simple and makes a
bad declaration fail deterministically; used-family closure, lazy loading, and
subsetting remain later distribution optimizations.

Bundled-family provenance records the upstream package version and integrity
beside the vendored files. Each checked glyph map and WOFF2 receives its own
deterministic SHA-256 identity, while the complete captured project revision
has one source fingerprint. The retired v0 project setup did not use a
separate `uhura.lock`.

Rendering performs no external/package resolution and never rereads project
paths: the browser fetches the checked manifest and WOFF2 only from the current
Uhura host. Editor and Play receive the same content-addressed resources and
work offline without third-party network access.

Core IR, semantic views, checkpoints, traces, replay, and EditorState carry
only normalized `{ family, name }` tokens. They never contain font URLs,
family names from the font file, bytes, codepoints, glyph maps, or generated
CSS.

## Failure behavior

The following are build-blocking diagnostics:

- unknown or dynamic family selection;
- unknown glyph name;
- missing, unreadable, malformed, or escaping paths;
- a non-WOFF2 or undecodable font;
- an embedded SVG, bitmap, color, or variable-font realization;
- malformed glyph JSON or codepoint syntax;
- a mapped codepoint absent from `cmap` or resolving to `.notdef`; and
- a resource-manifest section or field outside the closed assets/icons surface.

A runtime font-load failure is a renderer-resource diagnostic. The renderer
must not fall through to a system font, render tofu as if successful, select a
nearby codepoint, or substitute generic SVG geometry.

## Security and privacy

Local files are data only. The integration executes no CSS, HTML, JavaScript,
font lifecycle script, package hook, or remote response. Implementations must
bound font and map sizes, reject path escapes, decode fonts defensively, and
serve font bytes from the Uhura host with the `font/woff2` media type.

The pre-v1 JSON format does not carry license metadata. Foundation builds must
retain upstream licenses and notices as distribution artifacts; private local
fonts remain the project's responsibility.

## Conformance

Conformance coverage must include:

- zero-configuration Foundation-default resolution;
- multiple declared families with literal selection;
- positive and negative glyph-map and PUA validation;
- `cmap` and `.notdef` verification;
- rejection of embedded SVG, bitmap, color, and variable-font tables;
- safe-path and symlink-escape rejection;
- stable canonical glyph-map hashing and exact font-byte hashing;
- rejection of retired runtime configuration in `uhura.toml`;
- identical offline Editor and Play realization;
- explicit font-load failure with no fallback; and
- proof that codepoints and font data never enter semantic artifacts.

## Deferred beyond pre-v1

- SVG, raster, and native-symbol resources;
- ligatures and multi-codepoint glyphs;
- structured variants and variable-font axes;
- multiple fonts or fallback chains per family;
- inline mappings;
- remote or package resolution;
- font subsetting and preloading policy; and
- a permanent v1 distribution, locking, and licensing schema.
