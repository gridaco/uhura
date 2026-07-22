# RFC 0003 — Source comments, declaration docs, and markup annotations

- **Status:** Accepted
- **Implementation:** Partial — 0.4 sibling-list markup comments, annotation
  attachment, and checked authoring projection are implemented; the exact
  non-sibling `UH0001` placement gate, DSL ordinary-comment attachment, and
  declaration documentation remain pending
- **Scope:** `.uhura` ordinary comments, declaration documentation, tagged
  markup annotations, attachment, canonical formatting, checked authoring
  metadata, and diagnostics. The accepted 0.3 `.examples.uhura` target maps to
  ordinary 0.4 evidence modules selected explicitly by `[evidence.modules]` or
  discovered as subject-local `*.examples.uhura` by `web-app@1`.
- **Supersedes:** None
- **Related work:** [RFC 0001](0001-project-foundation.md),
  [Spock RFD 0016](https://github.com/gridaco/spock/blob/main/docs/rfd/0016-doc-comments.md)
- **0.4 reconciliation:** [Core source and lowering](../spec/drafts/0.4/source.md),
  [normative grammar](../spec/drafts/0.4/grammar.ebnf)

## 1. Proposal

Accept three deliberately different source tiers:

1. an **ordinary comment** is formatter-preserved source trivia;
2. a **doc comment** describes a declaration or declared member; and
3. a **markup annotation** records an ordered, kinded note about one precise
   markup occurrence.

The forms are:

| Source region | Ordinary comment | Declaration doc | Markup annotation |
|---|---|---|---|
| DSL regions | `// …` at a comment-bearing boundary | `/// …`, with `//! …` for the file | None |
| Markup | `<!-- … -->` | None | `<!-- @kind … -->` |
| Historical 0.3 `<style>` body | CSS syntax, outside this RFC | None | None |

For example:

```uhura
//! Feed application source module.
use uhura::ui;

/// The feed's deterministic state and input contract.
pub machine Feed {
  events {
    RetryReload,
  }

  outcomes {
    commit Accepted,
  }

  state {
    /// Whether a reload command is unsettled.
    reload_pending: Bool = false,
  }

  // The guard prevents duplicate commands.
  on RetryReload {
    if !reload_pending {
      reload_pending = true;
    }
    Accepted
  }
}

/// The Web projection of Feed observation.
pub ui FeedWeb for Feed(view) {
<!-- @annotation The primary recovery action in the failed state. -->
  <button on press -> RetryReload>Retry</button>
}
```

Declaration docs and markup annotations are different metadata classes even
when they use the same word. These are both valid annotations and retain their
literal, distinct kinds:

```uhura
<!-- @doc The primary action. -->
<button />

<!-- @annotation Confirm whether this remains visible while pending. -->
<button />
```

`@doc` does **not** turn an element occurrence into a documented declaration.
It is an annotation whose kind is `doc`. `@annotation` is the conventional
general-purpose kind for localized notes, while authors and tools may choose
more specific lower-kebab kinds such as `@rationale`, `@review-note`, or
`@todo`.

Docs and annotations are checked **authoring metadata**. They do not enter
canonical runtime IR or semantic view data and do not change evaluation.
Ordinary comments never enter authoring metadata.

### 1.1 0.4 reconciliation and history

RFC 0003 was accepted against Uhura's 0.3 page/component/store surface. The
metadata taxonomy, lexical sigils, normalization, forward attachment,
semantic-inertness, diagnostics, and separate authoring projection remain the
accepted decision. Uhura 0.4 replaces only the active `.uhura` target
vocabulary and comment-bearing boundaries with those of its machine/part
grammar and activated `ui` profile.

The historical mapping is explicit:

| Accepted 0.3 target | Active 0.4 treatment |
| --- | --- |
| `component` / `page` / `surface` header | `machine`, `part`, or activated `ui` declaration, according to what is declared |
| `props` and route parameters | Exact typed parameters on pure `ui` components; routes remain committed machine `Location` state observed by pages |
| `emits` event and payload | Finite `emits` protocol on a pure `ui` component, mapped outward at every call; machine/part inputs remain separate `events` entries |
| `store` scope | Removed; state belongs directly to a machine or part |
| Store state field | Machine/part state field |
| Event/outcome handler | Machine/part `on` handler |
| `{#match}` block | Removed; 0.4 uses core `match` expressions and does not annotate an expression as a markup occurrence |
| Component invocation | Checked UpperCamelCase `ui` call; the call occurrence is an annotation target with the full call-element span |
| Inline `<style>` region | Removed; 0.4 selects external CSS through `host.toml`, outside Uhura source metadata |

Git history preserves the original spelling and rationale; this reconciliation
does not pretend those forms were always 0.4 forms. Later references to the
0.3 component header or inline `<style>` describe that historical target set,
not the current grammar. Current component-call annotation follows the active
0.4 application profile. Evidence uses the same 0.4 frontend under a
manifest-enforced tooling role. Its file and example documentation rules below
remain part of this RFC's accepted metadata decision.

## 2. Motivation

Uhura source serves two different explanatory needs.

A declaration has a durable contract. A machine, part, nominal type, state or
observation field, handler, update, or parameter benefits from documentation
that follows it through checking and extraction. Spock's `///`/`//!` taxonomy
already gives this kind of prose a small, deterministic source form.

A markup element is different. It is one occurrence in an implementation,
often repeated or conditionally present. Treating every local note as the
element's declaration documentation erases that distinction. A tagged
annotation instead explains why that occurrence exists, records a review note,
or gives an authoring tool a precise target. Its kind communicates author
intent without changing its semantic class.

Ordinary comments remain necessary for implementation prose, visual
separators, and temporary explanation that should not become tool-visible
metadata.

The language must distinguish all three during parsing. Recovering intent from
rendered HTML, comment wording, or adjacency after lowering would be ambiguous
and renderer-dependent.

### 2.1 Prior art and the adopted boundary

The syntax follows existing practice without inheriting another framework's
metadata model:

- [XML comments](https://www.w3.org/TR/xml/#sec-comments) provide the familiar
  `<!-- … -->` carrier and well-formedness rules, but XML processors are not
  required to expose comment text to applications. Plain XML comments are
  therefore insufficient as Uhura's checked authoring contract.
- [JSX](https://facebook.github.io/jsx/) defines children as text, elements,
  fragments, or braced ECMAScript expressions. Its common `{/* … */}` form is
  consequently a host-language comment inside an expression, not a portable
  JSX documentation or source-target mechanism.
- [Svelte](https://svelte.dev/docs/svelte/basic-markup#Comments) demonstrates
  that tagged HTML comments can carry tool meaning: `svelte-ignore` applies to
  the next markup block, while `@component` supplies component documentation.
  Uhura adopts the explicit marker and forward attachment, but makes
  declaration docs and occurrence annotations separate metadata classes.
- [XML Schema](https://www.w3.org/TR/xmlschema-1/structures.html#cAnnotations)
  separates human `documentation` from machine `appinfo` inside an
  `annotation` schema component. That distinction is useful prior art for
  declaration metadata, but adding annotation elements to Uhura would pollute
  its closed runtime view tree.

The resulting rule is deliberate: declarations use the Spock-shaped doc tier;
markup occurrences use an XML-shaped, tagged annotation tier; and neither is
recovered from an ordinary comment after parsing.

## 3. Scope and non-goals

This RFC defines:

- ordinary comments in DSL and markup regions;
- Spock-shaped declaration docs in DSL regions;
- open, tagged annotation kinds inside markup;
- text normalization and canonical formatting;
- forward attachment and scope boundaries;
- documentable and annotatable target classes;
- the logical checked authoring-metadata projection;
- semantic-inertness requirements; and
- stable diagnostics reserved for this subsystem.

This RFC does not define:

- Canvas cards, pins, leader lines, DOM anchors, placement, or interaction;
- annotations outside markup, such as `// @kind …`;
- Markdown or another annotation-body markup language;
- authors, timestamps, replies, resolution, permissions, or collaborative
  threads;
- durable re-anchoring across arbitrary source edits;
- visibility, redaction, publication, or export policy;
- documentation inside port TOML, catalogs, manifests, fixtures, or CSS;
- annotation of individual attributes, event bindings, expressions,
  interpolation runs, match arms, or CSS rules; or
- a wire filename or JSON encoding for authoring metadata.

Port-contract declarations outside `.uhura`, catalog schemas, and their
documentation remain separate decisions. Core 0.4 `struct`, `enum`, and `key`
declarations are local source targets and are covered here.

## 4. Terminology and invariants

An **ordinary comment** has source text and a span but no semantic target. The
formatter preserves it at a closed comment-bearing item or list boundary.
Comments between arbitrary tokens of one expression, type, declaration, or
statement are not legal source.

A **doc comment** is `//!` or `///` in DSL mode. It attaches to one
documentable target and produces that target's optional documentation string.
Documentation is singular per target.

A **markup annotation** is `<!-- @kind … -->`. It attaches to one annotatable
markup target. Annotations are ordered and repeatable, including repeated
annotations of the same kind. The kind is an opaque label, not a schema or a
runtime directive.

A **source metadata item** is the normalized checked representation of one doc
or annotation. It retains both the metadata source span and its target's source
identity.

In this RFC, **horizontal whitespace** means ASCII space (U+0020) or tab
(U+0009), and a **blank line** contains only horizontal whitespace. CRLF and
bare CR line endings normalize to LF before comment-body classification.
Whitespace around an annotation marker means horizontal whitespace or LF;
other Unicode whitespace remains payload text.

The following invariants are mandatory:

1. Ordinary comments, docs, and annotations are lexically distinguishable
   without type checking or rendering.
2. A metadata item has exactly one source target; it is never inferred from
   prose, tag name, class, rendered text, or DOM structure.
3. Ordinary comments and metadata are trivia for structural validation. They
   do not count as markup nodes, children, roots, statements, handlers,
   expressions, or toward any bounded construct count.
4. Adding, removing, reordering, or editing valid metadata may change source
   revision, source spans, and authoring-metadata output, but not canonical
   program IR, program hashes, checkpoints, observations, receipts, commands,
   traces, or runtime diagnostic codes, messages, and semantic outcomes.
   Diagnostic source locations may shift with the surrounding text.
5. Annotation order is target-local source order and deterministic.
6. Doc and annotation bodies are inert UTF-8 text. Braces, tags, backticks,
   and `@` within a payload have no nested language meaning.

## 5. DSL comments and declaration docs

### 5.1 Lexical classification

In every DSL lexer region, the lexer classifies line comments exactly as Spock
does. This includes core module and machine/part source, examples source, and
DSL streams inside activated UI interpolation, braced attribute values, event
bindings, arguments, and structural block heads:

- `//!` is an inner file doc;
- exactly `///`, when not followed by a fourth `/`, is an outer doc;
- `////` and any longer slash run is ordinary; and
- every other `//` line is ordinary.

The following are therefore distinct:

```uhura
// ordinary
// @todo also ordinary in DSL mode
//// ordinary divider
/// declaration documentation
//! file documentation
```

`/// @todo …` is declaration documentation whose text begins `@todo`; Uhura
does not introduce a tagged DSL annotation grammar.

Classification does not imply valid placement. In particular, a non-empty
`///` run in a braced markup expression is still doc metadata, then receives
`UH0019` because an expression is not documentable. A non-empty `//!` there
receives `UH0018` because the file preamble has ended. An entirely empty
normalized doc run emits no metadata and receives no placement diagnostic.

### 5.2 Ordinary-comment placement

The comment-bearing DSL boundaries are closed:

| Containing context | A comment may occur immediately before |
|---|---|
| Module preamble/body | a complete `use`/`pub use`; a complete top-level declaration; an activated DSL-to-markup/style transition; EOF |
| Machine or part body | a complete member declaration or grouping head; `}` |
| `struct` body | a field after the preceding comma; `}` |
| `enum` body or record-variant body | a variant or field after the preceding comma; `}` |
| Parameter or protocol-payload list | the first parameter; a later parameter after the preceding comma; `)` |
| `config`, `events`, `commands`, `outcomes`, `requires outcomes`, `state`, `observe`, or invariant body | an entry after the preceding comma; `}` |
| Function, handler, update, reconciliation, `if`, or loop body | a complete statement or final expression; `}` |
| `match` body | a complete arm after the preceding comma; `}` |
| Examples module | a complete top-level `use` declaration; a named example; EOF |
| Example body | a complete example clause; `}` |

Import braces, part/port/constructor/ordinary call arguments, tuple and
collection literals, struct constructions and patterns, example-clause
sub-lists, types, expressions, conditions, event bindings, and the interior
token sequence of any declaration, parameter, statement, arm, or other
complete item are not comment-bearing boundaries. A comment also may not occur
between an item and its separating comma. Such placement receives the existing
`UH0001 syntax/unexpected-token`, with a repair that moves it to the nearest
owning boundary.

The parser retains a valid comment as leading trivia of the following complete
item, or as trailing trivia of the containing list at `)`, `}`, or EOF. A
comment at the DSL-to-markup transition is module trailing DSL trivia and is
emitted immediately before the first markup node. A source
trailing comment such as `item // note` is therefore legal only when the next
position is one of the listed boundaries; it canonicalizes to its own line
there. `// @kind …` follows these same rules and remains ordinary in DSL mode.

### 5.3 File docs

`//!` is legal only in the file preamble. The preamble ends at the first
non-comment syntactic item: the first `use`, `pub use`, top-level declaration,
or activated-profile declaration in a deployable `.uhura` module, or the first
`use`/`example` item in an evidence module. Whitespace, ordinary comments, and
other `//!` lines may coexist before that item.

In Uhura 0.4, `//!` documents the source module. In an explicit project, an
ordinary `.uhura` file becomes an evidence module only when mapped under
`[evidence.modules]`; a filename suffix alone has no meaning. The opted-in
`web-app@1` resolver additionally assigns the evidence role to its closed set
of subject-local `*.examples.uhura` files. File docs do not replace `///`
documentation for a machine, part, type, value, `ui`, scenario, checkpoint, or
example declared inside that module.

A non-empty `//!` after the preamble is
`UH0018 syntax/misplaced-inner-doc`.

For authoring metadata, the source-module target span is the full source-file
span from byte `0` through the file length. The `//!` run retains its own
smaller source span separately.

### 5.4 Outer docs and targets

`///` attaches to the next documentable item in the same syntactic item list.
Whitespace and ordinary comments are transparent. It never skips an
incompatible item to find a later compatible one and never crosses `}`, a
section boundary, a parameter-list open or close, transition into markup, or
end of file. A parameter doc must occur inside its parameter list, immediately
before the parameter it documents.

A doc run is a maximal sequence of the same doc form whose lines are separated
only by whitespace or ordinary comments. Its doc lines join in source order;
intervening ordinary comments do not contribute text. The opposite doc form
ends the run. Two independent non-empty doc runs that resolve to the same
target are an incompatible-target diagnostic; the checker does not merge them.
An empty normalized run emits no metadata but remains a run boundary; it can
therefore separate two non-empty runs of the opposite form. The empty run
itself never receives `UH0017`, `UH0018`, or `UH0019`; any two surviving
non-empty runs are checked independently.

A doc run's metadata span is the half-open byte envelope from the first doc
sigil through the end of the final doc token. It excludes the final line
terminator when one exists and may contain whitespace or ordinary comments
that were transparent while forming the run.

The documentable target table is closed:

| Target | Doc form |
|---|---|
| Source module | `//!` in the preamble |
| `machine`, `part`, activated `ui`, `struct`, `enum`, `key`, `const`, or `fn` declaration | `///` before the declaration |
| Part parameter or function parameter | `///` before the parameter |
| Configuration field | `///` before the field |
| Struct field, enum variant, or enum-variant field | `///` before the field or variant |
| Event, command, outcome, or required-outcome entry | `///` before the entry |
| Protocol payload parameter | `///` before the parameter |
| Part composition or port declaration | `///` before `part` or `port` |
| State field | `///` before the field |
| `computed` declaration or observation field | `///` before the member or field |
| Event handler | `///` before `on` |
| `update` declaration or update parameter | `///` before the update or parameter |
| Root reconciliation block | `///` before `before commit` |
| Named example declaration | `///` before `example` |

Imports, grouping sections, requirements, invariants, handler pattern binders,
statements, expressions, match arms, example clauses, markup occurrences,
style blocks, and CSS are not documentable.

Parameter docs use the existing comma-delimited parameter list. When any
parameter has docs or an ordinary comment, the canonical list is multiline:

```uhura
events {
  LikeToggled(
    /// The post whose state changed.
    post: PostId,
    /// The requested presentation state.
    now_liked: Bool,
  ),
}

update record_toggle(
  /// The post whose state changed.
  post: PostId,
  /// The requested presentation state.
  now_liked: Bool,
) {
  // ...
}
```

A doc before `)` with no parameter is dangling. Handler pattern binders are
not independently documentable; document their declared protocol payload
parameters instead.

### 5.5 Doc text normalization

For every `//!` or `///` line:

1. remove the three-character sigil;
2. remove at most one immediately following ASCII space;
3. remove trailing horizontal whitespace; and
4. normalize the source line ending to LF.

Join the resulting lines with LF and remove all trailing empty lines from the
logical run. Interior empty lines remain. A run whose normalized text is empty
emits no metadata but retains the run-boundary behavior defined in §5.4. The
canonical formatter omits that run's doc-sigil lines; any interleaved ordinary
comments remain at their legal boundary. This permits an empty run in a token
interior to be a true no-op without requiring token-interior comment layout.

## 6. Markup comments and annotations

### 6.1 Annotation-kind grammar

An annotation kind uses Uhura's lowercase kebab identifier shape and contains
between 1 and 64 ASCII bytes:

```text
annotation-kind := lower (lower | digit)* ("-" (lower | digit)+)*
lower           := "a" … "z"
digit           := "0" … "9"
```

Kinds are case-sensitive ASCII. Every well-formed kind is accepted and
preserved exactly. `doc`, `annotation`, `rationale`, and `review-note` have the
same language behavior; their difference is author/tool vocabulary. Uhura Core
does not interpret any of them.

No registry, attributes, parentheses, key/value fields, interpolation, or
directive mini-language is introduced. A tool assigning additional meaning to
a kind owns that separate contract.

### 6.2 Classification

XML-shaped comments are legal only where a markup node could occur in a
markup sibling list, including the trailing ordinary-comment position before a
parent or block-arm close. They are not legal inside an opening or closing tag,
an attribute list, a braced DSL expression, or a structural block head. Those
embedded DSL regions use the §5 rules instead. A lexically well-formed
XML-shaped comment in any non-sibling position receives
`UH0001 syntax/unexpected-token`; `UH0016` is reserved for malformed comment
bodies and annotation markers.

A markup comment whose first non-whitespace body content does not begin with
`@` is ordinary:

```uhura
<!-- Ordinary implementation comment. -->
```

A body beginning with `@kind`, followed by whitespace and a non-empty payload,
is an annotation:

```uhura
<!-- @doc The primary action. -->
<button />

<!-- @rationale
The destructive action is separated from the primary controls.
It remains visible because recovery is immediate.
-->
<button />
```

An `@` later in an ordinary comment does not promote it. If the first
non-whitespace content begins with `@` but the kind is malformed or the
normalized payload is empty, the comment is malformed rather than ordinary.

### 6.3 XML-shaped well-formedness

Markup comments begin with `<!--` and end with `-->`. Their body MUST NOT
contain `--` and MUST NOT end in `-`, which would form the invalid closing
sequence `--->`. Unterminated or otherwise malformed bodies are diagnosed.

Entity references and Uhura interpolation are not interpreted inside comment
bodies.

### 6.4 Text normalization

After line endings are normalized to LF, an ordinary one-line comment removes
all leading and trailing horizontal whitespace from its body. An ordinary
multiline comment is normalized by:

1. removing all blank first and last body lines;
2. removing trailing horizontal whitespace from every remaining line;
3. removing the common ASCII-space indentation shared by all non-empty lines;
   and
4. preserving interior line breaks and blank lines.

An annotation first removes leading whitespace before `@kind`. The kind MUST
be followed by at least one ASCII space, tab, or LF; remove the marker and the
first such separator. Its remaining body uses the same normalization. The
normalized annotation payload MUST be non-empty.

The formatter may retain an empty ordinary comment, but an empty tagged
annotation is malformed because it would create metadata without prose.

## 7. Attachment and target compatibility

### 7.1 Forward attachment

Docs and annotations use forward, outer attachment. A metadata item binds to
the next target in the same syntactic item or markup sibling list. Whitespace
and ordinary comments are transparent. Metadata never attaches backward.

Metadata never crosses any of these boundaries:

- `}`, a DSL section close, or end of file;
- a parameter-list open or close;
- transition from the DSL region into markup;
- `</element>`;
- `{:else}` or a block close.

Reaching a closing delimiter, arm boundary, region transition, or EOF without
encountering any construct to target is dangling. Encountering an incompatible
construct is incompatible; metadata does not skip it in search of a later
target. An opening delimiter encountered after an ineligible construct has
already begun belongs to that incompatible construct and is not a dangling
case. Closing delimiters, arm labels, and region-transition markers such as
the DSL-to-markup transition are boundaries, not incompatible target
constructs.

A parameter doc is valid only after the parameter-list open and immediately
before its parameter. A doc between a function, update, part, or protocol
entry name and `(` is therefore `UH0019` incompatible; it cannot cross the
open to document the first parameter. A doc inside the list immediately before
`)` encounters no construct and is `UH0017` dangling.

An annotation after an opening element annotates the next child, not the
containing element. To annotate the containing element, place it before the
opening tag.

### 7.2 Markup annotation targets

Markup annotations may target:

- a UI element;
- a checked UpperCamelCase UI call occurrence; and
- a complete `{#if}` or keyed `{#each}` block.

Raw text nodes, interpolations, attributes, event bindings, expressions,
arguments, `{:else}` arms, and parser recovery nodes are not annotatable.
External CSS is outside Uhura source metadata. Authors annotate the nearest
owning element or complete structural block.

For a UI call, the annotation describes that call site rather than the called
declaration. Its target span is the full self-closing call element. Declaration
documentation remains the called declaration's durable contract.

All annotation kinds, including `@doc`, use this same target table. The kind
does not change target eligibility.

### 7.3 Cardinality

Each documentable target has zero or one normalized doc. Each annotatable
markup target has an ordered list of zero or more annotations. Annotation kinds
do not impose cardinality; repeated `@doc` or `@review-note` entries remain
distinct.

Consecutive annotations before one target attach in source order. Their order
is the zero-based ordinal among metadata entries on that target. An ordinary
comment between them remains formatter trivia and does not change their target.

## 8. Canonical formatting

The one Uhura formatter owns comment layout and is idempotent.

DSL comments use these canonical prefixes:

```uhura
// Ordinary comment.
/// Declaration documentation.
//! File documentation.
```

The formatter emits docs immediately before their targets at the target's
indentation. It emits each ordinary DSL comment on its own line at the
following item's indentation. Trailing list trivia is emitted at the list's
member indentation, one level inside `)` or `}`; trailing file trivia uses
top-level indentation, and module transition trivia remains immediately before
the first markup node. Any ordinary comment or doc inside a
declaration parameter list forces the canonical multiline parameter layout.

For an ordinary DSL comment, formatting preserves the body bytes after the
first `//`, except for removing trailing horizontal whitespace and normalizing
the line ending. In particular, leading body spacing and `////` slash dividers
remain unchanged.

When ordinary comments occur between lines of one doc run, formatting retains
their relative ordering while keeping the doc lines bound to one target:

```uhura
/// First documentation paragraph.
// Implementation note retained as ordinary trivia.
/// Second documentation paragraph.
reload_pending: Bool = false,
```

Markup layout is selected from normalized text, not original delimiter layout.
Text containing no LF uses one line, so an originally multiline comment whose
normalized text is `text` canonicalizes to `<!-- text -->`. The canonical
empty ordinary comment is `<!-- -->`:

```uhura
<!-- Ordinary comment. -->
<!-- @annotation The primary action. -->
<!-- -->
```

Text containing at least one LF uses an opening or marker line, normalized body
lines, and one closing line. The ordinary and annotated forms are:

```uhura
<!--
Ordinary first line.
Ordinary second line.
-->

<!-- @rationale
The destructive action is separated from the primary controls.
Recovery remains immediate.
-->
<button />
```

Nested formatting prefixes every displayed line with its target or sibling-list
indentation; that indentation is not part of normalized comment or annotation
text. The formatter preserves markup annotation order and retains trailing
ordinary comments at their scope boundary rather than dropping or reattaching
them.

## 9. Checked authoring metadata

The syntax layer retains ordinary comments for formatting and attached docs
and annotations with their spans. Checking produces a logical authoring
metadata projection separate from canonical runtime IR.

Each metadata entry contains at least:

```text
SourceMetadataEntry = {
  class: doc | annotation,
  kind: "doc" | annotation-kind,
  text: normalized UTF-8,
  metadata-span: source span,
  target: {
    file: canonical project-relative source path,
    class: source-target class,
    span: source span
  },
  order: zero-based ordinal on target
}
```

For `//!`/`///`, `class` and `kind` are both `doc`. For a markup annotation,
`class` is `annotation` and `kind` is the exact marker text—even when that kind
is `doc`. Consumers MUST distinguish the class from the kind.

In a checked package graph, the exposed authoring projection is root-package
owned. Locked dependency sources and occurrences remain available through the
whole-graph provenance/source inventory, but dependency docs and annotations
do not become editable metadata of the consuming project. Thus “project-wide”
ordering below ranges over the root project's admitted authoring sources, not
over annotations owned by acquired packages.

`source-target class` uses this closed logical vocabulary:

```text
source-module
machine-declaration | part-declaration | ui-declaration
struct-declaration | enum-declaration | key-declaration
const-declaration | function-declaration
part-parameter | function-parameter | update-parameter | config-field
struct-field | enum-variant | enum-variant-field
event-declaration | command-declaration
outcome-declaration | required-outcome-declaration | protocol-parameter
part-composition | port-declaration
state-field | computed-declaration | observation-field
event-handler | update-declaration | reconciliation-block
example-declaration
ui-element
if-block | each-block
```

The ordinal is target-local; the only repeatable entries today are markup
annotations. A doc entry therefore has ordinal `0`. When a consumer needs a
deterministic project-wide traversal, entries sort by canonical file path
(bytewise), then `metadata-span.start`, then target-local ordinal. The logical
metadata span of one markup annotation is its full `<!-- … -->` span; a doc
run uses the envelope defined in §5.4.

Target spans are half-open byte spans. Except for the source module, they
exclude leading metadata and ordinary trivia, trailing trivia, and a trailing
line ending:

| Target class | Span |
|---|---|
| `source-module` | byte `0` through file length, including preamble metadata |
| machine/part/UI declaration | first byte of `machine`, `part`, or contextual `ui` through its body's closing `}`; excludes leading `pub` |
| struct/enum declaration | first byte of `struct`/`enum` through its closing `}`; excludes leading `pub` |
| key/const/function declaration | first byte of `key`/`const`/`fn` through its terminating `;` or body `}`; excludes leading `pub` |
| part/function/update parameter | first byte of the parameter name through the type's final token; excludes the separating comma |
| configuration field | first byte of the field name through the type's final token; excludes the separating comma |
| struct field or enum-variant field | first byte of the field name through the type's final token; excludes the separating comma |
| enum variant | first byte of the variant name through its payload `}` when present; excludes the separating comma |
| event/command declaration | first byte of the variant name through its closing `)` when present; excludes the separating comma |
| outcome/required-outcome declaration | first byte of `commit`/`abort` through the variant's closing `)` when present; excludes the separating comma |
| protocol payload parameter | first byte of the parameter name through the type's final token; excludes the separating comma |
| part composition or port declaration | first byte of `part`/`port` through its terminating `;` |
| `state-field` | first byte of the field name through the initializer's final token; excludes the separating comma |
| computed declaration | first byte of `computed` through its terminating `;`; excludes leading `pub` |
| observation field | first byte of the field name through its expression's final token when written; excludes the separating comma |
| event handler | first byte of `on` through the handler body's closing `}` |
| update declaration | first byte of `update` through the body's closing `}`; excludes leading `pub` |
| reconciliation block | first byte of `before` through the body's closing `}` |
| `example-declaration` | first byte of `example` through its terminating `;` |
| UI element | opening `<` through the self-closing `>`, or through the matching closing tag's `>` including children |
| `if-block`/`each-block` | opening block `{` through the matching block-close `}` including every arm |

The exact Rust representation, serialization protocol, artifact filename, and
long-term stable target identifier are implementation RFC concerns. Those
choices must preserve this logical information and the semantic-inertness
invariant.

The target span is sufficient for one checked source revision and navigation
back to source. It is not promised as a durable identity for external review
threads or arbitrary edits. A later provenance contract may add stable source
origins without changing text or attachment semantics.

Canonical `ProgramIr` and semantic `V` serialization must not acquire docs or
annotations merely to satisfy this RFC. Consumers that need authoring metadata
request the separate projection.

## 10. Diagnostics and recovery

This RFC reserves the next source-diagnostic entries:

| Code | Rule | Condition |
|---|---|---|
| `UH0016` | `syntax/malformed-markup-comment` | Unterminated/invalid XML-shaped comment, malformed leading marker, or empty annotation payload |
| `UH0017` | `syntax/dangling-metadata` | Non-empty doc or annotation reaches a scope/arm/end boundary without a target |
| `UH0018` | `syntax/misplaced-inner-doc` | Non-empty `//!` appears after the file preamble |
| `UH0019` | `syntax/incompatible-metadata-target` | Non-empty doc or annotation precedes or occurs inside a source construct that cannot accept its class, including duplicate docs on one target |

Diagnostics identify both the metadata span and, when present, the incompatible
target span. An ordinary DSL comment at a non-boundary position, or a
well-formed XML-shaped comment outside a markup sibling position, uses the
existing `UH0001 syntax/unexpected-token`. A malformed markup comment or
annotation never silently degrades to an ordinary comment or text node.

Diagnostic precedence is lexical malformation (`UH0016`), non-empty `//!`
after the preamble (`UH0018`), incompatible construct (`UH0019`), then a
closing/end boundary reached with no construct (`UH0017`). Entirely empty doc
runs take none of these metadata diagnostics. Thus a doc between an event name
and `(` is incompatible, while a doc after `(` and immediately before `)` is
dangling.

Parsers retain recovery nodes and continue after malformed markup comments
when a terminator or next safe source boundary can be found. Invalid metadata
does not enter the authoring projection.

## 11. Runtime and presentation boundary

Docs and annotations are compiler-owned authoring metadata. Uhura Core, its
semantic view protocol, widget catalogs, and host-driver contracts do not
interpret them.

An Editor or another authoring consumer may present the metadata and navigate
to its source target. Mapping one source target to zero, one, or many rendered
instances requires a separate provenance and presentation design. This RFC
does not select DOM attributes, placement algorithms, Canvas interaction, or
export policy.

## 12. Compatibility

Markup comments were additive to the 0.3 spike, which did not accept `<!--`.
The accepted comment taxonomy is now part of the versioned 0.4 grammar.
Programs using removed 0.3 page/component/store forms do not become 0.4 merely
by preserving their docs; migration reattaches the normalized text to the
corresponding 0.4 target listed in §1.1 and §5.4. `////` remains the escape for
slash dividers and ordinary prose, and `// @kind` remains ordinary in DSL
mode.

Valid metadata is runtime-inert. Tools that only understand canonical runtime
IR remain compatible because the authoring projection is separate. Source
locations after an inserted comment may move, but runtime diagnostic meaning
does not. Tools that rewrite source must preserve all three tiers and
target-local source order.

## 13. Alternatives considered

### `<!-- /// … -->`

Rejected. It carries a declaration-doc sigil into a region containing source
occurrences and encourages authors to treat every element as a declaration.
Tagged annotations state their local purpose and leave `///` with one meaning.

### Giving `@doc` declaration-doc semantics in markup

Rejected. Markup kinds are open author/tool vocabulary. `@doc` is valid, but
it remains an occurrence annotation and does not gain singleton declaration
semantics. The metadata class carries the nature distinction.

### Adding `// @kind …` outside markup

Rejected for this RFC. The non-markup taxonomy intentionally stays identical
to Spock: ordinary comments, outer docs, and file docs. A future need for
statement annotations must justify a separate source form and target table.

### One fixed `@annotation` kind

Rejected. Review notes, rationales, migration notes, and tool-specific metadata
benefit from distinct opaque kinds. The open lower-kebab kind space provides
that without adding a directive language or runtime behavior.

### Promoting every ordinary comment

Rejected. It would unexpectedly publish implementation trivia, separators,
disabled fragments, and temporary notes. Promotion must be explicit.

### `<annotation>` elements or annotation attributes

Rejected. Elements pollute the closed widget/component tree and child
validation. Attributes are poor multiline prose containers and risk mixing
authoring metadata with runtime props.

### Storing metadata in `ProgramIr` or semantic `V`

Rejected. Documentation changes must not alter executable artifacts, program
hashes, checkpoints, observations, receipts, runtime traces, or renderer
protocol traffic. A separate checked projection makes the authoring boundary
explicit.

## 14. Consequences

Benefits:

- declaration documentation remains consistent with Spock;
- local element notes remain occurrence annotations even when named `@doc`;
- humans and agents can choose precise, preserved annotation kinds;
- ordinary comments remain formatter-only trivia;
- runtime artifacts and behavior remain unchanged; and
- future Editor work receives explicit source targets rather than reverse
  engineering rendered output.

Costs:

- the DSL lexer/parser needs doc tokens, pending-doc attachment, explicit
  grouping-head and transition trivia, parameter metadata, example-clause
  trivia, and trailing list trivia;
- the markup parser needs XML-comment recognition before generic elements plus
  a pending-annotation queue owned by each sibling list;
- the AST needs ordinary trivia and attached metadata on the closed targets,
  including declarations, members, parameters, and examples;
- parameter docs and ordinary parameter-list comments require multiline
  signature formatting;
- the checker needs target compatibility, normalized spans/classes/order, and
  a separate authoring-metadata projection;
- canonical formatting needs trailing DSL trivia plus ordinary/tagged markup
  comments; and
- presentation still requires a separate source-to-render provenance design.

## 15. Deferred decisions

The following remain open:

- port-contract and catalog documentation;
- non-markup occurrence annotations;
- annotations on attributes, expressions, match arms, text runs, example
  clauses, and CSS syntax;
- stable identities and re-anchoring for externally stored threads;
- Markdown, links, structured payloads, and annotation-kind registries;
- internal/public visibility and self-contained export policy;
- collaborative editing and lifecycle metadata; and
- Canvas presentation and rendered-instance multiplicity.

Adding one of these capabilities requires a focused RFC; it does not follow
from the open markup-kind namespace.

## 16. Implementation and conformance requirements

RFC 0003 is an accepted source-language decision. An implementation conforms
only when tests demonstrate all of the following:

1. `//`, `////`, `///`, and `//!` classify exactly as specified; `// @kind`
   remains ordinary in DSL mode.
2. Ordinary DSL comments survive exactly at the closed comment-bearing
   boundaries—including grouping heads and the DSL-to-markup/style transition
   but excluding import braces and expression interiors—force multiline
   parameter layout when applicable, and receive `UH0001` elsewhere.
3. File-preamble, dangling, incompatible-target, and malformed-comment
   diagnostics use the reserved codes, spans, and precedence; entirely empty
   doc runs receive none of those diagnostics and canonicalize away without
   dropping interleaved legal ordinary comments.
4. DSL doc runs, their envelope spans, and every closed target span normalize
   deterministically.
5. Declaration and parameter docs attach only to the closed target table and
   do not cross a parameter-list open or close.
6. Ordinary and tagged XML comments parse only at markup sibling positions and
   recover, normalize, and format deterministically, including XML's `--` and
   trailing-`-` restrictions; well-formed comments elsewhere receive `UH0001`.
7. All valid lower-kebab annotation kinds—including `doc`—survive exactly in
   target-local source order and remain annotation-class metadata.
8. Markup annotations attach only to native elements, checked UI call
   occurrences, and complete `if`/`each` structural blocks without crossing
   parent or arm boundaries. A call annotation identifies the call-site span,
   not the called declaration.
9. Formatter output is idempotent, selects markup shape from normalized text,
   and retains ordinary comments at item/list, trailing scope, and source-mode
   transition boundaries without consuming structural node bounds.
10. Checked metadata retains class, kind, normalized text, metadata span,
    target span, target class, file, and target-local order.
11. Adding or editing valid docs and annotations leaves canonical runtime IR,
    program hashes, checkpoints, observations, receipts, commands, traces, and
    runtime diagnostic meaning unchanged; only source locations may shift.
12. Evidence-module file docs, example docs, and existing `note` clauses
    remain distinct through checking and formatting. Explicit projects select
    evidence modules through `[evidence.modules]`; `web-app@1` additionally
    discovers its closed subject-local `*.examples.uhura` set.
