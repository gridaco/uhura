# Instagram Uhura client

This directory is the independently checkable Uhura client inside the
[Instagram framework example](../README.md). Its `uhura.toml` owns routes,
catalog, ports, deterministic fixtures, and Play provider configuration. The
parent `spock.toml` composes it with `../backend/` for `spock start` and
`spock dev`; it does not absorb the Uhura manifest.

## Editor workflow provenance

Replay-derived previews are connected to their direct parent in a dedicated
rail above each row. Edge labels summarize directly authored replay steps,
and selection highlights immediate parent/child relationships. These are
checked example provenance edges, not a second runtime state graph. Mounted
surfaces are identified by their runtime instance key and shown as direct,
inherited, or snapshot-mounted children. When one surface opens another, the
Inspector reconstructs the checked opener chain recursively instead of
flattening both instances under the page.

Use the parent README for full-stack commands. Uhura-only checks can target
this directory directly and use `fixtures/` without a running authority.
