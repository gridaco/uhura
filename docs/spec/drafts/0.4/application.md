# Uhura 0.4 application profile

- **Status:** Active candidate application boundary
- **Core source:** [Source and lowering](source.md)
- **Kernel:** [Source-neutral kernel](kernel.md)
- **Project and identities:** [Project, resolution, and identity](project.md)
- **Validation:** [Conformance](conformance.md)

The core language is complete without this profile. This document adds Web UI,
explicit framework vocabulary, host bindings, evidence, and editor-facing
artifacts without adding another state-transition model.

## 1. Web UI activation

The profile name is fixed as `ui`. Candidate source activates it through the
same inert `use` model as other vocabulary:

```uhura
use uhura::ui;

use crate::counter::BoundedCounter;

pub ui CounterWeb for BoundedCounter(view) {
  <main>
    <output>{view.count}</output>

    <button
      disabled={view.at_maximum}
      on press -> Increment
    >
      Increment
    </button>
  </main>
}
```

The parser recognizes `ui` as a contextual declaration form, but the checker
admits it only when the explicit profile path resolves. `use uhura::ui;` is
inert: it runs no module, selects no renderer, and grants no browser authority.
This keeps profile activation inside the Rust-shaped module pattern instead of
adding a second pragma system. `pub ui ... for Machine` is a binding
declaration, not a trait implementation or machine construction.
`BoundedCounter(view)` binds the renameable immutable local `view` to
`BoundedCounter::Observation`. It allocates no instance and grants no access to
configuration, private state, drafts, or dependency handles.

Activation is lexical and exact. Every logical module containing a `ui`
declaration must directly contain unaliased `use uhura::ui;`, resolved to the
standard profile identity. `use uhura::ui as other;`, `pub use uhura::ui;`, or
using a package re-export does not activate the current module or any consumer.
This direct-use rule is checked before admitting contextual `ui` grammar and
prevents transitive or accidental profile activation.

Web is the only presentation target. `ui` may include HTML semantics,
Uhura-owned widgets, CSS, checked accessibility contracts, surfaces, and
semantic interaction bindings. It does not expose the mutable DOM object
model to machine source.

The exact finite element vocabulary currently admitted by this profile is the
[0.4 checked UI catalogue](../../../widgets/drafts/0.4/). That catalogue is an
executable incubation contract, not a supported or stable widget API.

## 2. Svelte-shaped presentation

The candidate reuses:

- HTML-shaped elements and attributes;
- `{expression}` interpolation;
- `{#if}`, `{:else}`, and `{/if}`;
- keyed `{#each}` repetition.

A core `match` expression may appear inside an interpolation. No separate
Svelte-like match block is selected by 0.4.

Reusable presentation invocation and scoped or imported CSS syntax are not
selected by 0.4. A presentation-shaped tag that resolves through `use` is
diagnosed rather than treated as a component call. Checked standard extensions
such as `Link` and `Surface` are finite catalogue entries, not a general
component protocol.

The body is a pure projection of the referenced machine observation.
Expressions inside braces use Uhura's Rust-shaped core expression language,
not JavaScript. The body cannot:

- mutate machine state;
- invoke an update directly;
- emit a command;
- read private state, drafts, inboxes, ports, or receipts;
- run JavaScript;
- create a callback, promise, subscription, or lifecycle effect; or
- access browser authority.

`on event -> input` constructs exactly one admitted semantic input. The arrow
is deliberately not Svelte's JavaScript callback spelling:

```uhura
<input
  value={view.search.query}
  on input -> search.Changed(event.value)
/>
```

The right side is a checked input constructor. It cannot contain a statement
block or perform an effect. Within that expression only, contextual immutable
name `event` contains the element or widget event payload declared by the
resolved UI contract. A payloadless UI event supplies `Unit`; an unknown field
or use of `event` outside the binding is rejected. The target constructor fixes
the required machine-input payload type, so the checker validates the entire
bridge without creating a callback.

This specimen uses the standard HTML-shaped `input` contract supplied by the
`ui` profile and recorded in the checked UI catalogue. Its semantic input
payload exposes `value: Text`.

### Refusal presentation

UI reads committed observation, never receipts or an aborted draft. An abort
outcome is therefore a classification and rollback result, not a presentation
channel. If rejected user intent must change visible UI, the machine must make
that state explicit and select a commit-policy outcome:

```uhura
notice.updates.show("Sign in to search");
Accepted
```

Alternatively, a later host refusal input may commit a modeled refusal state.
Writing notice state and then returning an abort-policy outcome discards the
notice by definition. A domain may name a commit-policy outcome `Blocked` when
it needs both a blocked classification and visible committed feedback; the
policy, not the constructor's English name, decides publication.

## 3. UI declarations do not own state

A 0.4 UI declaration is one named pure projection bound to one machine's
observation type. A host or evidence example may select that declaration for a
machine instance. Another UI declaration cannot invoke it, pass values to it,
or supply slots or children. Naming or selecting it does not allocate a
machine, part, store, inbox, or semantic lifetime.

