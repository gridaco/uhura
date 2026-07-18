# Visual state-machine authoring and deterministic simulation prior art

- **Status:** Non-normative prior-art study
- **Lifetime:** Disposable study
- **Method:** Primary-source comparison retrieved July 18, 2026
- **Doctrine:** [Mission and identity](../doctrine/mission.md),
  [authoring ergonomics](../doctrine/authoring.md), and
  [design principles](../doctrine/principles.md)
- **Companion study:** [Transactional state-machine language prior art](transactional-state-machine-language-prior-art.md)
- **Problem corpus:** [Program harnesses](../../examples/programs/README.md)
- **Authority:** Research only; this document accepts no machine model,
  language surface, visual editor, syntax, or implementation architecture

## Outcome

Game engines provide strong evidence that visual authoring and explicit state
topologies can be practical production tools. They do not generally provide
evidence for one visual language whose complete execution is deterministic,
bounded, atomic, and replayable.

Four properties must remain separate:

1. **Visual authoring** presents program structure as graphs, tables, blocks,
   or event sheets.
2. **State-machine semantics** define states, admitted stimuli, transition
   eligibility, conflict resolution, entry and exit behavior, and hierarchy.
3. **Deterministic evaluation** requires complete state and inputs, controlled
   time and randomness, stable scheduling and numeric behavior, and isolated
   external authority.
4. **Replayable execution** additionally requires serializable state,
   versioned configuration and assets, recorded inputs, and a reproducible
   trace contract.

Most reviewed game tools deliberately supply only a subset of these:

- Unreal Blueprints and Unity Script Graphs are general visual scripting
  systems with direct engine effects.
- Unreal StateTree, Unity State Graphs, animation graphs, PlayMaker, and other
  visual FSM tools make state topology explicit but do not make every action a
  pure or transactional reaction.
- Construct and GDevelop event sheets specify a readable evaluation order but
  mutate live runtime state and remain sensitive to frame time and host events.
- Unreal and Unity provide prediction, correction, or replay subsystems
  separately from their visual scripting systems.

