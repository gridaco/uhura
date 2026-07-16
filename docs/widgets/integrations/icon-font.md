# Icon font

- **Status:** Pre-v1 direction; implementation pending
- **Document type:** Shared facet
- **Primary form:** Not applicable
- **Facets:** Integration
- **Availability:** Foundation built-ins and local opt-in families planned
- **Decision:** Font files plus checked name maps are the only pre-v1 icon resources
- **Specification:** Pre-specification
- **Implementation:** Provisional SVG realization exists; font pipeline unimplemented
- **Owners:** Foundation, Checker, Host, Renderer
- **Applies to:** [`<icon>`](../elements/icon.md)
- **Supported renderers:** Browser Editor and Play planned

This integration defines how a logical `<icon>` token is backed by a font. It
owns family configuration, name-to-codepoint maps, font validation, locking,
transport, and renderer loading. The element contract remains in
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
├── uhura.lock
└── icons/
    └── brand/
        ├── icons.woff2
        └── glyphs.json
```

Both paths are project-relative. Relative escapes, absolute paths, symlink
escapes, and network URLs are rejected.

Foundation families use the same resolved pair but are shipped or installed by
the Uhura/Spock/Grida Foundation. Their generated artifacts belong in
[`gridaco/icons`](https://github.com/gridaco/icons):

```text
dist/<family>/font/
├── icons.woff2
└── glyphs.json
```

SVG may remain an upstream input to the Foundation's font build. It is not an
Uhura project resource and never crosses the icon-font contract.

## Manifest

The project default is a logical family name:

```toml
[icons]
default = "phosphor"
```

When `[icons]` is absent, the pre-v1 default is Foundation Phosphor. Other
Foundation families may be selected by their reserved names without local
paths.

A local family is declared directly beneath `[icons]`:

```toml
[icons]
default = "phosphor"

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

Authors and agents use names. Codepoints exist only in the renderer resource:

```json
{
  "$schema": "https://icons.grida.co/schema/font-glyphs-v0.json",
  "glyphs": {
    "heart": "U+E001",
    "heart-fill": "U+E002",
    "logo": "U+F0000"
  }
}
```

The format contains only `$schema` and `glyphs` before v1. `glyphs` must be a
non-empty JSON object whose keys are lowercase kebab-case names and whose
values are canonical `U+` codepoint strings. Duplicate JSON keys, unknown
fields, empty maps, non-string values, and raw Unicode characters are errors.

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
<icon name={if selected then "heart-fill" else "heart"} />
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

## Resolution and locking

The checker loads every declared family and validates its closed glyph-name
set. The host captures the JSON and font bytes as immutable project inputs and
serves them through a renderer-resource channel separate from assets and
EditorState.

`uhura.lock` pins semantic names and presentation bytes separately:

```text
icon-glyphs brand sha256:<canonical-glyph-map>
icon-font brand sha256:<woff2-bytes>
```

Foundation pins additionally record the exact `gridaco/icons` source revision.
No font or glyph map is fetched during rendering. Editor and Play must receive
the same pinned resources and work offline after resolution.

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
- lock drift.

A runtime font-load failure is a renderer-resource diagnostic. The renderer
must not fall through to a system font, render tofu as if successful, select a
nearby codepoint, or substitute generic SVG geometry.

## Security and privacy

Local files are data only. The integration executes no CSS, HTML, JavaScript,
font lifecycle script, package hook, or remote response. Implementations must
bound font and map sizes, reject path escapes, decode fonts defensively, and
serve bytes with a restrictive content security policy.

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
- a permanent v1 distribution and licensing schema.