State ownership remains in the machine or an explicitly composed part. A
component-local physical concern such as hover, pressed pose, caret, IME,
measurement, or animation frame remains renderer state unless promoted through
a named semantic contract.

Reusable UI composition remains an unselected language-design gate. Any later
proposal must restate its value, event, child, identity, and expansion
semantics, follow the Svelte-shaped parent pattern, and avoid creating another
behavior or state-ownership language.

## 4. Framework features

Activating `ui` does not activate a meta-framework. Each meaningful feature is
an explicit `use`:

```uhura
use uhura::web_router::{Link, Router};
use uhura::web_location::SearchParams;
```

A resolved feature must declare:

1. its exact types, values, elements, observations, inputs, commands, and
   optional syntax;
2. the facts it owns and the external facts it only observes;
3. its lowering into the machine, UI, renderer, or host boundary;
4. required host capability and admission behavior;
5. evidence and static-preview representation; and
6. diagnostics, availability, fallback, and compatibility.

Use declarations are inert. A `Link` use does not install interception, bind
browser history, prefetch, or grant navigation authority. A machine port
separately requires the browser capability:

```uhura
port router = Router<Location> { routes: return_routes };
```

The live host binds it before admission. Location changes return as later
qualified inputs; navigation requests publish as commands.

Filename and directory conventions are not ambient language semantics.
Projects may use a framework tool to generate `use` declarations and
composition source, but the checked program must contain or resolve every
semantic feature explicitly.

## 5. Host boundary

`host.toml` selects one live deployment only after project resolution and
source checking succeed. It is not a source module, dependency lock, or
machine-language feature.

The complete 0.4 schema is:

```toml
[entry.return-desk]
machine = "crate::ReturnDesk"
presentation = "crate::ReturnDeskWeb"
lifetime = "application-session"
stylesheet = "styles/theme.css"

[entry.return-desk.ports]
router = "web.history"
"feed.api" = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"

[entry.return-desk.provider.config]
endpoint = "https://example.invalid"
```

The only root key is `entry`. It contains exactly one entry in 0.4. Supporting
several deployment entries or selecting among them is a later host feature,
not an implicit property of the table syntax.

The entry name is a lowercase kebab-case local deployment identity. An entry
admits exactly:

- required `machine`;
- optional `presentation`;
- required `lifetime`;
- optional `configuration`;
- optional `ports`;
- optional `stylesheet`; and
- optional `provider`.

