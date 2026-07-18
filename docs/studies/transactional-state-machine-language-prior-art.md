# Transactional state-machine language prior art

- **Status:** Non-normative prior-art study
- **Lifetime:** Disposable study
- **Method:** Primary-source comparison retrieved July 18, 2026
- **Doctrine:** [Mission and identity](../doctrine/mission.md),
  [authoring ergonomics](../doctrine/authoring.md), and
  [design principles](../doctrine/principles.md)
- **Related study:** [Language necessity and surface reuse](language-necessity-and-surface-reuse.md)
- **Companion study:** [Visual state-machine authoring and deterministic simulation prior art](visual-state-machine-authoring-prior-art.md)
- **Problem corpus:** [Program harnesses](../../examples/programs/README.md)
- **Authority:** Research only; this document accepts no machine model,
  language surface, syntax, or implementation architecture

## Outcome

The state-machine candidate that motivated this audit is not a novel
computational paradigm. Deterministic Mealy reactions, bounded functional
automata, atomic write logs, reducer-plus-command architectures, and
communicating transition systems all have substantial precedent.

No reviewed language combines the complete candidate contract:

1. one closed typed input is admitted;
2. one finite deterministic step evaluates against owned pre-state;
3. the step returns a machine-defined outcome statically associated with
   commit or abort;
4. commit atomically publishes both next state and an ordered command buffer;
5. abort publishes neither;
6. commands execute only after publication, outside the semantic step;
7. command results can affect behavior only as later typed inputs; and
8. the complete input history produces a canonical inspectable trace.

Among the systems screened here, Scilla is the closest whole-language
precedent, FSM-Hume is the closest bounded functional-automaton precedent,
Lustre and SCADE provide the strongest deterministic synchronous foundation,
Kôika and Bluespec provide the closest private-log and atomic-publication
model, and the Elm Architecture provides the closest familiar authoring
signature.

That remaining conjunction may be useful, but uniqueness does not establish
that Uhura should own it. It must still outperform an existing language, a
library, or a checked profile on the independent program corpus.

## 1. Comparison contract

This study uses the following abstract contract only to compare prior work. It
is not proposed Uhura semantics:

```text
step : Configuration × State × Input
    -> Commit(Outcome, NextState, OrderedList<Command>)
     | Abort(Outcome)
```

For an admitted program and input:

- `step` is total and completes within the language's declared computational
  envelope;
- expressions cannot perform ambient I/O or mutate external authority;
- writes and commands remain private until the terminal result;
- commit publishes next state and the command list as one semantic fact;
- abort preserves pre-state and emits no command;
- command delivery cannot synchronously reenter the step; and
- any nondeterministic observation that matters returns as a later recorded
  input.

The distinction between **command order** and **physical execution order** is
load-bearing. A language may define an ordered semantic request list while a
host executes requests concurrently, serially, or not at all. Any such freedom
must be explicit and must not change the deterministic machine trace.

## 2. Selected detailed-comparison summary

`Yes` means the property is inherent in the cited language layer. `Partial`
means it can be represented or is guaranteed only under an additional profile,
schedule, runtime, or domain restriction.

| Prior work | Input/state to output/next state | Deterministic bounded reaction | Private atomic draft or rollback | Ordered deferred commands | Later results as inputs |
|---|---|---|---|---|---|
| Scilla | Yes | Yes for one transition | Yes | Partial | Partial |
| FSM-Hume | Yes | Yes in bounded layers | Partial: superstep publication | No: typed output wires | Partial: feedback and input wires |
| Lustre / SCADE | Yes, over synchronous flows | Yes | No explicit aborting draft | No: simultaneous output flows | Partial: environment-provided later flows |
| Kôika / Bluespec | Partial: scheduled rules | Yes with a fixed schedule | Yes | No: internal write log | No built-in external-result protocol |
| Elm Architecture | Yes | No termination guarantee | Partial: pure return value | No: `Cmd` has no result-order guarantee | Yes, as later `Msg` values |