[Photon Quantum](https://doc.photonengine.com/quantum/current/quantum-intro)
and its visual Bot SDK are the closest reviewed game-industry intersection:
visual HFSM and behavior-tree assets execute inside a deterministic
predict-and-rollback simulation. Even there, the graph defines only part of
the program. Custom C# leaves, the simulation API, fixed-point rules, complete
frame state, and the runtime boundary supply the determinism.

Games are not the only durable industry precedent. Stateflow, IEC 61131-3
graphical controller languages, and LabVIEW show substantial visual
programming practice in embedded and industrial control. They also expose a
warning: a diagram can have rigorously specified execution semantics while
still allowing live mutation, host-specific scheduling, or graphical position
to influence order.

The defensible conclusion is therefore:

> Visuality contributes authoring and inspection affordances, not
> determinism. A graph is trustworthy only when its nodes, ordering,
> state boundary, external effects, time model, and replay contract are
> independently specified.

This study does not establish that Uhura needs a visual editor or that a graph
should be its source of truth.

## 1. Comparison method

The companion study compares languages against this non-normative abstract
reaction:

```text
step : Configuration × State × Input
    -> Commit(Outcome, NextState, OrderedList<Command>)
     | Abort(Outcome)
```

This study adds authoring and simulation questions that a visually explicit
machine can otherwise hide.

### 1.1 Authoring representation

- Is the authored artifact a free-position graph, ordered table, event sheet,
  block diagram, text, or a combination?
- Is geometry only presentation, or can position determine priority?
- Is there a compact diffable representation independent of screenshots?
- Is the visual artifact the executable source, an exchange format, or a
  projection compiled into another runtime artifact?

### 1.2 Machine topology

- Is there one active state, one active root-to-leaf path, or several parallel
  regions?
- Are transitions triggered by typed events, engine callbacks, conditions
  polled every tick, task completion, time, or arbitrary code?
- When several transitions are eligible, what wins?
- Can one stimulus cause one transition, a bounded cascade, or an unbounded
  internal event chain?

### 1.3 Deterministic evaluation

- What constitutes complete state: variables, clocks, random streams,
  animation progress, queues, active submachines, and pending work?
- Is execution fixed-step, event-driven, frame-driven, or host-scheduled?
- Are iteration, concurrency, numeric behavior, and random selection stable?
- Which APIs are admitted inside the deterministic boundary?
- Does the system guarantee identical results, or only document local order?

### 1.4 Effects and recovery

- Do actions mutate the world immediately or return descriptions of work?
- Can a failed action undo earlier state and external effects?
- Are asynchronous results recorded as later inputs?
- Does rollback restore the complete simulation or only selected predicted
  state?

### 1.5 Replay and observation

- Can the complete relevant state be serialized and restored?
- Are inputs, configuration, and executable assets recorded or versioned?
- Can execution be reproduced independently of the live host?
- Is a debugger history also sufficient to replay the program?

## 2. Comparison summary

`Specified order` below means only that some local evaluation precedence is
documented. It does not imply deterministic replay.

| System | Authoring model | Explicit machine topology | Deterministic scope | Effect/publication model | Main lesson |
|---|---|---|---|---|---|
| Stateflow | Graphs and state-transition tables | Hierarchical and parallel statecharts | Specified chart execution under a configured model | State and transition actions update model data; events may cascade | Strong visual state semantics; visual position can affect priority |
| IEC 61131-3 / SFC | Ladder, function-block, sequential charts, and text | SFC steps, transitions, and parallel branches | Standard language semantics plus vendor task/runtime profile | Cyclic controller code reads and writes live process data | Long-lived visual control precedent; runtime and source exchange are separate |
| LabVIEW G | Graphical dataflow | Dataflow rather than an inherent FSM | Dependency-defined partial order; independent effects remain unordered; real-time profiles can constrain timing | Nodes perform computation and I/O when inputs are ready | Visual dependencies expose parallelism but leave unrelated order open |
| Unreal Blueprints | Typed imperative node graphs | General control flow; separate state tools | No whole-program deterministic contract | Impure nodes mutate engine state and may perform latent work | Mature visual scripting does not imply a state-machine kernel |
| Unreal StateTree | Visual hierarchical state and decision graph | Active paths, tasks, transitions, and selectors | Specified selection and transition order; random and concurrent behavior allowed | Tasks act on shared engine context | Strong state topology and debugger precedent, not atomic reaction semantics |
| Unreal Behavior Tree | Visual decision tree | Composites, tasks, services, and decorators; no state-transition topology | Ordered branch selection; task effects remain host-dependent | Tasks act on shared engine context | Decision control and state machines are distinct models |
| Unity Visual Scripting | Script Graphs and State Graphs | Nested states, multiple starts, arbitrary state scripts | Local graph order only; engine callback order remains partly unspecified | Graph units mutate Unity and variable scopes directly | State-shaped authoring over an open effectful runtime |
| Animation state machines | Domain graphs in Unreal, Unity, Godot, and Quantum | States, transitions, blends, retained playback | Domain-specific; among the reviewed systems, Quantum explicitly documents animation state in deterministic snapshots | Usually produces poses and callbacks rather than application commands | Timeful presentation state should not be confused with application state |
| PlayMaker and adjacent visual FSMs | State graph plus reusable action lists | Event-driven FSM; tool-specific hierarchy and priorities | Operational order without replay guarantee | Actions call Unity or host APIs directly | High-level actions and live debugging matter more than graphifying a low-level API |
| Construct / GDevelop | Ordered condition-action event sheets | Reactive rules, not explicit finite states | Top-to-bottom evaluation; frame and trigger behavior remain external | Earlier actions immediately affect later rules | Compact ordered rules are useful, but order is not transactionality |
| Photon Quantum Bot SDK | Visual HFSM / behavior-tree documents compiled to assets | Hierarchical states, transitions, decisions, and actions | Deterministic inside verified Quantum frames | Graph leaves update rollbackable simulation state; presentation stays outside | Closest reviewed game intersection, achieved by a restricted simulation boundary |
| Verse | Text integrated with UEFN rather than a visual graph | No inherent machine topology | Deterministic core calculus, not a whole-engine replay guarantee | Failure contexts can roll back admitted mutation | Game-language transactionality is independent of visuality and frame determinism |
| Godot VisualScript | General node graph, removed from Godot 4 core | General scripting rather than an inherent FSM | None claimed | Mirrored the ordinary engine API | Negative evidence: visual syntax without higher-level modeling did not earn its cost |

No reviewed system supplies the abstract comparison contract's full
combination of one closed typed input, a finite state reaction, conditional
atomic publication of state and an ordered command outbox, no synchronous
reentry, and canonical input-history replay.

## 3. Industrial graphical control precedents

### 3.1 Stateflow

[Stateflow](https://www.mathworks.com/help/stateflow/index.html) is a graphical
language for state-transition diagrams, flow charts, state-transition tables,
and truth tables. It is used to model decision logic reacting to signals,
events, and time conditions, and can simulate inside Simulink or generate
deployment code.

It is a stronger semantic precedent than a generic node editor:

- charts have explicit states, transitions, hierarchy, exclusive and parallel
  decomposition, events, guards, and state actions;
- a chart wakes on a time step or event, executes until it has no more work,
  and sleeps until the next wake-up;
- valid competing transitions follow defined execution priority;
- parallel states have explicit or derived execution order; and
- simulation animation, breakpoints, coverage, saved operating points, and
  generated code connect the diagram to executable behavior.

Primary references:

- [Stateflow semantics](https://www.mathworks.com/help/stateflow/ug/what-do-semantics-mean-for-stateflow-charts.html)
- [Chart execution](https://www.mathworks.com/help/stateflow/ug/chart-during-actions.html)
- [Transition order and actions](https://www.mathworks.com/help/stateflow/ug/create-transitions-between-states.html)
- [Parallel-state execution order](https://www.mathworks.com/help/stateflow/ug/control-state-execution-order.html)
- [Verification and code generation](https://www.mathworks.com/help/stateflow/verification-and-code-generation.html)

The differences from the comparison reaction remain substantial:

- charts may wake on periodic time steps as well as events;
- entry, during, exit, condition, and transition actions update model data
  during chart execution rather than building one state-and-command draft;
- local events can interrupt current activity and create additional internal
  work;
- functions may be written in MATLAB or C and inherit a broader computational
  and effect envelope; and
- there is no first-class machine outcome that chooses commit or abort for
  both state and deferred external commands.

Stateflow also makes a normally hidden design choice observable. Depending on
chart configuration, transition and parallel-state priority can derive from
creation order or physical position. Its API documentation describes implicit
ordering by hierarchy, label kind, clock position, and top-to-bottom,
left-to-right placement, while recommending explicit user-specified order for
compatibility.

[Stateflow's state-transition table](https://www.mathworks.com/help/stateflow/ref/statetransitiontable.html)
is an important counterpoint. It represents the same class of modal logic in a
compact table that requires less maintenance of graphical objects, and
Stateflow supports conversion between table and chart. The transferable lesson
is not that Uhura should adopt either notation. It is that semantic order must
survive alternate projections and must not depend accidentally on editor
geometry.

### 3.2 IEC 61131-3 graphical controller languages

[IEC 61131-3:2025](https://webstore.iec.ch/en/publication/68533) specifies the
syntax and semantics of Structured Text plus the graphical Ladder Diagram and
Function Block Diagram languages. It also defines graphical and textual
Sequential Function Chart elements for structuring programs and function
blocks.

This is durable evidence that industry can standardize visual program forms:

- FBD wires functions and stateful function blocks into networks;
- SFC gives steps, transitions, actions, alternative branches, and parallel
  branches an explicit operational topology;
- controller programs commonly run cyclically against process inputs and
  outputs, under vendor task configuration; and
- vendors provide online highlighting and inspection of active steps and
  values.

The standard language is not by itself a complete deterministic deployment
contract. Vendor task configuration, I/O sampling, real-time scheduling,
numeric targets, and extension libraries still define the execution envelope.
For example, [CODESYS documents](https://content.helpme-codesys.com/en/CODESYS%20SFC/_cds_sfc_sequence_of_processing.html)
its precise SFC cycle order, including top-to-bottom and left-to-right checks,
entry and exit actions, alphabetical IEC-action execution, and transition
activation. [TwinCAT documents](https://infosys.beckhoff.com/content/1033/tcsystemover/12695808651.html)
the separate real-time system that executes PLC tasks at configured priorities
and cycle times.

The mismatch with the comparison reaction remains:

- process variables and outputs are generally updated as part of a cyclic
  controller program, not returned as an inspectable command algebra;
- SFC actions may be written in other admitted controller languages and can
  call stateful function blocks;
- failure is not a domain outcome that atomically discards state and an
  external outbox; and
- graphical layout and vendor rules can participate in scheduling.

Source exchange is a separate layer. The
[PLCopen XML format](https://www.plcopen.org/standards/xml-echange/) preserves
both textual and graphical information, including block position and
connections, for tool interchange. A machine-readable graph exchange format
is useful infrastructure, but it is not necessarily compact, readable
authoring source for either people or agents.

### 3.3 LabVIEW

[LabVIEW G](https://www.ni.com/docs/en-GB/bundle/labview/page/block-diagram-data-flow.html)
is a graphical dataflow language. A node runs after all required inputs are
available; independent nodes may run concurrently and have no implied
left-to-right or top-to-bottom order.

LabVIEW provides two useful controls for this study:

1. A visual program can have precise dependency semantics without being a
   state machine.
2. A graphical language does not become deterministic merely because its
   wires are visible.

When order matters without a natural dependency, authors must add a data
dependency or sequence structure. Deterministic timing can be configured under
a separate deployment profile: the
[LabVIEW Real-Time environment](https://www.ni.com/en/shop/data-acquisition-and-control/add-ons-for-data-acquisition-and-control/what-is-labview-real-time-module/building-a-real-time-system-with-ni-hardware-and-software.html)
adds real-time operating systems, timed loops, priorities, and processor
assignment.

LabVIEW therefore supports the same architectural lesson as deterministic
game runtimes: authoring notation, dependency semantics, and deterministic
deployment are related but separately owned layers.

## 4. General game visual scripting

### 4.1 Unreal Blueprints

[Blueprints](https://dev.epicgames.com/documentation/unreal-engine/blueprints-visual-scripting-in-unreal-engine)
are Unreal Engine's complete node-based gameplay scripting system. They expose
engine objects, events, values, control flow, functions, macros, mutation, and
latent operations through a typed graph.

Blueprints validate several authoring facilities:

- visible data and execution flow;
- reusable classes and functions;
- live inspection and node-level debugging;
- a discoverable vocabulary of engine-level operations; and
- event-driven composition between authored logic and the host.

They do not validate the comparison reaction. Unreal distinguishes
[pure and impure functions](https://dev.epicgames.com/documentation/unreal-engine/functions-in-unreal-engine):
impure functions execute through control wires and may mutate state.
Blueprints also admit
[recursive calls](https://dev.epicgames.com/documentation/en-us/unreal-engine/blueprint-debugger-in-unreal-engine),
loops, arbitrary engine calls, and latent work. Epic's
[best-practices guidance](https://dev.epicgames.com/documentation/unreal-engine/blueprint-best-practices-in-unreal-engine)
recommends Blueprints for event-driven functionality and native C++ for
operation-heavy per-tick algorithms; this is a pragmatic authoring boundary,
not a bounded-language guarantee.

The useful precedent is the editor and domain vocabulary. Copying Blueprint
execution would import an open imperative host API, immediate effects, and
engine lifecycle semantics that the comparison reaction makes explicit.

### 4.2 Unity Visual Scripting

Unity Visual Scripting separates
[Script Graphs and State Graphs](https://docs.unity3d.com/Packages/com.unity.visualscripting@1.9/manual/vs-graph-types.html):

- Script Graphs connect actions and values in explicit local order; and
- State Graphs connect states through transitions, with Script Graphs inside
  states and transitions.

State Graphs support nested Super States, Any State, and multiple simultaneous
start states. State scripts may listen to enter, update, exit, collision,
lifecycle, UI, animation, or custom events and may use the ordinary Visual
Scripting unit catalog without a purity restriction.

This provides recognizable state-machine authoring while leaving the
computational universe open:

- state logic can run every frame;
- several start states can create parallel machines;
- graph, object, scene, application, and saved variable scopes are mutable;
- synchronous loops are available; and
- arbitrary Unity and third-party APIs are reachable.

Unity also documents that the order of the same event function across
different GameObjects cannot generally be specified, and that coroutine or
async resumption order is not guaranteed
[by the engine lifecycle](https://docs.unity3d.com/6000.0/Documentation/Manual/execution-order.html).
A locally ordered graph therefore cannot establish a globally deterministic
application.

### 4.3 Ordered event sheets: Construct and GDevelop

[Construct](https://www.construct.net/en/make-games/manuals/construct-3/project-primitives/events/how-events-work)
checks ordinary events once per tick from top to bottom; conditions and actions
within an event also run top to bottom. Trigger events are a documented
exception. Actions from earlier events immediately affect conditions and
actions later in the same tick.

[GDevelop](https://wiki.gdevelop.io/gdevelop5/events/) similarly represents
logic as conditions and actions, executes events in listed order, and runs
conditionless events every frame. Its documentation explicitly warns that
frame rates vary and that raw per-frame arithmetic therefore diverges across
machines; `TimeDelta()` corrects rate-dependent motion but does not establish
identical state replay.

These tools provide strong evidence for a compact, scan-friendly rule form.
Their rows often communicate simple logic more efficiently than free-position
wire graphs. They are not finite-state-machine languages:

- state is distributed across object, scene, and global variables;
- conditions also perform implicit instance selection;
- actions mutate live objects immediately;
- triggers and asynchronous functions have separate scheduling rules; and
- no commit, abort, outbox, or canonical replay boundary is documented.

The transferable lesson is that explicit list order can be highly legible.
The warning is that implicit object picking, frame polling, and trigger
exceptions make a short surface semantically larger than it first appears.

## 5. Visual state machines inside game engines

### 5.1 Unreal StateTree

[StateTree](https://dev.epicgames.com/documentation/unreal-engine/overview-of-state-tree-in-unreal-engine)
is a general-purpose hierarchical state machine combining state-machine
transitions with behavior-tree selection. State selection activates a
root-to-leaf path, tasks on active states execute, and success or failure can
drive hierarchical transitions.

It is valuable prior art for:

- hierarchical state organization;
- visible enter conditions and transition priority;
- task success and failure as control information;
- typed context data and bindings; and
- live inspection of the active path.

It is not an atomic reducer:

- tasks can execute concurrently and affect engine state;
- tick and task completion are ordinary stimuli;
- selectors include
  [random and utility-weighted choices](https://dev.epicgames.com/documentation/en-us/unreal-engine/state-tree-selectors-overview);
- failure chooses another control path rather than rolling back prior work;
  and
- custom conditions, evaluators, and tasks may execute Blueprint or C++.

### 5.2 Unreal Behavior Trees

Unreal
[Behavior Trees](https://dev.epicgames.com/documentation/en-us/unreal-engine/behavior-tree-in-unreal-engine---overview)
make another visual scheduling rule explicit: branches execute left-to-right
and top-to-bottom, while the runtime is event-driven rather than polling the
whole tree each frame. This is useful inspection and performance precedent,
but tasks still perform host work rather than return a transactional outbox.
Behavior Trees are decision trees with composites, tasks, services, and
decorators; they do not expose StateTree's active state-and-transition
topology.

### 5.3 Animation state machines

Animation state machines are a widespread but narrower family:

- [Unreal](https://dev.epicgames.com/documentation/en-us/unreal-engine/state-machines-in-unreal-engine)
  states produce animation poses and transition through rules and blends.
- [Unity](https://docs.unity3d.com/6000.0/Documentation/Manual/AnimationStateMachines.html)
  uses parameters, time, transition priority, blending, and interruption.
- [Godot AnimationTree](https://docs.godotengine.org/en/stable/tutorials/animation/animation_tree.html)
  connects animation states and can travel through a shortest path.

These systems demonstrate why a current state name is often insufficient.
Playback time, blend progress, source and destination, transition priority,
retained clip state, and pending transitions can all affect the next result.

They also support Uhura's existing nuance: a gesture or animation can be
encoded in a general state machine, but doing so may be the wrong authoring
model. Presentation-only progression can belong to a specialized temporal
system. If animation progress affects program behavior, it must instead become
explicit semantic state or a declared input.

### 5.4 PlayMaker and adjacent visual FSM tools

[PlayMaker](https://hutonggames.com/features.html) is a Unity visual FSM tool
whose states contain reusable actions and whose transitions are driven by
events. Its product surface includes variables, templates, custom actions,
breakpoints, state stepping, a transition log, and runtime inspection.
Its [official showcase](https://hutonggames.com/showcase.html) provides
concrete shipped-game evidence for this authoring model.

PlayMaker's durable lesson is not merely that boxes and arrows are usable. It
packages the graph with high-level domain actions, extension points, reusable
templates, error checking, and live debugging. Actions may span frames or call
arbitrary Unity APIs, and its
[event semantics](https://hutonggames.fogbugz.com/default.asp?ixWikiPage=128&nRevision1=1&pg=pgWikiDiff)
allow multiple transitions to cascade in one frame subject to a loop guard.
The system therefore does not provide a pure, atomic, replayable reaction.

Two adjacent systems expose useful choices without establishing Uhura
requirements:

- [NodeCanvas FSM](https://nodecanvas.paradoxnotion.com/documentation/?section=state-machines)
  documents one active state, author-ordered transition priority, and at most
  one transition per frame, while its action tasks still call Unity directly.
- [Godot State Charts](https://derkork.github.io/godot-statecharts/appendix)
  documents hierarchy, parallel regions, event bubbling and consumption,
  transition search, entry and exit order, and same-frame cascades, while
  signals and GDScript remain outside any rollbackable transaction.

These are precedents for questions—one versus many transitions, hierarchy,
parallel-region order, internal-event timing, and visible priority—not reasons
to add those facilities before the program corpus requires them.

## 6. Deterministic game simulation

### 6.1 Photon Quantum and Bot SDK

Quantum is a deterministic ECS and predict-and-rollback runtime integrated
with Unity while separating simulation from presentation. Clients simulate
from synchronized inputs; predicted frames may be rolled back and rerun; a
[verified frame](https://doc.photonengine.com/quantum/current/manual/frames)
is guaranteed to be deterministic and identical across client simulations.

The
[Bot SDK HFSM](https://doc.photonengine.com/quantum/v3/addons/bot-sdk/hfsm)
is the closest visual precedent reviewed here:

- authors create hierarchical states, actions, decisions, transitions,
  priorities, events, and blackboard data in a visual editor;
- the editor document is compiled into Quantum assets used by the simulation;
- the original editor XML is not required in a shipped build;
- runtime inspection can highlight recent agent flow from verified frames;
  and
- custom actions and decisions execute through the deterministic Quantum API.

The combination works because the graph is inside a larger restricted
simulation contract:

- game state is held in rollbackable frames;
- time advances by simulation ticks;
- player input is explicit;
- deterministic math, physics, navigation, and random APIs are supplied;
- presentation code is kept outside the simulation assembly; and
- replays rerun recorded input against matching assets and configuration.

The graph alone supplies none of those guarantees. Custom leaves remain C#
escape hatches constrained by the simulation boundary, and the Bot SDK is an
AI authoring system rather than one closed application reducer returning
commands.

Quantum's
[deterministic Animator](https://doc.photonengine.com/quantum/current/addons/animator/overview)
adds another useful rule. Animation that only presents game state can remain
in the view. Animation timing or root motion that affects gameplay must be
baked into rollbackable simulation state, increasing snapshot cost. The same
distinction applies to UI animation and gesture state.

### 6.2 GGPO and deterministic lockstep

[GGPO](https://raw.githubusercontent.com/pond3r/ggpo/master/doc/DeveloperGuide.md)
provides no visual language and no state machine. Its host contract instead
requires a deterministic simulation, completely encapsulated serializable
state, save and load, and the ability to advance exactly one frame without
rendering. Its synchronization test reruns saved frames and compares
checksums.

That yields a reusable conformance form:

```text
snapshot = serialize(state)
first = step(state, input)

state = deserialize(snapshot)
second = step(state, input)

assert serialize(first) == serialize(second)
```

The original Age of Empires lockstep architecture supplies production
precedent for the same discipline. Every peer simulated timestamped player
commands; recorded games replayed that command stream; small differences in
randomness, pathfinding, or hidden state could later cause large divergence.
The primary account is
["1500 Archers on a 28.8"](https://www.gamedevs.org/uploads/1500-archers-age-of-empires-network-programming.pdf).

These systems demonstrate that determinism is transitive. A deterministic
top-level reducer claim is false if clocks, iteration order, random streams,
numeric behavior, pending queues, or host state can still diverge.

### 6.3 Selective determinism in Unreal and Unity

The reviewed mainstream engines isolate rollback from visual scripting:

- Unreal's
  [Replay System](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-the-replay-system-in-unreal-engine)
  records replicated network data rather than proving input-only deterministic
  simulation replay.
- Unreal's
  [replication-order documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/replicated-object-execution-order-in-unreal-engine)
  states that some notification and cross-actor RPC order is not guaranteed.
- Unreal
  [Networked Physics](https://dev.epicgames.com/documentation/unreal-engine/networked-physics-overview)
  keeps histories, restores earlier state, corrects prediction, and
  resimulates through a specialized code-facing subsystem.
- Unity Netcode for Entities performs prediction and rollback while
  [explicitly stating](https://docs.unity.cn/Packages/com.unity.netcode%401.5/manual/prediction-details.html)
  that the package is not deterministic.

This is evidence for scoped guarantees. A runtime can benefit from snapshots
and resimulation without promising bit-identical whole-program behavior, and a
visual machine can remain useful without belonging to that runtime boundary.

### 6.4 Verse: transactional failure without visual state topology

Verse is a textual game language integrated with Unreal Editor for Fortnite,
not a visual state-machine system. It gives adjacent immediate expressions
[atomic execution](https://dev.epicgames.com/documentation/fortnite/atomic?lang=en-US)
within one simulation update: they cannot be preempted, but they do not thereby
gain rollback semantics. Separately, its
[failure contexts](https://dev.epicgames.com/documentation/fortnite/speculative-execution?lang=en-US)
support speculative execution: admitted mutation commits when the context
succeeds and is rolled back when it fails. The
[`transacts` effect](https://dev.epicgames.com/documentation/fortnite/transacts?lang=en-US)
admits rollbackable effects in that context, while `no_rollback` operations
are rejected there.

The published
[Verse core calculus](https://simon.peytonjones.org/assets/pdfs/verse-icfp23.pdf)
is designed as a deterministic functional-logic calculus. Neither property
should be widened into a claim that arbitrary UEFN programs or engine
simulation are deterministic, rollbackable, or replayable. Verse does not
inherently provide a finite-state topology, one closed input reaction, an
ordered deferred-command outbox, complete frame snapshots, or input-history
replay.

The narrow precedent is still important: a game-facing language can make
failure and transactional mutation part of its semantics without deriving
either property from a visual graph. This connects the game-engine survey to
the companion transactional-language study while keeping the two axes
separate.

## 7. Negative evidence from Godot VisualScript

Godot's official
[VisualScript retrospective](https://godotengine.org/article/godot-4-will-discontinue-visual-scripting/)
is unusually useful negative evidence.

Godot removed the general VisualScript language from Godot 4 core after it
failed to gain traction. The retrospective reports that:

- a self-reported poll of more than 5,000 respondents found 0.5% using it as
  their main engine language;
- many prospective users found textual GDScript easier than expected;
- VisualScript mostly represented the same low-level API graphically;
- unlike Unreal, GameMaker, or Construct, it lacked packaged high-level game
  components that made the visual surface useful;
- screenshot-based graph examples were expensive to create and maintain; and
- insufficient usage and feedback left no evidence-based improvement path.

Godot retained successful domain graphs such as visual shaders and
AnimationTree. Its conclusion was not that all visual authoring is useless,
but that a general graphified low-level language had not modeled a valuable
user problem.

For Uhura this is a direct warning:

- syntax substitution is not conceptual compression;
- a node for every low-level operation can be less compact than a regular
  textual grammar;
- high-level elements and actions are part of the authoring model, not merely
  a component catalog beside it;
- documentation and examples must have a maintainable canonical form; and
- human and agent learnability cannot be inferred from the absence of typed
  text.

## 8. Cross-cutting findings

### 8.1 A graph is a representation, not a semantic category

The reviewed visual forms implement different computational models:

- dependency graphs in LabVIEW;
- cyclic control networks in FBD;
- step-transition programs in SFC;
- hierarchical statecharts in Stateflow;
- imperative control flow in Blueprints and Unity Script Graphs;
- ordered reactive rules in Construct and GDevelop;
- decision trees in Unreal Behavior Trees; and
- deterministic HFSM assets in Quantum.

Calling all of them "visual scripting" hides more than it explains. A future
Uhura editor must name the model it projects.

### 8.2 Visual order has at least four meanings

Prior work uses geometry and order differently:

1. **Dependency order:** wires define a partial order; unrelated nodes may run
   in any order, as in LabVIEW.
2. **Spatial priority:** physical position can select transition or branch
   priority, as in some Stateflow and Unreal Behavior Tree configurations.
3. **List priority:** rows execute top to bottom, as in event sheets.
4. **Explicit priority:** a semantic field chooses among eligible
   transitions, independent of layout.

These choices must not be interchangeable. If layout is freely rearrangeable,
it should not silently change behavior. If order is semantic, alternate text,
table, graph, and trace projections must preserve it exactly.

### 8.3 Fixed ticks, run-to-completion events, and UI inputs differ

Games commonly update logic every simulation or render frame. Industrial
controllers commonly scan cyclically. Statecharts may wake on an event and
run until stable. Uhura's current comparison contract instead admits one typed
input.

Adopting game terminology without choosing among those clocks would leave:

- whether conditions are polled or triggered;
- whether time is state or ambient;
- whether one input may cause multiple internal transitions;
- whether transitions have a per-frame limit; and
- when external results may reenter

undefined.

### 8.4 Transition failure is not transactional abort

StateTree task failure, behavior-tree failure, an animation transition that is
not eligible, and a false statechart guard are control-flow facts. They do not
normally undo earlier state changes or host effects.

The abstract comparison contract's abort branch is stronger: it would preserve
pre-state and discard the command draft. Prior work should not be described as
matching that property merely because it exposes `success` and `failure`.

### 8.5 A debugger trace is not a replay contract

Blueprint, PlayMaker, Stateflow, behavior-tree, and Quantum editors can
highlight active or recently executed nodes. Only a stronger snapshot and
input contract explains whether the same trace can be reproduced.

Replay requires at least:

- complete serializable state;
- declared inputs and configuration;
- controlled time, randomness, and numeric behavior;
- stable scheduling and collection order;
- versioned executable assets; and
- isolation or recording of external authority.

### 8.6 Visual-only source has a human and agent cost

Graphs can reveal topology, active paths, and local dataflow exceptionally
well. They can be poor at dense expressions, global search, textual review,
small diffs, documentation, and fresh-context agent generation.

Prior work offers several alternatives:

- Stateflow provides both diagrams and compact state-transition tables.
- IEC controller ecosystems separate language semantics from PLCopen XML
  exchange and editor presentation.
- Quantum separates editor XML from compiled runtime assets.
- Godot found screenshot-only examples too costly to maintain.
- Construct and GDevelop use ordered sentence-like rows instead of free wire
  graphs.

No precedent proves the correct Uhura representation. It does establish that
the visual projection, compact human/agent source, exchange artifact, compiled
IR, and runtime trace are distinct design responsibilities.

## 9. What is established and what remains open

### Established by this study

- Visual programming and explicit state-machine authoring have durable
  production precedents in games, embedded control, and industrial
  automation.
- Visual authoring, state-machine semantics, deterministic evaluation, and
  replayable execution are independent properties.
- The reviewed game engines primarily expose specialized state-machine tools
  for animation and AI, while prediction and rollback live in separate
  subsystems.
- Deterministic game simulation depends on a restricted runtime boundary, not
  on graph notation.
- Photon Quantum is the closest reviewed game example combining a visual HFSM
  with a deterministic rollback simulation.
- Stateflow and IEC controller languages are stronger precedents for specified
  visual state and scheduling semantics.
- Immediate engine actions and uncontrolled host APIs, frame time, randomness,
  concurrency, and hidden state prevent local transition order from becoming a
  replay guarantee.
- A visual graph that merely mirrors a low-level API may fail to provide
  authoring value.

### Not established for Uhura

- that Uhura needs any visual programming surface;
- that the graph, table, or text should be canonical;
- that hierarchy, parallel regions, history states, or internal event queues
  are required;
- that one transition per input is preferable to a bounded run-to-completion
  cascade;
- that fixed ticks belong in the application machine rather than specialized
  gesture, animation, or simulation systems;
- that graphical layout may ever carry semantic order;
- that actions should be a closed built-in family, user-extensible commands,
  foreign code, or some combination;
- that all frontend logic can or should satisfy cross-platform
  simulation-grade determinism; or
- that the cost of visual documentation, source control, accessibility, and
  agent tooling is justified.

## 10. Consequences for the language comparison

Each future Uhura candidate should be evaluated in at least four projections:

1. compact canonical source for a person;
2. the same source for a fresh-context software agent;
3. a state-topology or reaction-flow visualization; and
4. a runtime trace showing admitted input, pre-state, selected reaction,
   outcome, next state, and commands.

The projections must agree on semantic identity and order. Rearranging a
non-semantic diagram must not change execution.

Each candidate should also answer:

1. What wakes a machine: a typed event, tick, time condition, task completion,
   or more than one of these?
2. Can one wake-up cause several transitions, and what bounds the cascade?
3. How are competing transitions prioritized without relying on incidental
   geometry?
4. Which clocks, random streams, queues, animation progress, and submachines
   belong to state?
5. Which actions are pure state updates, rollbackable simulation operations,
   deferred commands, or immediate foreign effects?
6. What does failure undo?
7. Can the program pass a GGPO-style save, run, restore, rerun, and checksum
   test?
8. Can animation and gesture remain specialized systems while exposing any
   behaviorally relevant progress to the machine?
9. Can documentation teach the source without requiring screenshots as the
   only executable explanation?
10. Does the visual representation compress a domain concept, or merely turn
    every textual operation into a node?

Until those tests are run against the program corpus, the narrow conclusion
stands:

> Game and industrial tools justify studying visual projections, explicit
> transition topology, high-level reusable actions, and deterministic runtime
> profiles. They do not justify treating visual scripting, state machines,
> deterministic evaluation, and replayable execution as one feature, nor do
> they demonstrate the need for an independently owned Uhura language.
