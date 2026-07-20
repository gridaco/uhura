# `<icon>`

- **Status:** Implemented element; font-only realization selected for pre-v1
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** [Icon font](../integrations/icon-font.md)
- **Availability:** Built-in Lucide family and local font families implemented
- **Decision:** Before v1, `<icon>` is realized only through an icon font
- **Specification:** Pre-specification
- **Implementation:** Checked token, font resources, host transport, and browser realization implemented
- **Owners:** Checker, Core, Host, Renderer
- **Supported renderers:** Browser Editor and Play

`<icon>` requests a named glyph from a configured icon-font family. It is a
system-defined content element, not a user-authored component, SVG container,
text node, or Unicode escape hatch.

The pre-v1 contract is deliberately narrow:

- one font file and one checked name-to-codepoint map define a local family;
- source uses human-readable glyph names, never characters or codepoints;
- an omitted family selects the project default;
- variants such as filled or outlined glyphs are distinct names;
- concrete font data remains outside Core and semantic protocols; and
- custom SVG, raster, native-symbol, ligature, and remote-registry sources are
  not supported.

This is the bedrock contract for the current implementation phase. It does not
decide whether v1 retains font-only realization or later admits an authored
`<svg>` primitive or other icon resources.

## Semantic contract

```uhura
<button label="Like">
  <icon name="heart" />
</button>

<icon family="brand" name="logo" />
```

| Contract | Pre-v1 behavior |
|---|---|
| Class | `content` |
| `family` | Optional literal icon-family name; omission selects the project default |
| `name` | Required checked glyph name from the selected family |
| `class` | Universal, CSS-owned class list |
| Children | None |
| Events | None |
| Accessibility | Always decorative and hidden from the accessibility tree |
| Semantic value | Normalized `{ family, name }` token only |

`family` must be a quoted literal before v1. A dynamic family would make the
valid type of `name` depend on runtime state and is intentionally unsupported.

`name` may be a checked expression when every possible value belongs to the
selected family's closed glyph map:

```uhura
<icon name={if alert then "heart-pulse" else "heart"} />
```

There is no portable `variant`, `weight`, `style`, `size`, or `color`
property. A local family could publish names such as `heart`, `heart-fill`, and
`heart-duotone`; those are hypothetical family-owned names, and their suffixes
have no language-defined meaning. The bundled Lucide family does not define
filled or duotone variants. Size and color remain CSS-owned.

Unknown families and glyph names are checker errors. Missing font coverage is
a build/resource error. Neither condition permits a silent circle, tofu glyph,
system-font fallback, or substitution with a different meaningful icon.

## Project integration

The project topology, resource-manifest syntax, glyph-map format, content
identity, and font requirements are defined by the
[Icon font integration](../integrations/icon-font.md).

The shortest form requires no configuration:

```uhura
<icon name="heart" />
```

It resolves through Uhura's Foundation-provided default family. A local family
is selected explicitly:

```uhura
<icon family="brand" name="logo" />
```

There is no `use icons` declaration. Family paths belong to `uhura.toml`, and
the literal `family` property already makes a non-default dependency explicit.

The built-in `lucide` vocabulary is derived from the official `lucide-static`
`codepoints.json`, with only entries that actually resolve through the shipped
font's `cmap`. Uhura adds no renamed or replacement aliases. For example,
source uses `x`, `clapperboard`, `user-round`, `message-circle`,
`square-plus`, and `loader-circle`. This keeps names searchable in Lucide's
own catalogue and makes generated source agree with the installed family.

## Ownership

The semantic layers may carry and validate only the logical family and glyph
name. They must not own or serialize:

- font paths or bytes;
- CSS font-family names or generated `@font-face` rules;
- Unicode codepoints, ligatures, glyph indices, or OpenType tables;
- token-to-codepoint mappings;
- SVG paths, command tables, or other fallback geometry; or
- Foundation-family build inputs.

After checking, omission is normalized:

```text
<icon name="heart" />
        ↓
{ family: "lucide", name: "heart" }
```

