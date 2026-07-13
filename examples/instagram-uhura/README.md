# Instagram — Uhura client slice

This is the Uhura implementation of the **client** half of the Instagram
example: the feed, profile, post, like, comment, and pagination experience,
compiled into a deterministic headless machine and driven by the scripted
fixture under `fixtures/`.

It is built and played from the sibling [`uhura/`](../../uhura/) workspace
(paths are `../examples/instagram-uhura` from there — see that README's quick
tour).

## Relationship to `examples/instagram/`

The Spock backend slice for the same product lives next door in
[`examples/instagram/`](../instagram/). They are kept as **two distinct
folders for now** because the two languages are not yet wired together — Uhura
still runs against its scripted fixture, not a live Spock provider. Once the
integration in [RFD 0022](../../docs/rfd/0022-uhura-the-client-language.md)
lands (contract projection + adapter), the plan is to **merge these into one
`examples/instagram/` domain** with a backend and a client side.

The product requirements are already shared: the canonical, language-neutral
PRD is [`examples/instagram/PRD.md`](../instagram/PRD.md), which Spock's
`v0.spock` implements and this slice targets. Uhura's assets are locally
generated (no shared media).

## Layout

- `app/` — routes and pages (`.uhura`)
- `components/` — reusable markup + `store` machines
- `ports/` — the typed service seam (`*.port.toml`) a Spock provider would satisfy
- `fixtures/` — scripted provider data + play scripts (the CI test double)
- `catalog/`, `styles/`, `surfaces/` — icon/token catalog, CSS, surface defs
- `uhura.toml` / `uhura.lock` — app manifest and port-binding lock
