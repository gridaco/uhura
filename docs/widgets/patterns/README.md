# Patterns

- **Status:** Stable router and working taxonomy
- **Lifetime:** Stable navigation; classification is revisable
- **Authority:** Working classification and guidance only

This directory documents reusable compositions that need no new primitive
contract. A pattern may combine Uhura components, catalog elements, source
state, events, and styling while remaining transparent to the semantic runtime
and renderers.

Patterns are guidance and reusable material, not catalog elements by naming
convention alone.

## Named patterns

The checker's unknown-element notes direct authors here for two names that are
deliberately not catalog elements. Both are compositions of existing
capabilities; exact element contracts belong to the versioned catalog docs.

### Avatar

An avatar is `<img>` with authored sizing and shape, not an element:

```uhura
<img class="viewer-avatar" src={viewer.avatar.src} alt={viewer.avatar.alt} />
```

with CSS owning the circular crop (`border-radius`, fixed inline/block size,
`object-fit: cover`). The accessibility contract is `<img>`'s own
`exactly-one-of [alt, decorative]`; an avatar that duplicates an adjacent
visible name is usually `decorative`. See the current
[v0 `<img>` page](../drafts/v0/elements/img.md).

### Card

A card is `<view>` with authored surface styling (border, radius, padding,
background), not an element. Grouping, list membership, and activation stay
with the existing contracts: `<view role="list">` + keyed `{#each}` for
collections, and a wrapping [`<region>`](../drafts/v0/elements/region.md) when
the whole card is one activation target. See the current
[v0 `<view>` page](../drafts/v0/elements/view.md).
