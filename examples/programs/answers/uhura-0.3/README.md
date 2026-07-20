# Uhura 0.3 answer to L0–L2

- **Status:** Executable Uhura 0.3 answer sheet
- **Historical machine-model record:** [Relay B3](../../../../docs/spec/drafts/relay-b3/)
- **Problem authority:** [L0–L2 program harnesses](../../)

[programs.uhura](programs.uhura) contains the complete Uhura 0.3 answer
for:

- L0 Bounded Counter;
- L1 River Crossing; and
- L2 Keyed Task Supervisor.

The Markdown problems remain authoritative. The single canonical Uhura engine
parses, checks, lowers, formats conservatively, and executes this exact source
against the L0–L2 conformance suites. The historical Relay B3 record explains
the design experiment that preceded this implementation; it is not another
runtime. A passing answer must still not be used to weaken a problem.

This answer deliberately declares no `use evidence` module: its frozen
exhaustive and adversarial cases remain implementation conformance tests, not
part of the authoring-size comparison. Consequently `uhura trace` rejects this
project with “no evidence scenarios” instead of silently selecting a different
runtime or test script.
