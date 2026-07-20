# Uhura 0.3 answer to A0 Return Desk

- **Status:** Executable Uhura 0.3 answer sheet
- **Language and system records:** [Uhura specification index](../../../../../docs/spec/)
- **Problem authority:** [A0 Return Desk](../..)
- **Independent evidence:** [A0 reference oracle](../../reference-oracle/)

The answer is split by authority:

```text
machine.uhura       headless application-session coordinator
web.uhura           use ui; pure checked web projection
conformance.uhura   use evidence; fixtures, scenarios, pins, checkpoints
host.toml           explicit live instance and port bindings
provider.mjs        application-owned live port adapters
```

These files are Uhura source, not pseudocode. The Uhura machine kernel parses,
checks, lowers, executes, renders, and hosts this exact project. The A0 Markdown
remains authoritative if this answer omits or changes a requirement.

The reference oracle tests the application behavior independently. It does
not prove that these source files parse or lower to the same machine.

`host.toml` is admitted against the closed Uhura deployment schema. Browser
history is injected by the standard `web.history` capability. The application
module does not implement or wrap it. The order observation and return request
ports use the generic `app.provider` boundary implemented by `provider.mjs`.
That module returns exactly those two adapters. The browser admits them against
the exact adapter, checked contract, and instance identities in
`uhura-play-config/1`
before any command crosses the boundary. Adapter deliveries enter the
`uhura-browser/2` machine boundary through the browser runtime's deferred FIFO
bridge, so foreign code cannot synchronously reenter a machine reaction.

The provider owns its live seed order and accepted return settlement
independently from `conformance.uhura`. Removing evidence source removes
previews and conformance artifacts without changing the live Play dependency
graph. `ReturnDesk` has `Unit` configuration, so this answer intentionally
omits the manifest's optional `configuration` field. A non-Unit machine must
instead provide one TOML string containing canonical tagged Uhura value JSON;
the host type-decodes and genesis-preflights that exact value before Play is
available.
