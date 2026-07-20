# Rust-shaped Uhura 0.4 reference

This is a bounded paper reference derived from the active 0.4 candidate. It is
not a second syntax authority. When the trial is rerun through an executable
frontend, the accepted grammar and diagnostics replace paper adjudication.

The source borrows Rust's reading shape. It does **not** borrow Rust ownership,
execution, layout, or library semantics.

## Files, names, and declarations

Resolved imports and public declarations use:

```uhura
use crate::notice::Notice;
use vendor::identity::{AccountId, Session};
pub use crate::identity::UserId;
```

`use` resolves names only. It does not initialize a module or acquire
authority. Top-level declarations are private unless marked `pub`. `pub use`
re-exports the same declaration identity and does not accept `as`.

Names use:

- `UpperCamelCase` for types and data variants;
- `snake_case` for values, functions, fields, parts, and ports; and
- `SCREAMING_SNAKE_CASE` for constants.

Core declarations:

```uhura
key RequestId(Text);

pub struct Request {
  id: RequestId,
  label: Text,
}

enum Phase {
  Idle,
  Active {
    request: RequestId,
    ratio: Ratio,
  },
}

const LIMIT: Nat = 2;

fn opposite(value: Bool) -> Bool {
  !value
}
```

A `pub struct` exposes its complete field contract. Fields never carry their
own `pub`. Structs, enums, and keys are nominal at source boundaries. General
data enum variants use `Type::Variant` and braced fields:

```uhura
Phase::Idle
Phase::Active { request, ratio }
```

There are no references, borrows, lifetimes, ownership moves, `mut`, `self`,
traits, `impl`, methods, macros, attributes, generics declared by users,
exceptions, panics, async functions, threads, or unsafe code.

## Values and expressions

Core literals include:

```uhura
true
42
-7
0.25
"exact Unicode text"
[first, second]
(left, right)
Some(value)
None
()
```

`Int` is an arbitrary integer, `Nat` is non-negative, and `PositiveInt` is at
least one. `Decimal` is exact. A `Ratio` is an exact decimal in inclusive
`0..1`. `BoundaryNumber` is a decoded external number that may be non-finite;
convert it explicitly:

```uhura
let ratio = match Ratio::checked_from(value) {
  None => return Invalid,
  Some(valid) => valid,
};
```

Strings are exact `Text`. No trimming or Unicode normalization is implicit.
`[a, b]` is `Seq<T>`, not an array. Values do not have truthiness or implicit
coercions.

Use immutable `let`, `if`, and exhaustive `match`:

```uhura
let next = if ready { primary } else { fallback };

match phase {
  Phase::Idle => None,
  Phase::Active { request, ratio } => Some((request, ratio)),
}
```

A block's final expression is its value. `return value;` returns lexically
from the enclosing function, handler, or update. Semicolons terminate
statements; there is no automatic semicolon insertion.

`value is pattern` is a Boolean pattern test. A binding introduced on the left
of `&&` is available on the right and on the true path. Bindings do not escape
through `||`, `!`, or a false path.

Struct values and persistent updates use:

```uhura
Request { id, label }
Task { phase: Phase::Idle, ..task }
```

`..base` copies a closed value. It does not consume or mutate `base`.

## Total collection operations

Only declared total operations exist. The tasks use:

```text
Ratio::checked_from(BoundaryNumber) -> Option<Ratio>
NonEmpty::checked_from(Seq<T>)      -> Option<NonEmpty<T>>
Seq::from_options(Seq<Option<T>>)   -> Seq<T>

Seq<T>.append(T)                    -> Seq<T>
Seq<T>.without(T)                   -> Seq<T>
Seq<T>.uncons()                     -> Option<Uncons<T>>
Seq<T>.len()                        -> Nat
Seq<T>.is_empty()                   -> Bool
Seq<T>.contains(T)                  -> Bool

Map::empty()                        -> Map<K,V>
Map<K,V>.get(K)                     -> Option<V>
Map<K,V>.put(K,V)                   -> Map<K,V>
Map<K,V>.remove(K)                  -> Map<K,V>
Map<K,V>.values()                   -> orderless finite view

Set::empty()                        -> Set<T>
Set<T>.add(T)                       -> Set<T>
Set<T>.remove(T)                    -> Set<T>
Set<T>.contains(T)                  -> Bool
```

