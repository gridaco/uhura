# Examples

This directory contains standalone program harnesses and application-scale
evidence for Uhura.

An example begins with a problem that exists independently of the current
language. Its requirements must not shrink, change names, or acquire convenient
exceptions merely because Uhura cannot express them yet. When an implementation
cannot satisfy an example honestly, that is evidence about the language.

Examples are not the language definition. Specifications define accepted
behavior for a named version, and conformance tests check that behavior.
Examples instead expose requirements, compare candidate designs, and record
where a language or runtime succeeds, distorts the problem, or fails.

## Example classes

| Path | Class | Purpose |
| --- | --- | --- |
| [`programs/`](programs/) | Program harnesses | Small, standalone, language-neutral transition problems used to design and compare the machine language |
| [`instagram/`](instagram/) | Application example | A full-stack product and integration example exercising Uhura, Spock, providers, rendering, and visual authoring together |

The two classes answer different questions. Program harnesses isolate the
behavioral core without depending on widgets or a renderer. Application
examples test whether that core remains useful when embedded in a consequential
frontend.

Repository-level program harnesses are also distinct from authored
`*.examples.uhura` modules. Those modules select static design previews for one
Uhura project. A program harness specifies an independent problem against which
different language designs may be evaluated.

## Problem authority

The language must fit the example; the example must not fit the language.

For a program harness, the authoritative artifacts are its language-neutral
problem statement, state and input model, transition behavior, invariants, and
observable traces. Uhura source, checked IR, runtime fixtures, and visual shells
are possible answer sheets. None may silently redefine the challenge.

For an application example, the product promise and independently grounded
behavior are authoritative. Existing source and screenshots are implementation
evidence rather than permission to simplify the product.

This authority is local to the example. A harness can reveal a language
requirement, but it cannot make a construct supported or normative without the
ordinary RFC, specification, and conformance process.

## Admission

A program harness belongs here when it:

- is a coherent standalone problem rather than a feature checklist;
- can be stated without current Uhura syntax or implementation concepts;
- has deterministic acceptance criteria over declared inputs;
- names its state, events, outputs, invariants, and invalid cases precisely;
- exposes a materially different pressure from the existing programs; and
- remains useful if Uhura is replaced or redesigned.

An application example belongs here when its product or integration problem is
valuable independently of Uhura and its stated scope remains honest.

Do not add examples merely to demonstrate a new keyword, widget, or convenient
happy path. A focused implementation fixture that only guards an accepted
behavior belongs with the relevant tests.

## Evolution

Problem statements are frozen while language candidates are being compared.
Changing one requires a problem-level reason independent of candidate success.
Candidate implementations may be rewritten or discarded freely.

Once behavior is accepted for a named language version, the relevant cases
should graduate into that version's conformance suite. The example remains
evidence and a readable problem statement; it does not become a substitute for
the specification.
