# Uhura 0.4 answer to A0 Return Desk

- **Status:** Executable 0.4 answer
- **Problem authority:** [A0 Return Desk](../..)
- **Language contract:** [Uhura 0.4 draft](../../../../../docs/spec/drafts/0.4/)

This answer preserves the complete A0 application rather than reducing it to
a syntax sample. It retains the original routing normalization, revision
fence, source-conflict recovery, asynchronous request correlation, stale and
invalid classifications, policy surface lifetime, commit reconciliation,
static examples, checkpoint replay, web projection, and live host boundary.

The project is split by authority:

```text
uhura.toml                  0.4 package and logical-module map
machine.uhura               headless ReturnDesk machine and public data model
ui.uhura                    explicitly activated web UI projection
evidence-support.uhura      immutable values imported by tooling evidence
evidence/conformance.uhura  native 0.4 scenarios, checkpoints, and examples
host.toml                   live application-session selection and bindings
provider.mjs                application-owned live port adapters
```

The physical filename `machine.uhura` is deliberately mapped to logical
module `return_desk`; filenames do not establish source namespaces, and
`machine` is a reserved language term. Host selectors remain independent of
that module path:

```toml
machine = "crate::ReturnDesk"
presentation = "crate::ReturnDeskWeb"
```

Evidence uses the same 0.4 frontend under a manifest-enforced tooling role.
Because an evidence source may declare only scenarios, checkpoints, and examples, its
closed fixture values live in the `evidence_support` core module and are
publicly imported through the project module graph. They are not read by the live
machine or host configuration and therefore do not alter a reaction.

This 0.4 answer is admitted by the complete checked evidence set and by the
editor/play host gate;
a parser-only pass does not count as conformance. The language-independent
reference oracle separately validates the shared A0 contract, but does not
parse or execute either Uhura answer.
