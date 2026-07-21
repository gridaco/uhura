# Rust-shaped false-friend worksheet

Each snippet is intended to preserve the behavior described in its caption.
Diagnose and replace the transferred assumption.

## F-R01 — immutable local

```uhura
fn increment(value: Int) -> Int {
  let mut result = value;
  result = result + 1;
  result
}
```

Intent: return the value plus one without changing machine state.

## F-R02 — values, not references

```uhura
fn label(request: &Request) -> Text {
  request.label
}
```

Intent: read a closed request value.

## F-R03 — public record contract

```uhura
pub struct Request {
  pub id: RequestId,
  pub label: Text,
}
```

Intent: publish the complete request schema.

## F-R04 — machine-domain payload

```uhura
events {
  Changed(value: Text),
}

on Changed { value } {
  message = value;
  Accepted
}
```

Intent: handle the declared changed input.

## F-R05 — computed dependency

```uhura
if session.signed_in() {
  Accepted
} else {
  Blocked
}
```

Intent: read a declared `Session::Reads` computed member.

## F-R06 — nested effectful update

```uhura
notice.show(session.updates.refresh_label());
```

Intent: refresh a label through one declared update, then show the returned
label through another declared update in the same transaction.

## F-R07 — dynamic exhaustive table

```uhura
let entries = build_entries();
let flags: Table<Flag, Bool> = Table::from(entries);
```

Intent: construct a total table for the closed unit enum `Flag`.

## F-R08 — deterministic fault

```uhura
None => {
  unreachable!();
},
```

Intent: mark a reaction branch as a deterministic program fault.

## F-R09 — UI activation

```uhura
use uhura::ui as app_ui;

pub ui Page for Application(view) {
  <main>{view.title}</main>
}
```

Intent: activate and declare checked UI in this logical module.

## F-R10 — declaration identity

An author moves public declaration `Notice` from logical module
`crate::old` to `crate::new`, updates every `use`, and also migrates persisted
semantic identity because the module path changed.

Intent: move source without changing the declaration's package-global public
name or lowered program.
