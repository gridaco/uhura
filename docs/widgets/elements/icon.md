# `<icon>`

- **Status:** Implemented element; family and provisioning model undecided
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Current spike convenience; no accepted widget RFC
- **Specification:** Pre-specification
- **Implementation:** Checked semantic token and browser realization implemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<icon>` names a glyph from the active element catalog. It is a system-defined
catalog element, not a user-authored component and not an SVG container.

Whether Uhura should permanently have a first-class icon element remains open.
The current element is practical implementation evidence, not a decision to
standardize one icon family, one asset format, or one provisioning mechanism.

## The language-design tension

At the most fundamental level, an icon is visual geometry. A sufficiently
expressive UI language could expose only an SVG-shaped primitive and let an
author draw or import any glyph. That approach is language-honest: the source
states the actual vector content and Uhura does not pretend that `heart` or
`home` has universal geometry.

Practical UI systems often add an icon abstraction anyway. A short checked
token can provide:

- a coherent visual set by default;
- validation and completion over known names;
- concise source for common prototype actions;
- consistent color, sizing, and variant behavior;
- one renderer-neutral request that a web, native, or terminal renderer can
  realize differently; and
- a stable seam for theme or platform substitution.

Flutter's themed icon vocabulary is evidence for that tradeoff, not proof that
Uhura should copy its ownership model. Uhura is a language with deterministic
IR and multiple renderer boundaries, so choosing a glyph family in compiler or
Core code would accidentally make presentation data part of the language.

The narrow current meaning is therefore:

> `<icon name="x" />` requests the active renderer's glyph for the checked
> catalog token `x`.

It does not promise SVG, a font codepoint, a particular family, or a semantic
label.

## Current semantic contract

```uhura
<button label="Like">
  <icon name="heart" />
</button>
```

| Contract | Current behavior |
|---|---|
| Class | `content` |
| Children | None |
| `name` | Required token from the active catalog's closed icon set |
| `class` | Universal, CSS-owned class list |
| Events | None |
| Accessibility | Always decorative and hidden from the accessibility tree |
| Semantic value | The checked `name` token only |

Literal unknown names are checker errors with a nearest-name suggestion when
one exists. An expression bound to `name` must have the catalog icon-token
type. Source cannot expand the set by writing a new string.

The current project-pinned base catalog contains these eighteen names:

```text
home, search, plus, reels, profile,
heart, heart-filled, comment, close, back,
grid, layers, video-off, progress,
bookmark, bookmark-filled, chevron-left, chevron-right
```

This is the current spike/base-catalog vocabulary; not every name is used by
the Instagram markup. It is not an accepted taxonomy, a selected public
family, or a promise that future base catalogs use these exact tokens.

## Ownership invariant

Uhura's semantic layers may carry and validate the icon token. They must not
own its concrete drawing.

The following data is renderer, provider, or resource-pack data and must not
be synthesized or owned by Uhura Core, checked program IR, semantic view
snapshots, checkpoints, traces, or the native Editor read model:

- SVG paths, shapes, view boxes, and paint commands;
- font files, family names, codepoints, ligatures, and font-feature settings;
- platform-symbol identifiers;
- raster glyph assets; and
- icon-family variant tables.

This invariant applies to hard-coded or bundled icon families and every
token-to-glyph mapping. A built-in family, an `icons.toml` manifest, font
icons, and native symbols all require the same separation: semantic source
names the request; the selected renderer resource realizes it.

A future authored `<svg>` or vector primitive is a different contract. Its
author-supplied geometry could legitimately be semantic source and appear in
checked IR or Editor data. This rule does not prohibit such a primitive; it
prohibits `<icon>` family geometry from masquerading as language or engine
data.

A future host may transport an explicitly selected project resource pack as
opaque renderer input. That is different from the compiler or Editor model
containing a hard-coded glyph family.

### Corrected implementation boundary

The original spike violated this boundary by hard-coding SVG commands in
`uhura-editor-model`, serializing them in `EditorState`, and republishing the
same table from the native host for Play.

That geometry now lives only in the shared browser renderer as an explicitly
provisional table. Editor and Play receive the same semantic `name` token and
resolve it locally. `uhura-editor-state/2` no longer contains icon commands,
and Play no longer fetches a native `/api/play/icons.json` artifact. Runtime
`uhura-ir/0` and `uhura-view/0` did not change because they already carried
only the token.

The relocated table preserves the spike's appearance. Its location makes it a
browser default, not language law.

## Accessibility

The current `<icon>` is decorative-only. The browser renderer always applies
`aria-hidden="true"`; the token is never exposed as an accessible name and the
element is never focusable or interactive.

Meaning belongs to the containing control:

```uhura
<button label="Open profile">
  <icon name="profile" />
