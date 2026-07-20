# Uhura 0.4 source and lowering

- **Status:** Active candidate concrete source
- **Semantic authority:** [Source-neutral kernel](kernel.md)
- **Normative grammar:** [Core grammar appendix](grammar.ebnf)
- **Application syntax:** [Application profile](application.md)
- **Validation:** [Conformance](conformance.md)

This document is the sole **core** concrete-syntax authority for the 0.4
candidate. Activated profiles own only their additional syntax; the `ui`
profile is specified in [Application profile](application.md). The
[grammar appendix](grammar.ebnf) fixes core tokenization, precedence, and
phrase structure. This document fixes the meaning and static checks of those
forms. A profile may add contextual forms after an explicit direct `use`; it
cannot reinterpret a core token or production.

## 1. Parent patterns

Core source follows a deliberately bounded Rust shape:

- `use`, `pub`, and `pub use` for inert resolution and visibility;
- immutable `let` bindings, `const` values, and `fn` declarations;
- `fn name(value: Type) -> Type`, lexical `return`, and block-tail values;
- `struct`, `enum`, `match`, `Type::Variant`, and `..base` updates;
- `&&`, `||`, and `!`;
- semicolon-terminated statements and comma-separated fields or variants;
- `|value| expression` only as a non-escaping collection binder; and
- snake_case values, SCREAMING_SNAKE_CASE constants, and UpperCamelCase types
  and data variants.

The `ui` profile follows Svelte-shaped markup. Uhura does not borrow Rust's
ownership or execution model, Svelte component lifecycle, JavaScript
execution, or DOM authority.

Novel forms are limited to the semantic model:

```text
machine  part  config  require  requires  events  commands  outcomes
state  computed  observe  on  update  port  emit  invariant  key
commit  abort  before commit  is  decreases  unreachable
```

Each term denotes a distinct checked or runtime fact. `is` is Uhura's compact
Boolean pattern test; it replaces neither exhaustive `match` nor a hidden
macro. Removing an owned term must either use a familiar form with the same
meaning or demonstrate that the concept is unnecessary.

### Lexical and naming contract

Core source is valid UTF-8 without a byte-order mark or U+0000. Outside a
`Text` literal or comment, whitespace is ASCII space, tab, LF, CRLF, or CR.
There is no block comment and no automatic semicolon insertion. The grammar
uses longest-token recognition for `::`, `->`, `=>`, `&&`, `||`, `==`, `!=`,
`<=`, `>=`, and `..`.

Symbolic names are ASCII in 0.4. Exact Unicode remains fully available in
`Text`, where it belongs without creating normalization-dependent symbol
identity. Value, field, module, part-instance, and parameter names use
snake_case; constants use SCREAMING_SNAKE_CASE; types, machines, parts, and
constructors use UpperCamelCase. `_` is only a pattern wildcard. The checker
rejects a name in the wrong category rather than silently recasing it.
Canonical identifier order is therefore bytewise ASCII order.

Unsigned integer source is `0` or a nonzero digit followed by digits. Decimal
source has digits on both sides of one dot. A leading `-` is an operator.
Leading zeroes, digit separators, base prefixes, exponents, `.5`, and `1.` are
not admitted. Strings use JSON escapes exactly, including four-hex-digit
`\uXXXX`; surrogate escapes must form a valid pair, and unescaped control
characters are rejected. Source decoding produces Unicode scalar values and
does not normalize them.

Core keywords are hard-reserved in declaration, path, and binding positions.
After `.`, a keyword token is admitted contextually as a member label; an
explicit `keyword: value` or `keyword: pattern` is likewise admitted in a
record construction or pattern. This keeps closed data contracts such as
`Entry.key` usable without admitting keyword-named locals or declarations.
Keyword labels never have shorthand spelling. Prelude names such as `Bool`,
`Option`, `None`, and `Some` are ordinary resolved names. Profile words such
as `ui` are contextual: they remain ordinary names unless the same logical
module contains the profile's exact direct activating `use`.

### Operator and block contract

From highest to lowest, precedence is postfix projection/call/indexing, unary
`!` and `-`, multiplication, addition/subtraction, one comparison, `&&`, `||`,
and `return`. A comparison is exactly one of `==`, `!=`, `<`, `<=`, `>`, `>=`,
or `is pattern`; comparison chains are rejected. Operators are fixed prelude
operations and cannot be overloaded.

`&&` and `||` short-circuit left to right. `return` is a diverging expression
with type `Never`, which is why it can be the complete right side of a
`match` arm. In a statement sequence its canonical spelling ends in `;`.
`unreachable;` is a distinct terminal statement, not another spelling of
`return`.

A parsed block retains an ordered statement list, every authored semicolon,
and at most one final unterminated expression. A bare `{ ... }` is itself a
block expression; its final expression is its value. An `if` or `match`
expression may omit its semicolon before a following statement, following the
familiar Rust block-shaped statement rule, but it must then check as `Unit` or
`Never`. Every other non-final expression statement requires `;` and must also
check as `Unit` or `Never`. A semicolon may never silently discard a value.

## 2. Complete L0 source

```uhura
pub machine BoundedCounter {
  config {
    minimum: Int,
    maximum: Int,
    initial: Int,
  }

  require minimum <= initial && initial <= maximum;

  events {
    Increment,
    Decrement,
    Reset,
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = initial,
  }

  invariant minimum <= count && count <= maximum;

  observe {
    count,
    at_minimum: count == minimum,
    at_maximum: count == maximum,
  }

  on Increment {
    count = min(count + 1, maximum);
    Accepted
  }

  on Decrement {
    count = max(count - 1, minimum);
    Accepted
  }

  on Reset {
    count = initial;
    Accepted
  }
}
```

This is not pseudocode. Its intended lowering preserves the 0.3 configuration,
state, observation, and reaction calculus. Protocol spelling and program
identity may differ; differential comparison uses the explicit constructor
map in the fixture. The frozen observation labels remain snake_case.

### 0.3 to 0.4 surface map

| Uhura 0.3 | Uhura 0.4 | Semantic result |
| --- | --- | --- |
| `language uhura 0.3` | Manifest `language = "0.4"` | Project language identity |
| `module app.x@1` | Manifest/package identity plus source module | Resolved module identity |
| Implicitly public top level | Explicit `pub` | Visibility |
| `key TaskId over Text` | `key TaskId(Text);` | Nominal key |
| `fn f(x: T) -> U = value` | `fn f(x: T) -> U { value }` | Named pure function |
| `input = \| increment` | `events { Increment }` | Local input sum |
| `command = Never` | Omit `commands` | Empty local command sum |
| `command = \| start(T)` | `commands { Start(value: T) }` | Local command sum |
| `accepted commit` | `commit Accepted` | Outcome constructor and policy |
| `derive x: T = value` | `computed x: T = value` | Pure derived binding |
| `const limit: Nat = 2` | `const LIMIT: Nat = 2;` | Closed immutable constant |
| `observe { x = x }` | `observe { x }` | Observation field |
| Local `let` | `let value = expression;` | Immutable local binding |
| `set count = value` | `count = value;` | Owned draft-state update |
| `finish accepted` | Tail `Accepted` | Terminal outcome selection |
| `on x = transition(...)` | `on x { transition(...) }` | Handler delegation |
| `record with { field: value }` | `Record { field: value, ..record }` | Closed record update |
| `and`, `or`, `not` | `&&`, `\|\|`, `!` | Boolean operations |
| `if c then a else b` | `if c { a } else { b }` | Conditional value |
| `port x: Contract(config)` | `port x = Contract { config };` | Port requirement |