`Seq` order is observable. Map and set traversal order is not. An orderless
finite view permits only traversal-invariant operations such as `all`, `any`,
or `count`.

An exhaustive fixed-key table uses a literal:

```uhura
const FLAGS: Table<Flag, Bool> = Table::from([
  (Flag::Alpha, false),
  (Flag::Beta, true),
]);
```

`Table::from` accepts only a literal with exactly one entry for every unit
variant of the key enum. `Table::from(entries)` is invalid.

Pipe syntax such as `|item| item.ready` is a pure, non-escaping binder admitted
only by a declared collection operation. It is not a stored closure.

## Machines

A complete machine normally has this shape:

```uhura
pub machine Example {
  config {
    limit: PositiveInt,
  }

  require limit <= 8;

  events {
    Begin(id: RequestId),
    Cancel(id: RequestId),
  }

  commands {
    Start(id: RequestId),
  }

  outcomes {
    commit Accepted,
    abort Duplicate,
    abort Invalid,
  }

  state {
    active: Option<RequestId> = None,
  }

  invariant active is None || limit > 0;

  computed busy: Bool = active is Some(_);

  observe {
    active,
    busy,
  }

  on Begin(id) {
    if active is Some(_) {
      return Duplicate;
    }
    active = Some(id);
    emit Start(id);
    Accepted
  }

  on Cancel(id) {
    // ...
    Accepted
  }
}
```

Configuration names are immutable. A `require` is checked before state
initialization. Every event constructor has exactly one handler.

Machine-domain declarations use compact named signatures but positional
construction and patterns:

```uhura
events {
  Changed(value: Text),
}

on Changed(value) {
  // ...
}
```

`Changed { value }` is invalid even though general data enums use braced
fields.

An outcome block defines both a closed result family and its commit/abort
policy. Every handler path produces exactly one outcome. A commit publishes
draft state and commands; an abort publishes neither. `emit` buffers command
data and never performs work.

Bare names declared by `state` are owned draft slots inside a handler. State
assignment is the only assignment. Locals are immutable and cannot be
reassigned.

`computed` is a pure current-state selector. `observe` is a pure committed
projection. Neither is mutable state.

## Checked updates and bounded reconciliation

An `update` is a same-transaction helper:

```uhura
update clear() {
  active = None;
}
```

An effectful update call is admitted only as:

- a standalone statement when it returns `Unit`;
- the entire right side of `let value = update_call;`; or
- the complete tail or `return update_call;` of a handler or update.

It cannot nest inside another argument, condition, operator, literal, pure
call, or state-assignment expression.

The root machine may declare one finite reconciliation:

```uhura
before commit {
  while queue.uncons() is Some(Uncons { head: id, tail: rest })
    decreases(queue.len()) {
    queue = rest;
    emit Start(id);
  }
}
```

The `Nat` measure must be statically strict on every back edge. There is no
`break`, `continue`, recursion, or runtime timeout. `unreachable;` is the
spelling for a deterministic program fault in a reaction context;
`unreachable!()` does not exist.

## Parts, ports, and UI false boundaries

```uhura
part notice = Notice();
part search = Search(session.reads, notice.updates);
port worker = WorkerPool { queue: "primary" };
```

These call-shaped declarations bind static configuration and checked
dependencies. They allocate and execute nothing.

A computed dependency is value-like:

```uhura
if session.signed_in {
  // ...
}
```

`session.signed_in()` is invalid. An update dependency is an effectful
same-transaction call and follows the update-call placement rules.

Port receives and sends remain qualified:

```uhura
on worker.Completed(id) { /* ... */ }
emit worker.Start(id);
```

The web profile requires direct, unaliased activation in the same logical
module:

```uhura
use uhura::ui;
```

An alias or transitive re-export does not activate UI syntax. Logical module
paths locate declarations; they do not define public semantic identity.
