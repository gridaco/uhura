# Play host inspection TODO

This tracker owns debugger work at the CLI/server boundary. Browser behavior is
tracked in the [Play debugger TODO](../../../../web/src/play/TODO.md), while
runtime and protocol semantics are tracked in the
[Core inspection TODO](../../../uhura-core/src/TODO.md).

## Source integration

- [ ] **[P1][M][Engine work: No] Serve revision-safe source excerpts and Editor
      locations for inspected nodes.**
  - **Owner:** `dev.rs`, the captured Editor model, and the Play HTTP namespace.
  - Resolve only files already captured inside the checked project corpus;
    never turn a client-provided path into an arbitrary filesystem read.
  - Require the program/source revision or hash that produced the span and
    reject stale mismatches instead of showing unrelated current bytes.
  - Accept and return UTF-8 byte offsets without converting them to JavaScript
    string indices on the server.
  - Define an Open-in-Editor location contract that degrades to a source excerpt
    when no desktop/editor integration is available.
  - Add traversal, symlink-escape, stale-revision, multibyte UTF-8, and cold-
    invalid/recovered-state tests.

## Deployment boundary

- [ ] **[P2][L][Engine work: Conditional] Make inspection availability explicit
      if `uhura play` gains a non-local or shareable mode.**
  - **Owner:** CLI listen/config policy and inspection HTTP routes; protocol
    redaction remains in the core tracker.
  - Keep the current trusted-local assumption explicit and default-deny
    inspection on an untrusted bind until an exposure policy exists.
  - Do not rely on the browser hiding the Debug control to protect inspection
    artifacts or source excerpts.
  - If shareable inspection is authorized later, advertise capabilities and
    redaction metadata rather than silently returning partial data.

## Non-goals for the CLI host

- Evaluating Uhura expressions for the debugger.
- Reconstructing runtime state from exported browser history.
- Owning graph layout, timeline selection, or other browser presentation state.