The first two rows change source and package metadata, not the reaction
calculus. Every other row must have a direct semantic-equivalence test.
UpperCamelCase protocol variants intentionally change source symbols; a
differential fixture maps them to the corresponding 0.3 constructors rather
than claiming equal raw IR or program identity. Composed parts are a new
source-composition feature, not a spelling migration;
flat-versus-part behavior is tested under an explicit path mapping rather than
claiming identical program identity.

## 3. Project and module envelope

Language and package versions belong once in `uhura.toml`, not at the top of
every source file:

```toml
[project]
name = "examples.programs"
version = 1
language = "0.4"

[modules]
programs = "programs.uhura"
```

The complete closed manifest, lock, and identity rules are specified in
[Project and identity](project.md). The lock resolves every non-local `use`
path to an exact compatible contract.
Source files begin with ordinary use declarations:

```uhura
use uhura::observation::Observation;
use crate::notice::Notice;

pub machine Application {
  // ...
}
```

`crate` names the current project package; an external first segment names a
locked package. A path resolves one public declaration. Braced named imports,
local aliases, and explicit same-name re-export are admitted:

```uhura
use uhura::web_router::{Link, Router};
use crate::notice::Notice as AppNotice;
pub use crate::identity::UserId;
```

`use ... as LocalName` changes only the local binding. `pub use` does not admit
`as`: it adds another public locator for the same declaration and preserves
that declaration's `resolved-package-identity :: public-name`. It never
creates a second semantic declaration. Conflicting public names or locators
are rejected.

Glob, dynamic, conditional, default, and side-effect-only imports are
excluded. A profile may assign additional checker meaning to a resolved
declaration, but path resolution itself still follows these rules.

Before source checking, project resolution supplies the closed one-to-one
`[modules]` map from logical module paths to physical source files:

- `crate::notice::Notice` names public declaration `Notice` in logical module
  `notice` of the current package;
- `vendor::icons::Icon` names public declaration `Icon` in logical module
  `icons` of locked package `vendor`;
- every checked source file has exactly one logical module path;
- a physical filename or directory does not infer a logical path; and
- moving a file while updating only the map preserves both source and IR.

Changing a logical module path requires updating affected `use` declarations,
but still preserves IR when package identity, public names, and logical
composition are unchanged. A framework may generate the map from a directory
convention; the resulting resolved map, not that convention, is the checker
input.

Use declarations are declarative and inert. Resolution:

- performs no initialization or I/O;
- grants no host authority;
- exposes only names explicitly named by `use`;
- permits explicit type/declaration cycles when two-phase resolution can
  close them; and
- rejects initialization, derived-value, and update cycles rather than
  rejecting every cyclic file graph.

Top-level declarations are private unless marked `pub`. Re-export uses
`pub use`. Top-level declaration order and the placement of named sections
inside a machine or part are not semantic; the checker resolves the complete
module graph and reports actual dependency cycles. Order inside a form follows
that form's contract: statement sequences, emitted commands, sequence
literals, struct declarations, configuration and state fields, closed-domain
constructors, and invariant entries are semantic. Named fields in a struct
construction, pattern, or `..base` update are canonicalized to declaration
order, so their authored order is not semantic. Sets, maps, exhaustive table
entries, and orderless finite views expose no authored traversal order.

Composition uses one canonical owner order everywhere: the root owner first,
then its directly composed stable part names lexicographically by canonical
identifier bytes. Within one owner, configuration, state, and observation
fields retain declaration order; local event and command constructors retain
declaration order and precede that owner's port contributions; ports are
sorted by canonical port name; and each port's receive and send constructors
retain contract declaration order. This order determines the aggregate `S`,
`I`, `K`, and `V` domains, constructor ordinals, checkpoints, canonical
encodings, receipts, semantic IR, program hashes, and invariant groups.
Physical file layout and the authored placement of `part` or `port`
declarations never participate in it.

A package's public top-level names are unique across its source modules.
Their semantic identity is:

```text
resolved-package-identity :: public-name
```

Private declarations that require semantic identity are scoped beneath exactly
one public machine, part, UI, type, or value that owns their lowered use. If
a private nominal declaration is reachable from more than one public owner,
the checker requires it to become an explicit package declaration. Private
structural helpers may be referenced by more than one public owner; they are
independently canonicalized into each owner's IR and do not acquire a shared
or path-based identity.

Renaming a public declaration or a stable part composition name changes semantic
identity. A physical path is only a source locator and provenance.

A logical module path locates source but is not a semantic runtime identity.
Moving a physical file and updating the resolver map, or renaming a logical
module and updating its `use` sites, must preserve checked IR when package
identity, public names, and composition names are unchanged.

The source has no automatic semicolon insertion. Semicolons terminate
non-block statements; a final expression without a semicolon is a block's
value. An unterminated block-shaped `if` or `match` before another statement
uses the closed exception defined in §1 and must have type `Unit` or `Never`.
Commas delimit struct fields, enum variants, tuple, argument, parameter,
sequence, match-arm, and machine-domain entries. A semicolon may not silently
discard a non-`Unit` value or outcome. The formatter emits the Rust-shaped
punctuation canonically; newlines alone carry no grammar.

### Declaration scopes and visibility

The declaration matrix is closed:

| Form | Module | Machine | Part | Lexical block | `pub` rule |
| --- | --- | --- | --- | --- | --- |
| `use`, `pub use` | Before declarations | No | No | No | Only singular, unaliased module re-export |
| `machine` | Yes | No | No | No | Optional |
| `part` declaration | Yes | No | No | No | Optional |
| `struct`, `enum`, `key` | Yes | No | No | No | Optional; fields never carry `pub` |
| `const` | Yes | Yes | Yes | No | Optional only at module scope |
| `fn` | Yes | Yes | Yes | No | Optional only at module scope |
| `config` | No | At most one | No; ordinary parameters replace it | No | Never |
| `require` | No | Repeated | Repeated | No | Never |
| `part name = Part(...)` | No | Repeated | No | No | Never |
| `events`, `commands`, `state`, `observe` | No | At most one each | At most one each | No | Never |
| `outcomes` | No | At most one | No | No | Never |
| `requires outcomes` | No | No | At most one | No | Never |
| `port` | No | Repeated | Repeated | No | Never |
| `computed` | No | Repeated | Repeated | No | Only `pub computed` in a part |
| `invariant` | No | Repeated | Repeated | No | Never |
| `on` | No | Repeated | Repeated | No | Never |
| `update` | No | Repeated | Repeated | No | Only `pub update` in a part |
| `before commit` | No | At most one | No | No | Never |
| `let` and pattern binders | No | No | No | Yes | Never |

An empty optional grouping is rejected; omission is its canonical empty form.
Nominal declarations remain module-level so one type has one package-resolved
identity. Machine- and part-local `fn` declarations are private total helpers,
not methods, and cannot capture configuration or state. Their inputs remain
explicit.

### Namespaces, lookup, and shadowing

Uhura deliberately does not inherit Rust's separate type and value namespace
collision rules:

- one module declaration namespace contains imports, machines, parts, structs,
  enums, keys, constants, and functions;
- one owner value namespace contains its configuration or ordinary part
  parameters, constants, functions, part-instance names, port names, state,
  computed values, and updates;
- event, command, and outcome constructors inhabit three distinct closed
  protocol namespaces; every port direction has its own qualified constructor
  namespace;
- each nominal enum owns its `Type::Variant` namespace, and each struct,
  variant payload, configuration, state, observation, or protocol payload owns
  a field-label namespace; and
- generated `Part::Observation`, `Part::Reads`, and `Part::Updates` names are
  reserved associated names of that part.

Two declarations cannot share a spelling in the same namespace. An import
cannot collide with another import or local declaration, even when both
resolve to the same semantic declaration; an explicit `pub use` is the sole
re-export mechanism. Field labels do not introduce lexical values except when
a shorthand construction or pattern resolves an already-visible value or
creates an explicit pattern binder.