None of these five detailed comparisons has the candidate's first-class
combination of a machine-defined outcome, static commit policy, rollbackable
state-and-command draft, independently published ordered outbox, and later
typed result protocol.

## 3. Scilla

[Scilla](https://arxiv.org/abs/1801.00687) models smart contracts as
communicating automata. A received message selects a standalone atomic
transition. The language separates pure computation, local state manipulation,
and blockchain communication; transition-local computation must terminate.
When another party must participate, messages are buffered during the
transition and processed only after the transition completes.

This closely matches:

- message-triggered owned state transitions;
- atomic local publication;
- separation of computation from communication;
- explicit messages as boundary data;
- transition-level termination; and
- protection against synchronous reentrancy within local computation.

The differences remain material:

- dispatch and message schemas are contract-oriented rather than one closed
  exhaustive input algebra;
- outgoing message records are not a machine-declared closed command family;
- commit or abort is principally blockchain transaction behavior, not a
  policy attached to each machine-defined domain outcome;
- a chain of contract messages can remain coupled to one blockchain
  transaction and rollback regime; and
- self- or cyclic-message computation is controlled by gas and a fixed
  message-chain limit rather than statically terminating as a composed system;
  exhausting either can roll back the whole transaction.

The transferable lesson is the separation boundary: complete local transition
work before explicit communication. This precedent does not justify inheriting
blockchain transaction coupling merely because the automata model is useful.

Primary sources:

- [Scilla design paper](https://docs.zilliqa.com/scilla-oopsla19.pdf)
- [Scilla language design principles](https://scilla.readthedocs.io/en/latest/intro.html)

## 4. FSM-Hume

[Hume](https://www.macs.hw.ac.uk/~greg/hume/) is a functional language built
around concurrent boxes connected by wires. An FSM-Hume box pattern-matches
input and old-state values and functionally produces new-state and output
values. Restricted Hume layers deliberately trade expressive power for
decidable termination and bounded time and space.

This closely matches:

- automata as the primary program topology;
- typed input, state, and output values;
- functional transition computation;
- explicit old-state to new-state flow;
- static computational bounds in the restricted layers; and
- a runtime superstep separating calculation from publication.

The differences are:

- persistent state is commonly represented through feedback wires rather than
  one owned state record;
- a network superstep, not one FIFO event receipt, is the central execution
  unit;
- outputs are dataflow values rather than declared capability commands;
- no domain outcome determines commit or abort; and
- feedback wiring is not an explicit request/result protocol with correlated
  host authority.

FSM-Hume is especially important because it tests the same product tension as
Uhura: useful expressiveness versus decidable language properties. Its layered
answer is prior art for a least-power core with progressively broader profiles.

Primary sources:

- [FSM-Hume: Programming Resource-Limited Systems Using Bounded Automata](https://doi.org/10.1145/967900.968192)
- [FSM-Hume is Finite State](https://www.macs.hw.ac.uk/~greg/publications/mhs.TFP03.pdf)

## 5. Lustre and SCADE

[Lustre](https://doi.org/10.1109/5.97300) is a synchronous dataflow language
for reactive systems. A compiled node can be understood as a deterministic
cycle function from current state and current input flows to output flows and
next state. Synchronous causality and initialization checks rule out
instantaneous dependency cycles and undefined past values. The model supports
bounded-memory implementations and verification-oriented safety properties.

This closely matches:

- a mathematical Mealy-style core;
- deterministic reaction to complete inputs;
- a clean functional relationship between streams;
- bounded-memory sequential implementation;
- explicit temporal state; and
- machine-checkable safety properties.

The differences are:

- one logical tick observes a set of simultaneous flows, not necessarily one
  algebraic event;
- outputs are simultaneous flow values, not an ordered list;
- a well-formed tick has no first-class commit or abort result;
- no private mutation-and-command draft is exposed to authors; and
- deferred external commands and later correlated outcomes are an environment
  architecture, not core language semantics.

Among the systems screened here, Lustre is the strongest precedent for
specifying the deterministic kernel mathematically. It is not evidence that
synchronous-flow notation is the right Uhura authoring surface.

Primary sources:

- [The Synchronous Data Flow Programming Language LUSTRE](https://doi.org/10.1109/5.97300)
- [SCADE/Swan language principles](https://ansyshelp.ansys.com/public/Views/Secured/ScadeOne/v242/en/ScadeOne/techdoc/getting_started/language/preamble.html)

## 6. Kôika and Bluespec guarded atomic actions

Kôika gives a small formal account of the guarded atomic-action model used by
Bluespec-like hardware languages. A rule reads committed beginning-of-cycle
register state, accumulates writes in a private log, and publishes compatible
writes together at the cycle boundary. Failure cancels the rule and discards
its log. A fixed schedule produces a deterministic sequential machine.

This closely matches:

- reads from one stable pre-state;
- private sequential draft updates;
- all-or-nothing publication;
- failure that discards earlier writes;
- bounded typed action evaluation; and
- invariant reasoning over atomic steps.

The differences are:

- scheduled hardware rules rather than admitted typed events are the unit of
  work;
- the baseline rule system needs a schedule to select deterministic order;
- failure or conflict is not a machine-defined domain outcome statically
  assigned a publication policy;
- the log contains internal register writes, not external commands; and
- no capability boundary returns physical results as later machine inputs.

Among the systems screened here, this is the most direct precedent for the
candidate's draft mechanics. It suggests that the write log and outbox can
share one transaction without making either immediately observable.

Primary source:

- [The Essence of Bluespec](https://doi.org/10.1145/3385412.3385965)

## 7. The Elm Architecture

The Elm Architecture uses the canonical effectful update signature:

```elm
update : Msg -> Model -> (Model, Cmd Msg)
```

The update function receives one message and immutable model, returns a new
model, and describes work for the runtime with `Cmd`. A command can eventually
produce another `Msg`, which reenters the same update path. The official
[effect guide](https://guide.elm-lang.org/effects/http.html) presents this
separation directly.

This closely matches:

- a compact event-and-model authoring surface;
- closed message types and pattern matching;
- functional next-state calculation;
- effects represented through runtime-managed command values; and
- asynchronous results returning as ordinary later messages.

The differences are:

- Elm does not statically prove termination of each update;
- `Cmd` is opaque, and `Cmd.batch` gives no ordering guarantee for results,
  rather than representing a declared inspectable ordered command algebra;
- the update result has no machine-defined outcome;
- state and commands are not conditionally discarded through a typed
  commit-or-abort result;
- multiple owned machine instances, routing, and receipts are application or
  runtime architecture rather than one closed semantic contract; and
- deterministic replay depends on the application's complete input and effect
  discipline, not only the language type.

Among the systems screened here, Elm is the strongest precedent for
ergonomics, not for the full guarantee ledger. Uhura should distinguish the
value of the update signature from properties that Elm does not claim.

Primary sources:

- [The Elm Architecture](https://guide.elm-lang.org/architecture/)
- [Elm commands and asynchronous results](https://guide.elm-lang.org/effects/http.html)
- [Elm `Cmd` API and batch ordering](https://github.com/elm/core/blob/master/src/Platform/Cmd.elm)

## 8. Additional screened precedents

These systems illuminate particular dimensions but are weaker matches for the
complete comparison contract:

- **Marlowe** has a total step function from input, state, contract, and
  observations to next state, continuation contract, and an action list.
  Execution within one block repeatedly steps until quiescent, so its semantic
  unit is not the candidate's single admitted event. Its algebras and progress
  rules are also fixed to financial contracts.
  [Primary paper](https://www.iog.io/api/research/pdf/L65KWDLX).
- **Michelson** gives deployed contracts the shape
  `(parameter, storage) -> (operation list, storage)`. Generated operations run
  after the entrypoint but remain in the same rollback group: a downstream
  failure cancels them and the originating storage update. Michelson also
  relies on gas rather than static totality and does not independently publish
  state before deferred work.
  [Official operation semantics](https://docs.tezos.com/smart-contracts/logic/operations).
- **Pact** combines atomic transactions with a language that excludes
  recursion and unbounded looping, while `defpact` sequences explicit steps
  across distinct transactions. It does not provide a closed input reducer
  that returns an ordered deferred-command outbox.
  [Pact language reference](https://docs.kadena.io/pact/reference).
- **Erlang/OTP `gen_statem`** literally returns next state, new data, and an
  action list from an event callback. Arbitrary Erlang code may execute effects
  or diverge before returning, so the callback shape is much closer than the
  guarantee model.
  [Official behavior guide](https://www.erlang.org/doc/system/statem.html).
- **P** provides typed asynchronous state machines, machine-local data, and
  per-machine FIFO event queues. Handlers mutate and send imperatively rather
  than return one rollbackable state-and-command transaction.
  [State-machine manual](https://p-org.github.io/P/manual/statemachines/).
- **Esterel** supplies deterministic atomic reactions and strong no-reentry
  precedent, but signals are synchronous and simultaneous rather than an
  ordered deferred-command protocol.
  [Language primer](https://www.college-de-france.fr/sites/default/files/documents/gerard-berry/UPL8106359781114103786_Esterelv5_primer.pdf).
- **Event-B** supplies atomic guarded state transitions and invariant
  preservation proof obligations. Its transition relation may be
  nondeterministic and has no intrinsic ordered command or asynchronous result
  model.
  [Rodin handbook: Event-B concepts](https://stups.hhu-hosting.de/handbook/rodin/current/html/tut_eventb_concepts.html).
- **SCXML and XState** supply run-to-completion statechart macrosteps and
  action models, but admit internal microsteps and host-language execution
  outside the candidate's total functional envelope.
  [SCXML Recommendation](https://www.w3.org/TR/scxml/) and
  [XState pure transitions](https://stately.ai/docs/pure-transitions).

## 9. What is established and what remains open

### Established by prior work

- Deterministic state/input to output/next-state semantics are mature.
- Functional reducer authoring can remain compact.
- Bounded automata can trade expressiveness for decidable resource properties.
- Private write logs can provide atomic publication and rollback.
- Communication can be separated from local computation.
- External results can reenter through the same event algebra.
- Invariants and traces can be checked independently of presentation.

### Not established for Uhura

- that one first-class machine abstraction is sufficient for frontend
  application lifetimes;
- that every accepted handler should be statically terminating rather than
  resource-metered or faulting;
- that an outcome family should encode commit policy;
- that command order must imply physical dispatch order;
- that state and an external-command outbox should share one atomic semantic
  publication;
- that every foreign computation should be asynchronous;
- that an independent language provides better authoring or checking than a
  library or checked profile; or
- that the complete model transfers cleanly to human and fresh-context agent
  authors under a bounded teaching packet.

## 10. Consequences for the next comparison

A successor candidate should not claim novelty for reducers, Mealy machines,
atomic rules, command values, or communicating automata. It should identify
which precedent supplies each semantic mechanism and justify every deliberate
departure.

The next comparison should test at least:

1. an encoding in the smallest viable existing language or library;
2. an encoding with Scilla- or Hume-like restricted computation;
3. whether commit policy belongs to outcome declarations or handler results;
4. whether the outbox is ordered semantically and what the host may reorder;
5. failure between semantic publication and physical command delivery;
6. duplicate, stale, cancelled, and unavailable external outcomes;
7. composition, destruction, restoration, and late events;
8. controlled changes beyond the original program corpus; and
9. bounded-acquisition trials using equal canonical teaching packets.

Until those comparisons exist, the defensible conclusion is narrow:

> The candidate is a coherent synthesis of mature ideas. Its complete
> guarantee bundle is not found in the reviewed prior work, but neither its
> uniqueness nor the need for an independently owned Uhura language has been
> demonstrated.
