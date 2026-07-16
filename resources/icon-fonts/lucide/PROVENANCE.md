# Lucide icon font provenance

- Package: `lucide-static`
- Version: `1.24.0`
- Repository: <https://github.com/lucide-icons/lucide>
- Package: <https://www.npmjs.com/package/lucide-static>
- npm integrity: `sha512-NDSgPb/RWllI9QooPbGXfQCXQi/45oquDmtljeN3qVxqfEUf91uM2C4zijy8bs+pEWxKckA0/WZLZT43YD4Xnw==`
- Vendored font SHA-256: `ac8e910a948c000ad075c8ebc7c429f066f68b87a4fbf6bce2d911588102c403`
- Vendored upstream codepoint-map SHA-256: `b6cc1d3840803e3cf54603c77937dfa9e60db36d4f22bc64d57318a147b00c04`
- Derived checked glyph-map SHA-256: `71c09f9d0f64e8004a84d0a0ae6a855667fdf5c56d7613d4e763e19ed3807155`

`lucide.woff2`, `codepoints.json`, and `LICENSE` are copied unchanged from
the `font/` directory and package root of the published npm tarball.

`glyphs.json` is the deterministic checked renderer vocabulary derived from
those two upstream resources. Its keys are lexically sorted; each entry is
copied unchanged from `codepoints.json` only when the codepoint resolves
through `lucide.woff2`'s Unicode `cmap` to a non-`.notdef` glyph. It is emitted
as two-space-indented JSON with one trailing newline. The upstream map has
2,014 entries; the checked map has 1,995.

The following 19 upstream-stale names were removed because their mapped
codepoints have no glyph in the published font:

`chrome`, `chromium`, `codepen`, `codesandbox`, `dribbble`, `facebook`,
`figma`, `framer`, `github`, `gitlab`, `instagram`, `linkedin`, `pocket`,
`rail-symbol`, `slack`, `trello`, `twitch`, `twitter`, and `youtube`.

No replacement aliases or codepoints are introduced. Glyph names stay
human-readable in source while codepoints remain a renderer resource.