All declarations and owner members are resolved independent of textual order.
Lexical bindings begin after their initializer and end at the containing
block or pattern arm. A new parameter, `let`, or pattern binder may not shadow
any still-visible lexical or owner value. Duplicate binders in one pattern are
rejected. Separate `match` arms may reuse a spelling because their scopes are
disjoint. Every alternative of `p1 | p2` must bind exactly the same names with
the same types; those names enter the arm only after the complete alternative
matches.

`Type::Name` resolves nominal association. `value.field` resolves closed field
projection, a fixed prelude operation, a directly composed part capability,
or a port constructor in its admitted context. No lookup falls back from one
namespace to another, and import order, declaration order, or casing
heuristics never disambiguate a name.

The readable environment of each declaration is also closed:

| Context | Values it may read |
| --- | --- |
| Module/owner `const` initializer | Resolved constants and total pure functions only |
| `fn` body | Parameters, resolved constants, and total pure functions only |
| Root `require` | Root configuration, constants, and pure functions |
| Part `require` | Ordinary part parameters, constants, and pure functions; not dependency handles |
| Root state initializer | Root configuration, constants, and pure functions; no state or computed value |
| Part state initializer | Ordinary part parameters, constants, and pure functions; no state, computed value, or dependency handle |
| Root port binding | Root configuration and constants |
| Part port binding | Ordinary part parameters and constants |
| Part composition binding | Root configuration and constants, or one exact direct-sibling `reads`/`updates` handle |
| `computed` or invariant | Configuration/ordinary parameters, constants, pure functions, owned current draft state, acyclic computed values, and declared `Reads` |
| `observe` | Configuration/ordinary parameters, constants, pure functions, committed owned state, acyclic computed values, and declared `Reads` |
| Handler or `update` | Configuration/ordinary parameters, constants, pure functions, owned current draft state, computed values, declared `Reads`, and admitted updates |
| Root `before commit` | Root current draft plus the root's directly composed read/update capabilities |

An expression may use only the row for its enclosing context plus its lexical
parameters and `let` or successful flow binders. More permissive name
resolution followed by an effect check is conforming only when it produces the
same accept/reject result and diagnostics.

## 4. Values, types, and pure code

The exact value model is defined by the kernel. Familiar source forms do not
change its semantics.

### Literals

Core literals use the smallest familiar forms:

```uhura
true
42
-7
0.25
"exact Unicode text"
[first, second]
(left, right)
Request { id, label }
None
Some(value)
()
```

An unconstrained integer literal has type `Int`; an expected `Nat` or
`PositiveInt` admits it only when the value fits. An unconstrained decimal
literal has type `Decimal`; an expected `Ratio` admits it only in `[0, 1]`.
`0.0` and `1.0` are decimal source spellings even though canonical value
encoding removes redundant fractional zeros; the formatter retains `.0` when
needed to preserve literal type.
Strings use JSON escape spelling and receive no implicit normalization.
`[...]` is a sequence, `(a, b)` is a tuple, and parentheses around one
expression only group it. `Map::empty()` and `Set::empty()` are context-typed
empty collections. `Map::from([(key, value), ...])` constructs a map from a
literal sequence whose keys are compile-time values and unique by typed
equality; source entry order is not observable. `BoundaryNumber` NaN and
infinities have no authored literals and arrive only through typed boundary
decoding.

`()` is the single `Unit` literal. It is also produced by canonical fallthrough
from a result-less update and by other declarations whose contract has no
payload.

### Bidirectional typing and refinements

Checking is bidirectional. An expression either **synthesizes** one type from
its form and resolved declarations or is **checked** against an expected type.
The implementation must not guess a nominal type, pick an import by search, or
insert a conversion to make an expression fit.

Type annotations are required on module/owner constants; configuration,
state, struct, enum-payload, event, command, and outcome fields; part,
function, handler-payload, and update parameters; key payloads; and function
results. A `pub computed` must declare its result type because it enters
`Part::Reads`. A private `computed`, an observation field, and a `let` may
infer their type. An update without `-> Type` has result type `Unit`; a
non-`Unit` update declares its result. An unannotated `let` must synthesize
uniquely from its initializer; `let value: Type = expression;` instead checks
the initializer against `Type`.

The only refinement-subsumption relationships are:

```text
PositiveInt <: Nat <: Int
Ratio       <: Decimal
NonEmpty<T> <: Seq<T>
Never       <: T            for every expected T
```

Using a member of a subset at a wider type is not a value conversion and does
not alter canonical data. The reverse direction requires a proof from a
literal, active checked facts, or an explicit total checked constructor. No
other pair of nominal or structural types is substitutable. In particular,
`Int` does not become `Nat`, `Decimal` does not become `Ratio`, `Text` does not
become a key, and equal struct shapes do not become the same nominal source
type.

Literal and constructor inference is exact:

- an unconstrained integer synthesizes `Int`; an expected `Nat` or
  `PositiveInt` accepts it only when its mathematical value lies in that set;
- an unconstrained decimal synthesizes `Decimal`; an expected `Ratio` accepts
  it only when it lies in `[0, 1]`;
- `true` and `false` synthesize `Bool`, text synthesizes `Text`, and `()`
  synthesizes `Unit`;
- a non-empty sequence literal joins its element types and synthesizes
  `Seq<T>`; `[]` requires an expected `Seq<T>`;
- `Map::empty()`, `Map::from`, `Set::empty()`, and bare `None` require an
  expected result type; `Some(value)` can synthesize `Option<T>` when `value`
  synthesizes `T`; and
- a struct, enum, key, event, command, or outcome constructor supplies the
  expected type of every payload from its resolved declaration. Named struct
  fields are checked in declaration order after duplicate/missing-field
  validation.

For `+`, `-`, `*`, `min`, and `max`, refinement operands first use their
declared base family. Integer-family arithmetic synthesizes `Int`;
unrefined decimal-family arithmetic synthesizes `Decimal`. `Ratio * Ratio`
instead remains `Ratio`, because `[0,1]` is closed under multiplication.
`Ratio + Ratio` and `Ratio - Ratio` remain `Ratio` only when the proof fragment
establishes the upper or lower bound respectively; otherwise the source is
rejected instead of changing its numeric family implicitly. Thus
`started + 1` may check as `PositiveInt` from `started: Nat`, while `left -
right` cannot check as `Nat` without a fact proving `left >= right`.
Multiplication never participates in the 0.4 loop-decrease solver except by an
integer literal constant.

`==` and `!=` require both operands to check at one identical complete-value
type after upward refinement subsumption. Ordered comparison is admitted only
within one numeric base family. Boolean operators require `Bool`; `is` checks
its pattern against the left operand and synthesizes `Bool`.

When no expected type is available, branch and collection joins use only this
closed least-upper-bound table:

- identical types join to themselves;
- `Never` contributes no constraint, and all-`Never` branches synthesize
  `Never`;
- integer refinements join at their nearest member of
  `PositiveInt <: Nat <: Int`;
- `Ratio` and `Decimal` join as `Decimal`;
- `NonEmpty<T>` and `Seq<T>` join as `Seq<T>`;
- `None` and `Some(T)` join as `Option<T>`; and
- no other union, structural, or nominal join is inferred.

An expected type is pushed into every `if` branch, `match` arm, sequence
element, tuple element, constructor payload, call argument, and annotated
initializer before falling back to synthesis and the join table. An `if`
without `else` is admitted only as `Unit` or `Never`; its absent branch is
`Unit`. A `match` must be exhaustive after pattern checking.

`return value` checks `value` against the enclosing function, update, or
handler result and synthesizes `Never`; value-less `return` is admitted only
for an expected `Unit` result. `unreachable;` also terminates with `Never` in
its admitted reaction contexts. Consequently a block terminated on every path
by return or `unreachable` may check against any expected result, but it never
manufactures a value.

