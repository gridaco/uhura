# Uhura 0.4 answer to L0–L2

- **Status:** Executable incubation-candidate answer
- **Language:** Uhura 0.4 incubation candidate
- **Problem authority:** [L0–L2 program harnesses](../../)
- **Specification:** [Uhura 0.4](../../../../docs/spec/drafts/0.4/)

[programs.uhura](programs.uhura) is the complete executable source fixture
against which the 0.4 grammar, formatter, checker, lowering, and runtime
behavior are tested. It answers:

- L0 Bounded Counter;
- L1 River Crossing; and
- L2 Keyed Task Supervisor.

The 0.4 frontend parses, formats, checks, lowers, executes, checkpoints, and
replays this file against the frozen harness traces. It remains subordinate to
the language-neutral problems.

The answer deliberately contains no UI, framework feature, host adapter, or
widget. It tests the standalone machine core. Its project identity and
single-file logical-module map are fixed by [uhura.toml](uhura.toml).