That token may appear in Core IR and semantic views. The corresponding
codepoint map and font remain an opaque, content-addressed renderer resource
served by the host. Font geometry changes therefore change resource pins, not
Core hashes or replay.

## Accessibility

`<icon>` is decorative-only before v1. The browser renderer applies
`aria-hidden="true"`; the glyph is not focusable, selectable, or exposed as an
accessible name.

Meaning belongs to the containing control:

```uhura
<button label="Open profile">
  <icon name="user-round" />
</button>
```

The glyph name must never become the button label. It is developer vocabulary,
may be family-specific, and is not localized human-readable text.

A meaningful standalone icon would require an explicit accessible-name,
localization, role, and composition contract. It is deferred beyond pre-v1.

## Rendering

A conforming pre-v1 renderer resolves the checked `{ family, name }` token to
exactly one codepoint in the pinned icon font. The browser converts that scalar
internally and renders it with a content-addressed font-family name. Pre-v1
icons are monochrome font outlines whose paint follows inherited color.

The renderer must:

- load no fallback font;
- use `currentColor` and inherited CSS font sizing;
- use `font-synthesis: none` and disable ligatures;
- keep the glyph decorative and non-selectable; and
- report font-load or mapping failures explicitly.

Although the renderer ultimately emits a font character, source and semantic
artifacts never contain that character. `<icon>` remains distinct from
`<text>`: text carries human-readable content and participates in reading
order; an icon carries a closed developer token and is always decorative.

## Implementation

The checker loads the bundled or project-local family, decodes and validates
its WOFF2/OpenType data, checks every named codepoint against `cmap`, and pins
the glyph map separately from the font bytes. The host publishes immutable,
revision- or generation-matched renderer resources. Editor and Play load the
content-addressed WOFF2 through `FontFace` before realizing an icon.

The former hard-coded SVG command table and generic-circle fallback are
removed. A missing family, glyph, resource, or browser font load fails
explicitly instead of substituting geometry or falling through to a system
font.

## Motion

`<icon>` defines no motion. CSS animation remains presentation. A future
animated-icon contract must define state, interruption, completion, and
reduced-motion behavior separately.

## Conformance

Pre-v1 conformance requires tests proving that:

- the configured default family is inserted when `family` is omitted;
- an explicit family must be a known literal family;
- every literal or finite dynamic `name` belongs to the selected glyph map;
- missing names, font files, mappings, or codepoints are build errors;
- Editor and Play use identical pinned font and glyph-map bytes;
- font loading is deterministic and works offline;
- no system-font, Unicode, SVG, or generic-shape fallback is used;
- the element stays decorative and a labeled parent retains its name;
- Core IR, semantic views, traces, replay, and EditorState contain only the
  normalized `{ family, name }` token; and
- changing font bytes cannot change Core hashes or replay.

## Deferred beyond pre-v1

The narrow contract intentionally defers:

- custom SVG and authored vector geometry;
- variants, weights, styles, and variable-font axes as structured properties;
- ligatures and multi-codepoint sequences;
- raster and platform-native symbols;
- remote registries and package resolution;
- per-file icon imports or icon namespace values;
- font subsetting and other distribution optimizations;
- semantic aliases across families; and
- meaningful standalone icons.

Pre-0.4 implementation references (paths may now contain replacement code):

- [Native element and icon-token checking](../../../../../crates/uhura-check/src/checker.rs)
- [Supplemental resource manifest](../../../../../crates/uhura-check/src/resource_manifest.rs)
- [Browser icon-font resource loader](../../../../../web/src/renderer/icons.ts)
- [Checked icon-font registry](../../../../../crates/uhura-check/src/icon_fonts.rs)
- [Bundled Lucide provenance](../../../../../resources/icon-fonts/lucide/PROVENANCE.md)
- [Editor state protocol](../../../../../web/src/editor/editor-state.ts)
- [Instagram icon usage](../../../../../examples/instagram/client/ui.uhura)