Flow facts are lexical and path-sensitive. A successful `is` pattern refines
its matched value and introduces its binders on the proven-success path.
Numeric comparisons contribute exact equality or inequality facts. A state
assignment invalidates facts mentioning that state field or a projection from
it; direct substitution may then establish new facts. Facts from one
short-circuit branch do not leak into another, and facts do not survive an
effectful update that may write a referenced field.

### Structs

```uhura
pub struct Request {
  id: RequestId,
  label: Text,
}

let request = Request {
  id,
  label,
};
```

`struct` is a nominal boundary during source checking: two distinct
declarations are not implicitly substitutable even when their fields match.
Its values lower to the kernel's existing closed structural record IR rather
than a new runtime value kind. A public struct still retains
`resolved-package-identity :: public-name` in the exported schema and program
identity; a private struct name erases after checking except for source
provenance. Making the declaration `pub` exposes its complete field contract.
Unlike Rust, fields do not require their own `pub`; 0.4 has no per-field
visibility, methods, inheritance, or `impl`. Struct update is a closed
persistent copy:

```uhura
let cancelled = Task {
  phase: Phase::Cancelled,
  ..task
};
```

Exactly one same-typed base appears last. Explicit fields replace known
fields. The base remains usable: Uhura has value semantics, not Rust move or
borrow semantics. Unknown, duplicated, missing, prototype, getter,
enumeration, and dynamic spread behavior do not exist.

### Closed sums

```uhura
pub enum Phase {
  Queued,
  Running {
    attempt: PositiveInt,
    progress: Ratio,
  },
  Succeeded,
  Failed,
  Cancelled,
}
```

An enum is a closed nominal sum. It is not a Rust memory-layout promise, and
it admits no discriminant arithmetic or non-exhaustive extension. Constructor
payloads are checked and canonical. Matches are exhaustive:

```uhura
match task.phase {
  Phase::Queued => "queued",
  Phase::Running { .. } => "running",
  Phase::Succeeded => "succeeded",
  Phase::Failed => "failed",
  Phase::Cancelled => "cancelled",
}
```

Constructor resolution is contextual and deterministic:

- a pattern constructor is resolved from the scrutinee's closed sum;
- an event after `on`, an outcome returned by a handler, and a command after
  `emit` are resolved from those distinct closed domains;
- a data-enum constructor is written `TypeName::Variant`, including when an
  expected type would make an unqualified name unambiguous;
- the prelude option variants are the reserved unqualified names `None` and
  `Some`;
- machine event, outcome, and command variants use UpperCamelCase and are
  resolved from their explicit `on`, tail/return, and `emit` contexts without
  a generated type qualifier; and
- a nominal key constructor always uses its type name.

Keeping protocol variants unqualified is an intentional machine-language
rule, not Rust enum inference. Use-declaration order never participates in
either resolution rule.

### Nominal keys

```uhura
pub key TaskId(Text);
```

`TaskId(value)` constructs the key and `.value` projects its underlying value.
No implicit string or integer conversion exists.

### Constants

Module, machine, and part scopes admit closed immutable constants:

```uhura
const LIMIT: Nat = 2;
pub const DEFAULT_ATTEMPT: PositiveInt = 1;
```

A constant initializer is a pure finite expression over literals, constructors,
other resolved constants, and admitted total pure functions whose arguments
are constant. It cannot read configuration, state, computed values, ports, or
host facts. Constant dependencies must be acyclic; textual order is
irrelevant. `pub const` is admitted only at module scope. A machine- or
part-scoped constant is private and does not enter `Observation`, `Reads`, or
`Updates`; `pub` there is rejected. Constants lower to canonical typed values
and introduce no storage, initialization step, or Rust compile-time execution
environment. `static` does not exist.

### Pure functions

Named pure code uses the familiar Rust function shape:

```uhura
fn opposite(side: Side) -> Side {
  match side {
    Side::Left => Side::Right,
    Side::Right => Side::Left,
  }
}
```

Larger bodies use immutable `let`; a block's final expression is its value:

```uhura
fn blank_draft(order: Order) -> ReturnDraft {
  let selections = selections_for(order);
  ReturnDraft {
    order: order.id,
    selections,
  }
}
```

These are declarations, not serializable function values or ambient closures.
A pure function cannot capture mutable draft state, mutate, emit, acquire
authority, recurse, or escape as data.

`let` is the only local binding. It is always immutable: `mut`, reassignment,
references, `self`, and user-defined receiver parameters are excluded.
`return expression;` remains a lexical early return; the formatter prefers a
tail expression for the ordinary result.

Dot syntax is statically resolved only as closed-value field projection, an
admitted total-prelude operation, part/composition qualification, or port
qualification inside `on` and `emit`. Part/composition qualification includes
`part.Event`, `part.reads`, `part.updates.member`, and members of an explicitly
declared dependency handle. None performs trait lookup, virtual dispatch,
extension-method search, or arbitrary object calls.

### Finite collections and expression binders

Collection construction deliberately reuses calls, literals, and pipe binders
instead of adding comprehension grammar. A total table has one special
exhaustive literal constructor:

```uhura
const INITIAL: Table<Entity, Side> = Table::from([
  (Entity::Farmer, Side::Left),
  (Entity::Wolf, Side::Left),
  (Entity::Goat, Side::Left),
  (Entity::Cabbage, Side::Left),
]);
```

`Table::from` admits only a sequence literal with exactly one entry for every
unit variant of the closed key enum. Unknown, missing, or duplicate keys are
rejected. Entry order is non-semantic; lookup is total and `values()` follows
the key enum's declaration order. This is a checked construction form, not a
fallible runtime call.

The L1 ordered violation list is written:

```uhura
fn violations(at: Table<Entity, Side>) -> Seq<Violation> {
  Seq::from_options([
    if at[Entity::Wolf] == at[Entity::Goat]
      && at[Entity::Farmer] != at[Entity::Wolf]
    {
      Some(Violation::WolfWithGoat)
    } else {
      None
    },
    if at[Entity::Goat] == at[Entity::Cabbage]
      && at[Entity::Farmer] != at[Entity::Goat]
    {
      Some(Violation::GoatWithCabbage)
    } else {
      None
    },
  ])
}
```

`Seq::from_options(source)` accepts any checked `Seq<Option<T>>` expression,
visits its values from left to right, removes `None`, and preserves the
relative order of every `Some(T)` payload in the resulting `Seq<T>`. The
literal above is only the compact L1 construction, not a restriction on the
helper. Operations needed by L0–L2 have these fixed order properties:

- sequence literals, `append`, `without`, `uncons`, `map`, `filter`, `all`,
  `count`, and `len` preserve or consume declared sequence order;
- `get` and `put` on `Map` do not expose traversal;
- `entries()` and `values()` on `Map` are finite orderless views;
- `values()` on `Table` is a sequence in key-constructor declaration order;
- `all`, `any`, and `count` over an orderless view are admitted because their
  result is traversal-invariant; and
- `Set::filter_map(source, binder)` accepts a checked finite collection and an
  arbitrary pure binder whose result is `Option<U>`. It drops `None`, inserts
  every `Some(U)` payload into the canonical `Set<U>`, and may consume an
  orderless view because the result has no traversal order.

A0 fixes the following additional source operations rather than relying on
implementation-only helpers:

- `Int::from(boundary)` returns `Option<Int>` and succeeds only for a finite,
  mathematically integral `BoundaryNumber`;
- `sequence.try_map(|value| ...)` preserves left-to-right order and returns
  `Some(Seq<U>)` only when every binder result is `Some(U)`;
