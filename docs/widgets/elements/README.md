# Elements

This directory documents semantic primitives declared by a versioned Uhura
element catalog. Elements survive into the renderer-neutral semantic view and
therefore require checker contracts, renderer support, and conformance cases.

An element document does not replace its machine-readable catalog declaration.
It explains the declaration's semantics, accessibility rules, realization, and
compatibility requirements.

## Catalogue

- [`<button>`](button.md) — generic named action control with checked `press`
  delivery; specialized control roles and some state semantics remain open.
- [`<scroll>`](scroll.md) — explicit semantic viewport with renderer-owned
  physical position and pagination observation.
- [`<icon>`](icon.md) — checked glyph token with renderer-owned realization;
  the permanent family and provisioning model remains open.
- [`<img>`](img.md) — typed asset-backed image with an explicit informative or
  decorative choice and native browser realization; responsive sources and
  lifecycle state remain open.
- [`<view>`](view.md) — neutral structural container and CSS layout hook;
  current list realization forces `listitem` onto direct children, overwriting
  existing roles, and navigation/tablist contracts remain incomplete.