</button>
```

`name="profile"` must not become the button's label. Token spelling is
developer vocabulary, may be family-specific, and is not necessarily suitable
for people or localization.

A standalone meaningful-icon contract would require an explicit accessible
name, role, localization behavior, and composition rules. It is not part of
the current element.

## Rendering and fallback

The browser currently creates a `currentColor` SVG with a 24 by 24 default
size from its provisional renderer table. That mapping is an implementation
detail. Another renderer may use a font glyph, native symbol, raster resource,
or different vector geometry while honoring the same checked token.

CSS may override the host element and SVG's layout, size, color, and
surrounding presentation. There are currently no portable `size`, `color`,
`weight`, `fill`, or `variant` properties.

The browser uses a generic circular placeholder if its renderer table lacks a
checked token. This prevents broken DOM but is not an accepted semantic
fallback. A durable design must decide whether missing realization is a build
error, renderer capability diagnostic, explicit placeholder, or negotiated
fallback. It must never silently substitute a different meaningful glyph.

## Why `<icon>` is not `<text>`

A font-backed renderer may implement an icon with a glyph, but that does not
make the semantic element text. `<text>` carries human-readable content,
participates in reading order, and may require localization. The current
`<icon>` carries a closed developer token, is always decorative, and delegates
its visual representation to the renderer.

Treating icon names as ordinary text or Unicode codepoints would leak one
font's encoding into source and would make accessibility behavior accidental.

## Motion

`<icon>` defines no motion. A future animated-icon or transition contract must
specify state, interruption, reduced-motion behavior, and ownership separately.
CSS animation applied by an author remains presentation and does not change
the icon token's semantics.

## Conformance

Current and future conformance coverage should include:

- every current catalog token checks and survives into the semantic view as
  only its name;
- an unknown literal name is rejected with a useful diagnostic;
- an invalid dynamic name type is rejected;
- missing `name`, children, and event bindings are rejected;
- `<icon>` remains non-interactive and accessibility-hidden;
- a labeled parent control retains its accessible name;
- Editor and Play resolve the same token through the shared renderer policy;
- renderer-owned glyph coverage matches the active provisional catalog;
- a missing renderer glyph takes the deterministic fallback path;
- no engine-owned SVG path, font codepoint, or glyph table appears in native
  Editor state, semantic Play artifacts, Core IR, view snapshots, checkpoints,
  or traces; and
- changing renderer glyph geometry cannot change Core hashes or replay.

## Decisions and open questions

The following choices are intentionally unresolved:

1. Whether Uhura keeps `<icon>` at all or ultimately exposes only an SVG/vector
   primitive.
2. Whether names describe semantic concepts such as `back` or concrete family
   glyphs such as a particular icon pack's identifier.
3. Whether Uhura ships one default family, requires a project resource such as
   `icons.toml`, accepts renderer/provider packs, or combines those layers.
4. How a project pins family identity, version, license, and reproducible
   assets.
5. Whether names need namespaces and how collisions compose across packs.
6. Whether fill, outline, weight, grade, optical size, and other variants are
   tokens, properties, separate names, or renderer theme policy.
7. Whether and how user-authored SVG or custom glyphs participate in the same
   name space.
8. Whether a meaningful standalone icon exists and what accessible-name
   contract it requires.
9. How renderers declare glyph coverage and what a portable missing-glyph
   diagnostic or fallback looks like.
10. Whether family selection belongs to the element catalog, a separate
    resource manifest, project configuration, or host negotiation.

No option above is implied by retaining the provisional browser table. Any
future `<icon>` or other named-glyph abstraction must preserve the ownership
invariant.

Current implementation references:

- [Base catalog names and element declaration](../../../examples/instagram/client/catalog/base.toml)
- [Catalog and markup checking](../../../crates/uhura-check/src/markup.rs)
- [Renderer-owned provisional glyph table](../../../web/src/renderer/icons.ts)
- [Shared semantic element application](../../../web/src/renderer/appliers.ts)
- [Editor state protocol](../../../web/src/editor/editor-state.ts)