- `map.try_map_values(|entry| ...)` binds the same closed
  `Entry { key, value }` shape as `entries()`, preserves every key, and returns
  `Some(Map<K,U>)` only when every binder result is `Some(U)`;
- `Map::from_unique(sequence)` returns `Some(Map<K,V>)` only when the dynamic
  sequence contains no duplicate typed key;
- `Set::from_unique(sequence)` returns `Some(Set<T>)` only when the dynamic
  sequence contains no duplicate typed value; and
- `map.entries_by_key()` is the deliberate ordered form. It returns a
  `Seq<(K,V)>` sorted by canonical typed-key bytes, whereas `entries()` remains
  an orderless `FiniteView<Entry<K,V>>`.

These are total operations. Failed numeric or uniqueness refinements return
`None`; they do not clamp, discard duplicates, select a winning map entry, or
fault. A `try_map_values` traversal order is unobservable because its only
results are `None` or a canonical map.

For example, L2's running-task projection is:

```uhura
computed running: Set<Running> =
  Set::filter_map(tasks.entries(), |entry|
    match entry.value.phase {
      Phase::Running { attempt, progress } =>
        Some(Running { task: entry.key, attempt, progress }),
      _ => None,
    }
  );
```

A pipe binder in one of these admitted operations is statically scoped and
non-escaping, not a general closure value. It may capture immutable values
visible to the enclosing pure expression. Its body is one pure expression: it
cannot assign, emit, call an update, use `return`, or be stored or returned. A
collection operation over a map or set is rejected if its result could reveal
traversal order; observable ordering requires an explicit `Seq` operation
with a declared ordering rule.

### Operators and conditional values

```uhura
ready && !busy
left_ready || right_ready
if condition { accepted_value } else { fallback_value }
```

`==` is typed complete-value equality. It performs neither coercion nor trait
dispatch and cannot be overloaded.

### Patterns and flow scope

`match value { ... }` selects one exhaustive pattern arm. Names bound by a
pattern exist only in that arm. A `return` in an arm remains a lexical return
from the enclosing function, update, or handler.

`value is pattern` is the non-exhaustive Boolean pattern test. `_` never binds.
Named bindings are admitted only in an `if` or `while` condition and are in
scope on paths where the test succeeded:

```uhura
if task.phase is Phase::Running { attempt, progress }
  && progress < 1.0
{
  // attempt and progress are in scope
}
```

`&&` and `||` evaluate left to right and short-circuit. A binding introduced
on the left of `&&` is available to its right and to the true branch. A
binding cannot escape through `||` or `!`, because success would not prove
which pattern introduced it. In a `while`, successful bindings are recreated
for each iteration and remain in scope in the body. Outside these flow
conditions, `is` may use only non-binding patterns such as `Some(_)` or
`Phase::Running { .. }` and simply produces `Bool`.

The 0.4 pattern set is closed to wildcard, binder, scalar and `Unit` literal,
tuple, `None`, `Some(pattern)`, nominal unit constructor, and nominal
struct/record-constructor patterns. A record pattern uses declared field
labels; an ordinary `field` is shorthand for `field: field`, while a keyword
label always requires `keyword: pattern`. One `..` may appear only last.
Without `..`, every declared field must appear exactly once. Sequence, map,
set, range, guard, reference, and user extractor patterns are not admitted.
An alternative pattern binds the same name/type set in every alternative.
Machine event, command, and outcome payloads remain positional protocol
patterns and do not acquire the braced data-enum form.

## 5. Machine declaration

A machine declares one complete transaction boundary.

Omitted contribution blocks have exact empty meanings:

| Omitted form | Lowering |
| --- | --- |
| root `config` | `Unit` configuration |
| root `outcomes` | Empty outcome sum with its unique empty policy, only when the complete input sum is empty |
| `events` | Empty local input sum |
| `commands` | Empty local command sum |
| `state` | `Unit` / empty owned-state product |
| `observe` | `Unit` / empty observation product |
| `invariant` | No obligations, equivalently `true` |
| `port` declarations | No local host requirements |
| `before commit` | Identity reconciliation |

Part contributions use the same empty meanings except that immutable part
configuration is declared as ordinary part parameters. Omitting root
`outcomes` lowers to the empty outcome sum and its unique empty policy. It is
admitted only when the fully composed input sum is empty and no handler,
update, or reconciliation form requires an `Outcome`; otherwise the complete
machine must declare `outcomes`. A part never declares a second outcome
family; it uses `requires outcomes` when one of its handlers or updates needs
the enclosing family.

### Configuration

```uhura
config {
  minimum: Int,
  maximum: Int,
}

require minimum <= maximum;
```

Configuration fields are immutable lexical names throughout the machine.
Every requirement is checked before state initialization.

### Events

```uhura
events {
  Increment,
  Changed(value: Text),
  Submit(task: TaskId, priority: Nat),
}
```

`events` declares the closed local input family. It is separate from `on`
because it is the public machine interface and because missing, duplicate, or
extra handlers must be diagnosable. Port receive families join it during
resolution.

Machine-domain variants deliberately use compact signature spelling:
`Changed(value: Text)` declares a named payload for interfaces and
diagnostics, while `Changed(value)` constructs or patterns it positionally.
This is an Uhura machine form, not a Rust enum struct variant. Braced
`Changed { value }`, an arity mismatch, or reordered payloads are rejected.
General data enums continue to use `Type::Variant { field }`.

### Commands

```uhura
commands {
  Start(task: TaskId, attempt: PositiveInt),
  Cancel(task: TaskId, attempt: PositiveInt),
}
```

Omitting `commands` means the local command family is empty. The 0.3
`command = Never` spelling is removed. Ports may still contribute commands.

### Ports

```uhura
port worker = WorkerPool { queue: "primary" };

on worker.Progress(task, attempt, value) {
  // classify the later worker report
  Accepted
}

on Submit(task, priority) {
  // priority may participate in owned scheduling state
  emit worker.Start(task, 1);
  Accepted
}
```

A port is a typed host requirement, not a runtime object. Its declared
receive constructors join the input domain, its send constructors join the
command domain, and its configuration is immutable data. A port-contract
`use` makes the contract name available; only the deployment manifest binds
authority.
Ports declared by a part are qualified by that part's stable composition name.

A root port configuration is a pure expression over root configuration and
constants. A part-owned port configuration may additionally read that part's
ordinary immutable configuration parameters. It cannot read state, computed
or observed values, dependency handles, or host facts. The canonical binding
expression is part of program IR, is evaluated and checked against the port
contract during atomic admission, and is reproduced from the complete
configuration during restore. It has no independent checkpoint state and
neither chooses nor initializes an adapter or authority.

A receive handler is always `on port_name.Variant(...)`; a send is always
`emit port_name.Variant(...)`. The dot denotes a qualified port contribution,
not Rust member lookup. Port qualification cannot be omitted. Port
names are unique within their owning machine or part, and constructor names
are unique within one contract direction. A local event or command may reuse a
constructor name because its unqualified domain is distinct; two ports may
also reuse one because their port names disambiguate them. After part
composition, the semantic path is
`part_name.port_name.Variant`.

The port name is not a runtime object. Outside the checked `on` and `emit`
forms, member access or calls on it are rejected. A host or evidence driver may
deliver a declared receive value; UI input binding cannot forge a port receive
unless a resolved framework feature explicitly owns that bridge.

### Outcomes

```uhura
outcomes {
  commit Accepted,
  abort Duplicate,
  abort Stale,
  abort Invalid(reason: InvalidReason),
}
```

The policy is written first because commit versus abort is the transaction
fact that distinguishes the constructors. Every constructor appears exactly
once. The block defines one closed outcome family and total policy function.

### State and computed values