`machine` and `presentation` use the module-independent host selector defined
by [Project and identity §2](project.md#host-declaration-selector):

```text
crate::PublicName
dependency_alias::PublicName
```

The resolver replaces the selector with its canonical `PublicId` before
admission or hashing. A logical-module route such as
`crate::app::ReturnDesk` is rejected in `host.toml`; moving source between
modules cannot require a host-manifest edit.

`presentation`, when present, must be a public `ui` declaration bound to the
selected machine. `lifetime` is exactly `"application-session"` in 0.4.

`configuration` is an exact canonical tagged Uhura JSON string. It is omitted
if and only if the selected machine configuration is `Unit`; otherwise it is
required. Admission parses it, verifies exact canonical spelling, decodes it
against the complete configuration type, and hashes the decoded typed value
rather than the source string.

The `ports` table maps every required
[port locator](project.md#port-locator) to one host adapter identity. Root
ports may use an unquoted TOML key. A part-owned dotted locator must be quoted
so TOML does not reinterpret it as a nested table:

```toml
[entry.return-desk.ports]
router = "web.history"
"feed.api" = "app.provider"
```

The table must contain the exact complete port-locator set, with no missing,
duplicate, or unknown key. Adapter identity is one or more lowercase
kebab-case segments separated by `.`. Binding order is nonsemantic; the host
sorts bindings by canonical port-locator bytes.

For every binding, admission verifies:

- the resolved required contract identity;
- its exact generic arguments and immutable port configuration;
- its receive and send domains;
- its canonical codecs;
- the adapter's admitted contract-instance hash; and
- any exclusivity rule of the host capability.

Importing a contract never selects an adapter. A host adapter cannot
synchronously re-enter a reaction, forge a declared outcome, or turn a
process failure into domain data.

`stylesheet`, when present, is a safe project-relative UTF-8 file. `provider`
admits exactly required `module` and optional `config`. `module` is a safe
project-relative regular file. Provider configuration admits only recursively
deterministic TOML Boolean, integer, text, array, and table values; floating
point, date/time, and other non-Uhura values are rejected.

A provider module is host adapter implementation, not code called from a
machine or `ui` expression. It remains outside the reaction, cannot re-enter,
and communicates only through admitted port envelopes. This does not select a
general source-language foreign-call escape hatch.

### Deployment admission

One entry names one complete lowered machine and optional presentation.
Admission is atomic:

- every required port is bound exactly once;
- adapter contracts and codecs match exactly;
- `use` declarations never select adapters;
- providers cannot synchronously re-enter a reaction; and
- adapter or process failure cannot masquerade as a declared domain outcome.

A port owned by a part is bound by its stable composition path, not its source
file. For example, a port declared as `api` in part `feed` is addressed as
`feed.api` in the entry's port table. Moving the part declaration between
files does not rename the host requirement.

Core checking, evidence, and structural Editor inspection do not require a
valid `host.toml`. Play and any other live deployment do.

### Deployment identity

The host computes:

```text
DeploymentId = sha256(
  frame(
    "uhura-deployment/0",
    resolved machine PublicId,
    MachineProgramId,
    optional resolved presentation PublicId,
    optional PresentationId,
    entry name,
    lifetime,
    canonical typed configuration,
    sorted admitted port bindings,
    selected resource and provider content
  )
)
```

Each admitted port-binding record contains its canonical port locator, adapter
identity, required contract hash, and admitted contract-instance hash.
Selected stylesheet and provider material contains protocol identity,
deterministic configuration, and content hash. Physical resource paths are
provenance and do not enter `DeploymentId`.

Deployment hashing uses resolved declaration identities, never the authored
`crate` or dependency-alias spelling. It canonicalizes maps and bindings
before hashing. Reordering TOML keys, moving a provider or stylesheet without
changing its selected contents, moving machine source, or renaming a logical
module therefore preserves deployment identity.

Changing entry name, machine program, presentation, typed configuration,
adapter identity or admitted contract instance, lifetime, provider
configuration or contents, or selected stylesheet contents changes
`DeploymentId`.

Evidence identity is separate and does not enter `DeploymentId`. A runtime
instance identity is supplied by the host after deployment admission; the
standalone host uses `entry/<entry-name>`. It is not a machine, presentation,
or source identity.

## 6. Evidence and static examples

Evidence is tooling source outside the deployment graph. It uses the same 0.4
frontend, expression language, imports, and logical-module identity as core
source. The manifest role is the capability boundary: evidence modules may
declare only `scenario`, `checkpoint`, and `example`, while deployable modules
may not declare them. There is one evidence spelling and no compatibility
loader.

Its semantic operations are:

- create a fresh machine from complete configuration and sealed fixtures;
- restore a complete checkpoint;
- enqueue one local input;
- deliver one qualified port input;
- expect one outcome and exact ordered commands;
- inspect committed observation and lifecycle;
- pin a reachable committed state;
- name a checkpoint; and
- bind a pin to a UI example.

Evidence cannot:

- assign arbitrary private state;
- claim an unreachable state is reachable;
- mark a pending command as settled without a delivery;
- perform live network or host work;
- bypass admission; or
- create a different interpreter.

A static example identifies a complete, honest machine state plus an optional
physical preview pose. Physical scroll, animation, focus, and media pose
remain separately labelled renderer-preview data. They do not enter the
semantic checkpoint unless the machine explicitly owns them.

## 7. Editor and inspection contract

Flattening source must not flatten the author experience. The compiler retains
the canonical
[provenance sidecar](project.md#6-semantic-node-and-source-provenance)
sufficient for the editor to show:

- module and part hierarchy;
- state ownership and visibility;
- events and their unique handlers;
- update, draft-read, and committed-observation dependencies;
- port and command edges;
- UI input edges;
- invariant and outcome policy;
- receipt attribution by authored owner; and
- complete global checkpoint state with namespaced authored projection.

Receipts remain globally ordered. Part attribution is provenance, not an
independently ordered trace. The interaction graph may group nodes by source
part without inventing runtime instances.

Static examples may author or display namespaced values, but tooling must
materialize a complete admitted global state before rendering.

The Editor and Play consume one checked semantic program and the same
provenance sidecar. Physical source moves may change source occurrences and
the source-revision identity, but cannot create new graph-node identities or
change the machine, presentation, or deployment identity when their resolved
semantic contents are unchanged.

## 8. Foreign escape boundary

The need for custom foreign integration is accepted as a design requirement;
its executable form is not selected by this candidate. Any future escape
boundary must:

- be explicit at the project and source call site;
- declare trust, determinism, replay, capability, and failure effects;
- be impossible to activate accidentally through ordinary vocabulary;
- preserve the no-synchronous-reentry rule;
- distinguish checked typed adapters from unchecked foreign execution; and
- remain visible in receipts, diagnostics, examples, and deployment review.

Inline JavaScript in machine or UI source is not part of 0.4. The existing
[escape-hatch study](../../../studies/escape-hatches-and-foreign-bindings.md)
remains the non-normative design input.

## 9. Application exclusions

This profile does not select:

- a permanent or general-purpose widget system beyond the finite checked
  incubation catalogue;
- file-system routing;
- server components;
- automatic data loading;
- a browser-global store;
- arbitrary npm packages;
- runtime child-machine lifecycle;
- DOM callbacks or Svelte actions; or
- inline foreign code.

Each may be studied independently. None is implied by `ui`.
