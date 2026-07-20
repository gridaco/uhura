use uhura_check::check_v04_module;
use uhura_syntax::v04::{SourceIdentity, format, parse};

fn check(source: &str) -> uhura_check::CheckOutput {
    let parsed = parse(
        SourceIdentity::new(91, "example.application-bridges@1", "main", "main.uhura"),
        source,
    );
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    let formatted = format(&parsed.module).expect("bridge source formats");
    let reparsed = parse(
        SourceIdentity::new(91, "example.application-bridges@1", "main", "main.uhura"),
        &formatted,
    );
    assert!(
        reparsed.diagnostics.is_empty(),
        "formatted parse diagnostics:\n{:#?}",
        reparsed.diagnostics
    );
    check_v04_module(&reparsed.module)
}

#[test]
fn imported_token_record_constructors_lower_in_values_and_patterns() {
    let output = check(
        r#"
use uhura::boundary::Token;
use uhura::ui;

pub enum Reason {
  Damaged,
  NotNeeded,
}

pub machine TokenProbe {
  events {
    Choose(value: Token<Reason>),
  }

  outcomes {
    commit Applied,
    abort Invalid,
  }

  state {
    reason: Option<Reason> = None,
  }

  observe {
    reason,
  }

  on Choose(value) {
    let selected = match value {
      Token::Known { value } => value,
      Token::Unknown { value: _ } => return Invalid,
    };
    reason = Some(selected);
    Applied
  }
}

pub ui TokenProbeUi for TokenProbe(view) {
  <button on press -> Choose(Token::Known { value: Reason::Damaged })>
    Choose damaged
  </button>
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("Token bridge checks");
    assert!(
        program.machine_program.machines["example.application-bridges@1::TokenProbe"]
            .handlers
            .contains_key("Choose")
    );
    assert!(
        program
            .presentations
            .contains_key("example.application-bridges@1::TokenProbeUi")
    );
}

#[test]
fn map_try_map_values_expands_one_entry_binder_to_key_and_value() {
    let output = check(
        r#"
pub key ItemId(Text);

pub enum Reason {
  Damaged,
}

pub struct DraftSelection {
  reason: Option<Reason>,
}

pub struct SubmittedSelection {
  reason: Reason,
}

pub const ITEM: ItemId = ItemId("item");

fn submitted(
  selections: Map<ItemId, DraftSelection>,
) -> Option<Map<ItemId, SubmittedSelection>> {
  selections.try_map_values(|entry| match entry.value.reason {
    None => None,
    Some(reason) => Some(SubmittedSelection { reason }),
  })
}

pub machine MapProbe {
  outcomes {
    commit Done,
  }

  state {
    selections: Map<ItemId, DraftSelection> = Map::from([
      (ITEM, DraftSelection { reason: Some(Reason::Damaged) }),
    ]),
  }

  observe {
    submitted: submitted(selections),
  }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert!(output.program.is_some());
}

#[test]
fn ui_entries_by_key_call_is_an_ordered_sequence() {
    let output = check(
        r#"
use uhura::ui;

pub key ItemId(Text);

pub struct Item {
  title: Text,
}

pub const ITEM: ItemId = ItemId("item");
pub const ITEMS: Map<ItemId, Item> = Map::from([
  (ITEM, Item { title: "Item" }),
]);

pub machine ListProbe {
  outcomes {
    commit Done,
  }

  state {
    items: Map<ItemId, Item> = ITEMS,
  }

  observe {
    items,
  }
}

pub ui ListProbeUi for ListProbe(view) {
  <main>
    {#each view.items.entries_by_key() as (id, item) (id)}
      <p>{item.title}</p>
    {/each}
  </main>
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert!(output.program.is_some());
}