```uhura
const LIMIT: Nat = 2;

state {
  tasks: Map<TaskId, Task> = Map::empty(),
  queue: Seq<TaskId> = [],
}

computed running_count: Nat =
  tasks.values().count(|task| task.phase is Phase::Running { .. });
```

State initializers cannot read another state field. A `computed` value is a
pure acyclic binding over configuration, state, pure functions, and any other
resolvable computed value. Dependency order comes from the checked acyclic
graph, never textual order. It is not independently mutable state.

A computed reference is evaluated against the state visible at that exact
reference. Inside a reaction or `before commit`, that is the current draft;
after an assignment, a later reference sees the updated draft. In observation,
it is the committed state being projected. An implementation may cache only
when invalidation preserves this rule.

### Invariants

```uhura
invariant {
  running_count <= LIMIT,
  queue.is_unique(),
  queue.all(|id| tasks.get(id) is Some(Task {
    phase: Phase::Queued,
    ..
  })),
}
```

`invariant expression;` is the canonical single-entry shorthand for
`invariant { expression }`.

Each invariant is a pure Boolean obligation over a complete draft. The checker
does not pretend to prove an arbitrary authored invariant unless the
conformance profile explicitly adds such proof.

Invariant entries within one owner are checked in authored order. Across a
composed machine, the root owner's sequence comes first; directly composed
part names then follow lexicographic order of their canonical identifier
bytes. The first false entry in that total order selects the stable fault
site. Source-file order and the placement of `part` declarations do not
participate. The composed order is semantic and retained in IR.

### Observation

```uhura
observe {
  tasks,
  queue,
  available_capacity: LIMIT - running_count,
}
```

A bare field is shorthand for `field: field`. Observation field types are
inferred and included in the public machine interface. `observe`, rather
than `view`, remains the core term because the same machine may be used
without UI. A `ui` normally binds the observation as `view`.

### Reactions

```uhura
on Progress(id, attempt, value) {
  if attempt <= 0 {
    return Invalid(InvalidReason::InvalidAttempt);
  }

  let next = match Ratio::checked_from(value) {
    None => return Invalid(InvalidReason::InvalidProgress),
    Some(progress) => progress,
  };

  let task = match tasks.get(id) {
    None => return Invalid(InvalidReason::UnknownTask),
    Some(found) => found,
  };

  tasks = tasks.put(id, Task {
    phase: Phase::Running {
      attempt,
      progress: next,
    },
    ..task
  });
  Accepted
}
```

The pattern after `on` must select exactly one resolved input constructor.
Every resolved constructor has exactly one handler. `on` is not callback
registration: it has no listener identity, priority, subscription lifetime,
dynamic ordering, suspension, or re-entry.

Assignment is a statement and is legal only when its target resolves to draft
state owned by the current machine or part. There is no general assignment
expression and no local reassignment.

An `on` block has expected type `Outcome`. Its tail outcome selects the
reaction result; `return outcome;` selects the same result early and remains
lexical through nested `if` and `match` blocks. Every handler path must produce
exactly one declared outcome; fallthrough is rejected.

`emit command` appends inert command data to the ordered draft buffer:

```uhura
emit Start(id, attempt);
```

It does not call a function or begin external work.

### Checked updates

An `update` is an in-transaction helper over the current draft:

```uhura
update resolve_terminal(
  id: TaskId,
  attempt: Int,
  terminal: Terminal,
) -> Outcome {
  // checked draft changes
  Accepted
}
```

Within a machine, `Outcome` names that machine's generated closed outcome
type. Within a part handler or private update, it names the explicit enclosing
outcome requirement recorded by the part and closed during composition.

It may read and write state owned by its declaration, call declared update
dependencies, and buffer declared commands. It cannot admit another input,
commit independently, create a receipt, suspend, re-enter, or access a host.

Private updates are visible only in their declaration. `pub update` creates
a checked dependency interface for static composition. A public part update
cannot accept, return, or contain `Outcome`; otherwise `Part::Updates` would
change type with the enclosing machine. It may return `Unit` or a part-owned
closed data result that the caller explicitly maps to its own outcome.

`return` from an update returns to its caller; it does not non-locally
terminate the reaction. A handler propagates an outcome-valued update
explicitly:

```uhura
on Succeed(id, attempt) {
  resolve_terminal(id, attempt, Terminal::Success)
}
```

Effectful update calls are admitted only in explicit sequential positions:

- a standalone statement when the result is `Unit`;
- the complete right side of `let name = update_call;`; or
- the tail expression or complete `return update_call;` of a handler or
  update.

They cannot nest inside an argument, operator, condition, tuple, struct field,
collection, binder, pure call, or state-assignment expression. Statements run
top to bottom, and each update completes before the next statement, so draft
changes and buffered command order are fixed without a general effectful
expression-evaluation rule.

Calling an outcome-valued update as a semicolon-terminated statement is
rejected. An update with no result annotation returns `Unit` and falls through
canonically; a non-`Unit` update must produce a value on every path. A tail
expression is canonical; explicit `return value;` remains available for early
return.

### Commit reconciliation

```uhura
before commit {
  while running_count < LIMIT
    && queue.uncons() is Some(Uncons { head: id, tail: rest })
  decreases(queue.len()) {
    let task = match tasks.get(id) {
      Some(task) => task,
      None => {
        unreachable;
      },
    };
    let attempt: PositiveInt = task.started + 1;

    queue = rest;
    tasks = tasks.put(id, Task {
      started: attempt,
      phase: Phase::Running {
        attempt,
        progress: 0.0,
      },
      ..task
    });
    emit Start(id, attempt);
  }
}
```

Only the enclosing `machine` may declare `before commit`, and it may declare
at most one. A `part` cannot declare one.

`while condition decreases(measure) { body }` is admitted in a handler,
update, or `before commit` only. There is no `break` or `continue`, and user
recursion is rejected rather than governed by a second notation.

The minimum 0.4 decrease contract is exact:

1. `measure` must check as `Nat` and must be either a `Nat` local/state
   projection or `.len()` applied to a `Seq` local/state projection. Calls,
   dependency reads, arbitrary computed values, indexing, conditionals, and
   compound arithmetic are not admitted as the written measure.
2. At each loop head the checker creates an immutable symbol `before` equal to
   that measure. It symbolically checks every finite control-flow path through
   the body. A `return` or `unreachable` path has no back edge; every other path
   must prove `0 <= next && next < before`, where `next` is the same measure
   re-evaluated after that path's assignments.
3. Proof obligations are decided in quantifier-free Presburger integer
   arithmetic: integer literals, equality and order, addition/subtraction, and
   multiplication by a literal integer only. The available assumptions are
   type-refinement bounds, true-path comparison facts from the loop condition
   and nested `if`/`match`/`is`, exact `let` equalities, and direct
   substitution through assignments.
4. The only collection equation introduced by the minimum solver is the
   normative `uncons` law: matching
   `sequence.uncons()` as `Some(Uncons { head, tail })` establishes
   `tail.len() + 1 = sequence.len()` for that path. No cardinality fact is
   guessed for `filter`, `without`, map/set traversal, or a user function.
5. A state assignment invalidates every assumption mentioning the assigned
   path or one of its projections before the assignment's exact substitution
   is added. A called update that may write a path used by the measure or its
   proof facts makes that path unprovable; 0.4 does not infer effect
   postconditions. A read-only call contributes no theorem beyond its declared
   result refinement.

The queue example is therefore mechanical: successful `uncons` proves
`rest.len() + 1 = before`; `queue = rest` substitutes
`next = rest.len()`; Presburger arithmetic proves `next < before`. A decrement
of `remaining: Nat` under `remaining > 0` is equally provable. Because `Nat`
is well founded, strict decrease on every back edge already implies that no
back edge can exist at zero; a separate runtime zero check is unnecessary.

