# Instagram Uhura client

This directory is the independently checkable Uhura client inside the
[Instagram framework example](../README.md). Its `uhura.toml` owns routes,
catalog, ports, deterministic fixtures, and Play provider configuration. The
parent `spock.toml` composes it with `../backend/` for `spock start` and
`spock dev`; it does not absorb the Uhura manifest.

Use the parent README for full-stack commands. Uhura-only checks can target
this directory directly and use `fixtures/` without a running authority.
