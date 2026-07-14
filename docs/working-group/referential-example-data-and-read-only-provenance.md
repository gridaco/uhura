# Referential example data and read-only preview provenance

- **Status:** Design note; read-only inspection is implemented, editing
  semantics are deferred
- **Scope:** Static examples, fixture-backed data, and the Canvas inspector
- **Authority:** Non-normative working-group material
- **Related:** [Instagram spike design, §6](instagram-spike-design.md#6-examples--example-defined-design)

## Decision for the current milestone

The Uhura editor remains read-only. Selecting a preview shows:

1. the computed top-level example data that rendered it—properties, route
   parameters, and final provided data; and
2. where each value came from, in plain language.

The inspector does not edit an example, detach a reference, or mutate shared
fixture data. This is useful on its own and provides prerequisite evidence for
any future editing design.

The UI calls this section **Example data**. It uses familiar labels such as
**Set in this example**, **From Standard sample data**, **Inherited from**, and
**Calculated by example steps**. Terms such as AST, projection snapshot, and
typed `Value` do not appear in the primary explanation.

## Current executable behavior

This section describes the implementation today; it does not propose new
syntax.

- A component or surface example supplies `props`. A page example supplies
  route `params`; pages do not have props.
- An example may bind a literal, a record of static values, or a path rooted at
  its imported fixture, such as `fixture.profiles.lena`.
- Fixtures are shared, semantically grouped example data. A reference to a
  profile or post is intentionally more meaningful than copying all of its
  fields into every example.
- An example may inherit another example with `from`; the child's bindings win.
- Page and surface examples may also pin local state and external page data.
  Derived examples replay declared events during checking and freeze the result
  before Canvas rendering.
- The checker resolves every binding and validates it against its declared
  type. Resolved component and surface previews retain computed props, state,
  and page data; resolved page previews retain route parameters, state, and
  page data.
- During that resolution, a fixture reference is replaced by a cloned, typed
  value. The checker now carries a separate, editor-only description of each
  top-level property, page parameter, and provided-data value together with its
  authored origin. Generated Canvas frames include that read-only description.
- Missing or failed provided data is represented as **Waiting for data** or
  **Couldn’t load** rather than being omitted. Local state is not included
  because replay can change it after its authored starting value.

Examples remain design artifacts and are excluded from runtime IR. Displaying
their data in Canvas must not change that boundary.

## Implemented read-only inspector

The editor build pipeline carries, beside each displayed computed value, enough
origin information to explain the top-level binding. It collects this while
checking examples, but it remains tool metadata—not a new runtime value or a
live data reference.

| Authored source | Primary inspector wording |
|---|---|
| Literal or record written in this example | **Set in this example** |
| Literal inherited through `from` | **Inherited from “first-page”** |
| Named fixture path | **From Standard sample data · Profiles · Lena** |
| Fixture binding inherited through `from` | Its sample-data source followed by **via “first-page”** |
| Inline timeline update | **Calculated by “appended” example steps** |
| Fixture-backed timeline update | Its sample-data source followed by **updated by “appended” steps** |
| Automatically supplied boot data | **Automatically from Standard sample data · Boot · Viewer** |

The panel groups data according to what the selected subject actually has:

- **Properties** for component and surface props;
- **Page address** for route parameters;
- **Provided data** for external data supplied to a page or surface.

Local state is deliberately absent from the first inspector. A replay can
change it after an authored state pin, so a single source label would not
truthfully explain the final computed value.

Values are rendered for reading rather than in source form: `true` becomes
“Yes”, records show named fields, and lists show a count with their items
available for inspection. The origin remains attached to the whole binding in
the first version. Exact field-by-field provenance inside a shared record,
transitive fixture aliases, and event-level causality are intentionally not
included.

For example, the selected profile-header preview exposes the Lena Holt
(`@lena.holt`) fixture record and the scalar properties beside it:

```text
Example data

Properties

User
  4 fields
    Display name: Lena Holt
    Username: lena.holt
  Source: From Standard sample data · Users · Lena

Viewer follows
  Yes
  Source: Set in this example
```

There are no input controls, editable-looking fields, save actions, or warnings
about side effects because this milestone performs no mutation.

## Proposed doctrine for eventual editing

Fixture binding is a feature, not temporary demo machinery. Shared entities
preserve relationships and make examples coherent. A future editor should not
force examples to copy only scalar values merely to make property controls
easy.

Editing a referenced value has three different legitimate scopes, which must
never be conflated:

1. **Override this preview** — explore a typed value locally without changing
   source.
2. **Fork this scenario** — create a distinct fixture-backed case and retarget
   this example.
3. **Edit shared example data** — change the referenced entity and deliberately
   affect every consumer.

Designers are trusted authors and may eventually use all three operations.
Responsibility comes from making scope, dependencies, and consequences visible,
not from silently limiting the data model to primitives. An LLM may help author
the source, but it does not remove the need for explicit edit scope.

A separate future concept, **example intent**, may state what an example is
meant to prove—for example, that a public-profile example remains public. Type
checking protects data shape; origin and usage information explain blast
radius; example-intent checks would protect the authored meaning. No syntax for
intent is chosen here.

## Deferred work and non-goals

This note does not define or implement:

- inspector editing or session-local overrides;
- source rewriting, save behavior, undo, or formatting;
- fixture forking or mutation;
- affected-preview analysis for shared edits;
- example-intent syntax or validation;
- leaf-level provenance inside records and lists;
- computed local-state provenance;
- precise attribution of a derived state field to one replay event;
- authored JSON fixtures or a new general-purpose data-source abstraction;
- component-instance selection inside an already evaluated page; or
- any change to Uhura Core, runtime IR, Play providers, or Spock authority.

If editing is pursued later, it requires a focused proposal informed by the
read-only provenance model and its use in real examples. A polished demo alone
is not sufficient reason to freeze those semantics.