The proof is compile-time only. Failure to decide the obligation in this
closed fragment rejects the source. `decreases` is erased after retaining its
source provenance; it never becomes a runtime assertion, timeout, fuel
counter, or fault.

Reconciliation runs only after a commit-policy outcome has been selected. It
may update root-owned state, call declared `Unit` updates, and emit commands;
those commands append after commands buffered by the handler and its updates.
It cannot use `return`, select or replace an outcome, call an outcome-valued
update, or cause an abort. It may fault through invariant or `unreachable`
failure.

`unreachable;` is a terminal reaction statement that lowers to
`unreachable_reached(SiteId)`. It is accepted only in a handler, update, or
`before commit`; pure functions, initializers, computed values, and
observation remain total without it. A block terminated by `unreachable;` has
no value and may inhabit the surrounding expected type; execution never
continues from it.

## 6. Source composition

### Parts are not runtime instances

A `part` owns a namespaced contribution to one enclosing machine:

```uhura
pub part Notice {
  state {
    message: Option<Text> = None,
  }

  pub computed current_message: Option<Text> = message;

  observe {
    message,
  }

  pub update show(next: Text) {
    message = Some(next);
  }

  pub update dismiss() {
    message = None;
  }
}
```

Composition is direct-only in 0.4. A module-level `machine` may compose a
module-level `part` declaration. A `part` body cannot contain another
`part name = ...` member, and a machine cannot be used where a part declaration
is required. The aggregate therefore has exactly two ownership levels: the
root and zero or more directly composed parts. There is no recursive component
tree hidden behind the flat kernel.

Every dependency-handle composition argument is exactly
`direct_sibling.reads` or `direct_sibling.updates`, where `direct_sibling` is
another part instance in the same machine. The referenced sibling may be
declared textually before or after the consumer because member order is
non-semantic. A dependency cannot be forwarded through a part parameter,
selected from a conditional, stored as data, or reached through two or more
part-name segments. The resolved direct-sibling dependency graph must be
acyclic for reads and updates under the existing cycle checks.

A part parameter whose type is ordinary data is immutable part
configuration; a parameter whose type is `OtherPart::Reads` or
`OtherPart::Updates` is a dependency handle:

```uhura
pub part Feed(
  page_size: PositiveInt,
  notice: Notice::Updates,
) {
  state {
    visible: Seq<Post> = [],
  }

  // ...
}
```

Parameters and composition arguments are positional and must match in arity,
order, and exact type. There are no default, rest, named, or overloaded part
arguments. An ordinary argument is a pure expression over enclosing
configuration parameters and constants; it cannot read state, computed
values, observation, or a dependency handle. It is bound during admission and
may be read by the part's requirements, state initializers, computed values,
handlers, updates, and observation. A dependency handle cannot be read from a
requirement or state initializer.

A machine composes parts under explicit stable names:

```uhura
use crate::notice::Notice;
use crate::feed::Feed;
use crate::search::Search;
use crate::session::Session;

pub machine Instagram {
  config {
    feed_page_size: PositiveInt,
  }

  outcomes {
    commit Accepted,
    abort Duplicate,
    abort Stale,
    abort Blocked,
    abort Invalid(reason: InvalidReason),
  }

  part session = Session();
  part notice = Notice();
  part feed = Feed(feed_page_size, notice.updates);
  part search = Search(session.reads, notice.updates);

  events {
    DismissNotice,
  }

  on DismissNotice {
    notice.updates.dismiss();
    Accepted
  }
}
```

`part name = Declaration(...)` is a static composition declaration. The
right-hand call shape binds immutable configuration and declared dependency
handles; it allocates no object and executes no constructor. The names on the
left determine semantic state, input, command, observation, diagnostic, and
editor provenance paths.

Ordinary part configuration is a pure projection of the complete machine
configuration, not another independently decoded domain. The canonical
binding expression is part of program IR; the resulting value is reproduced
from the complete configuration during restore. Dependency handles lower to
checked selector/update edges and are not checkpoint data.

The default composition is:

```text
state.notice
input.feed.*
command.feed.*
observation.notice.*
```

The semantic IR remains one aggregate machine. A formatter or editor may show
the authored hierarchy without claiming a runtime hierarchy.

### Dependency interfaces

A part opts a draft-aware selector into its read interface with
`pub computed`:

```uhura
pub part Session {
  state {
    user: Option<User> = None,
  }

  pub computed signed_in: Bool = user is Some(_);

  observe {
    signed_in,
  }
}
```

A reusable part then declares only the interfaces it needs:

```uhura
pub part Search(
  session: Session::Reads,
  notice: Notice::Updates,
) {
  requires outcomes {
    commit Accepted,
  }

  events {
    SearchRequested(query: Text),
  }

  on SearchRequested(query) {
    if !session.signed_in {
      notice.show("Sign in to search");
      return Accepted;
    }

    // ...
    Accepted
  }
}
```

Each public part has three associated interfaces nominally anchored to its
stable declaration identity:

- `Part::Observation` contains every field in the part's `observe` block and
  projects committed state for UI and external inspection;
- `Part::Reads` contains only value-like `pub computed` members;
- `Part::Updates` contains public in-transaction updates; and
- private state, functions, non-public computed values, handlers, ports, and
  commands do not enter a dependency interface.

Member signatures are checked exactly when a dependency handle is bound, but
a shape from another part declaration is not silently substituted for
`Part::Reads` or `Part::Updates`. Composition writes the boundary explicitly:

```uhura
part session = Session();
part notice = Notice();
part search = Search(session.reads, notice.updates);
```

Inside `Search`, `session.signed_in` evaluates a pure dependency member.
Source never writes `session.signed_in()`; parentheses are rejected because a
computed member is value-like, not a method. `notice.show(message)` is a
checked update call over the same transaction. Neither creates a message,
scheduler step, receipt, or runtime child.

A declared `Reads` member may appear in `computed`, `observe`, a handler, or an
`update`. In a handler, update, or root `before commit`, it sees the current
draft at that exact reference. When reached through observation, it sees the
committed state being projected. It is forbidden in constants, configuration
requirements, and state initializers. Read/computed dependencies must remain
statically acyclic across all parts.

`Part::Observation` never changes temporal meaning: it projects committed
state. It is not an in-transaction dependency. Every field written in
`observe` is externally visible in the complete namespaced observation; there
is no field-level `pub`.

### Part events and outcomes

A part may contribute declared events and their `on` handlers. Contributions
are qualified by the composition name. A part handler terminates with a
constructor required from the enclosing machine's outcome family.

A part declares that policy dependency explicitly:

```uhura
pub part Search(
  session: Session::Reads,
  notice: Notice::Updates,
) {
  requires outcomes {
    commit Accepted,
    abort Blocked,
    abort Invalid(reason: InvalidReason),
  }

  // events, state, updates, and handlers
}
```

`requires outcomes` contributes no second outcome family. It records the exact
constructor, payload, and commit/abort policy the part is allowed to return.
Composition succeeds only when the enclosing machine declares every entry
with an exact match. Conflicting requirements or a missing constructor are
link errors.

## 7. Required checks and non-local obligations

The source checker rejects:

1. zero or multiple owners for a mutable state path;
2. a nested part composition, a machine used as a part, a non-direct
   dependency handle, a dependency-handle expression, or duplicate composition
   or port names;
3. missing, duplicate, wildcard, or overlapping handlers;
4. direct access to another part's private state;
5. a cross-part read not present in a declared `Reads` interface;
6. a cross-part write not present in an update interface;
7. constant, update, selector, or computed-dependency cycles, or `pub const`
   inside a machine or part;
8. a missing, duplicate, or ambiguous logical-module mapping, or a `use`
   declaration that is unresolved, executes, initializes, or grants authority;
9. an unsatisfied port, outcome, read, or update requirement; omitted root
   outcomes with a non-empty composed input or an outcome-requiring form; a
   part-argument arity/type mismatch; or a part or port configuration binding
   that reads non-config data;
10. state initialization that depends on partially initialized sibling state;
11. a `before commit` inside a part, outcome selection during reconciliation,
    or multiple root reconciliation blocks;
12. a missing handler/update return, discarded or nested effectful update, a
    public part update exposing `Outcome`, or `return` in a `Unit` update with
    a value;
13. a non-literal, non-exhaustive, duplicate, or unknown-key `Table::from`,
    or a map/set operation that can expose traversal order;
14. an ambiguous constructor or an unqualified port receive/send constructor;
15. any user recursion, or a loop without the required static strict-decrease
    proof;
16. `unreachable` outside an admitted reaction context;
17. a declaration in a scope or with visibility excluded by the matrix, an
    empty optional grouping, or a member cardinality violation;
18. a namespace collision, illegal shadow, duplicate pattern binder, or
    alternative pattern with unequal binder sets;
19. an implicit refinement conversion, unresolved branch join, discarded
    non-`Unit` value, comparison chain, or expression that cannot synthesize
    uniquely without its required expected type; and
20. malformed UTF-8/core trivia, a non-ASCII symbolic name, non-JSON string
    escape, unsupported numeric spelling, or token sequence outside the
    normative grammar.

The compiler, runtime, and differential conformance suite must additionally
prove properties that cannot be rejected from one source fixture:

- admission of the complete composed program is atomic;
- every diagnostic and editor node retains authored source provenance; and
- source-layout-only changes preserve semantic IR identity.

Moving a declaration between files is behavior-preserving when its public name,
composition name, dependencies, and lowered semantic contribution are
unchanged.

## 8. Lowering table

| Source form | Semantic lowering |
| --- | --- |
| `pub` | Module visibility; absent from runtime |
| `use` / `pub use` | Inert name resolution and optional re-export; absent from runtime |
| `struct` | Source-nominal check, closed structural record values, and public schema identity when exported |
| `enum` | Closed nominal sum |
| `key` | Nominal wrapper over its admitted value type |
| `fn` | Pure total helper with no authority |
| `let` | Immutable lexical binding; absent after expression lowering |
| `part name = Part(...)` | Config bindings, dependency edges, and namespaced machine contribution |
| `events` | Local closed input sum |
| omitted `commands` | Empty local command sum |
| `port name = Contract { config };` | Qualified input and command families plus host requirement |
| `on port_name.Receive(...)` | Unique handler for one qualified port input |
| `emit port_name.Send(...)` | Qualified ordered command-buffer append |
| `outcomes` | Closed outcome sum plus total commit/abort policy |
| `requires outcomes` | Exact part-to-machine outcome policy constraint |
| `state` | Aggregate draft fields and initializers |
| `const` | Canonical closed typed value; no runtime initialization |
| exhaustive `Table::from` | Total table value in key-constructor order |
| `computed` | Pure acyclic call-site state selector |
| `pub computed` in a part | Member of the part's current-draft `Reads` interface |
| `observe` shorthand | Named pure committed-state observation field |
| `match` | Exhaustive closed-pattern selection |
| `value is pattern` | Refutable Boolean test with checked flow bindings |
| state assignment | Draft-state update statement |
| tail outcome or `return outcome;` in `on` | Terminal outcome selection |
| tail value or `return value;` in `fn` or `update` | Lexical result to caller |
| `emit command` | Ordered command-buffer append |
| `update` call | Statically checked same-transaction call |
| `before commit` | One root-owned reconciliation phase |
| `decreases(nat)` | Static strict-decrease loop proof obligation |
| `unreachable;` | Stable semantic program fault |
| pipe binder | Pure non-escaping collection binder |
| `Struct { fields, ..base }` | Closed persistent struct update |

The compiler must retain a source map from every lowered field, input,
handler, update, command, observation, and invariant to its module,
composition path, declaration, and source span.

## 9. Deliberate false friends

The surface is not Rust:

- `use` is inert resolution and `pub` is a two-level visibility marker; there
  is no module initialization, visibility lattice, orphan rule, or executable
  crate;
- no references, borrowing, lifetimes, ownership moves, `mut`, dereference, or
  interior mutability;
- no traits, `impl`, user-defined methods, method lookup, operator overloading,
  user-generic functions, trait bounds, implicit monomorphization, macros, or
  attributes; only explicitly admitted type, data, and contract constructors
  such as `Option<T>`, `Map<K,V>`, and `Router<Location>` use angle arguments;
- no `panic`, unwinding, partial indexing, destructors, machine-word overflow,
  `Result` propagation with `?`, or layout promises;
- no `async`, futures, threads, atomics, channels, or synchronization;
- no `unsafe`, `extern`, or FFI;
- no stored or returned closure values; pipes are non-escaping pure binders;
- no mutable local collections, iterators, or iterator side effects; prelude
  operations such as `put`, `remove`, and `append` return persistent values;
- no structural substitution at source boundaries for structs, enums, keys,
  ports, or generated part interfaces; and
- no assignment except an owned draft-state statement.

`"text"` is exact `Text`, not `&str` or `String`; `0.25` is exact `Decimal`,
not `f64`; `[a, b]` is `Seq<T>`, not a fixed array. Pattern matching never
moves or borrows a value, and `..base` copies a closed value without consuming
the base. Struct declaration field order contributes to canonical typed
encoding but makes no ABI or memory-layout promise. A `part` or `port`
call-shaped declaration is static composition and allocates or executes
nothing. `unreachable` produces a deterministic `ProgramFault`; it is neither
panic nor undefined behavior.
Bare names declared by `state` denote owned mutable draft slots inside
reactions; they are not Rust locals or implicit `self` fields, and Uhura has
no `self`, `&self`, or `&mut self` receiver model.

Unlike Rust, `pub struct Name` exposes its complete field contract, and the
source nominality check lowers values to closed record IR after checking.
Public schema identity remains; private names erase except for provenance.
Agents must not add `pub` to fields, infer ownership or layout, or assume a
Rust ABI.
Logical module segments are locators, not declaration identity: public names
are package-global, two modules cannot independently publish the same name,
and moving `crate::a::Thing` to `crate::b::Thing` preserves semantic identity
when its public name and lowered program are unchanged.

Nor does Rust-shaped source grant JavaScript, browser, filesystem, process,
clock, randomness, network, or storage globals. The surface is familiar where
its reading rules transfer. Where semantics do not transfer, Uhura specifies
and diagnoses the boundary.

## 10. Executable evidence and remaining design evidence

The core grammar, project/lock contract, and the 0.4 reconciliation of RFC
0003 remain disposable candidate inputs if executable evidence disproves
them. The in-tree implementation now establishes that:

- the checked-in complete 0.4 L0, L1, and L2 fixture parses, formats,
  reparses, lowers, and executes under this grammar;
- explicit modules, locked vendored packages, parts, dependencies, ports, and
  the `ui` profile lower through the retained machine kernel rather than a
  parallel runtime;
- A0 is admitted with its complete 12-preview evidence corpus alongside a
  separately executable, language-independent reference oracle; and
- Instagram exercises the existing Editor and Play integration through the
  0.4 frontend.

The remaining language-design evidence is the post-implementation repetition
of the equal-budget Rust-shaped-candidate and TypeScript-shaped-control
acquisition, repair, and controlled-change trials. RFC 0003's comment
attachment and checked authoring projection are also intentionally separate
work: the formatter refuses to erase unsupported attached comments until that
projection exists.

Those trials may refine spelling. They cannot introduce another runtime
machine, ambient authority, or a second concrete-syntax authority.
