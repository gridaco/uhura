use uhura_syntax::v04::{
    FormatError, SourceIdentity, TriviaKind, UnsupportedComment, format, parse,
};

const PROGRAMS: &str = include_str!("../../../examples/programs/answers/uhura-0.4/programs.uhura");

fn identity(path: &str) -> SourceIdentity {
    SourceIdentity::new(19, "examples.programs@1", "programs", path)
}

fn parse_clean(path: &str, source: &str) -> uhura_syntax::v04::Parse {
    let parsed = parse(identity(path), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics for {path}:\n{:#?}",
        parsed.diagnostics
    );
    parsed
}

fn assert_round_trip(path: &str, source: &str) -> String {
    let parsed = parse_clean(path, source);
    let formatted = format(&parsed.module).expect("comment-free source must format");
    let reparsed = parse_clean(path, &formatted);
    let reformatted = format(&reparsed.module).expect("formatted source must format again");

    // The formatter is a complete structural projection of the AST: parsing
    // its output and projecting again must retain every represented choice.
    assert_eq!(reformatted, formatted, "formatter must be idempotent");
    assert_eq!(
        reparsed.module.uses.len(),
        parsed.module.uses.len(),
        "imports must survive formatting"
    );
    assert_eq!(
        reparsed.module.declarations.len(),
        parsed.module.declarations.len(),
        "declarations must survive formatting"
    );
    formatted
}

#[test]
fn formats_the_complete_l0_l1_l2_fixture_and_reparses_it() {
    let formatted = assert_round_trip("programs.uhura", PROGRAMS);
    assert!(formatted.starts_with("pub machine BoundedCounter {\n  config {\n"));
    assert!(formatted.contains("\n  before commit {\n"));
    assert!(formatted.ends_with("\n"));
    assert!(!formatted.ends_with("\n\n"));
}

#[test]
fn formats_every_core_declaration_member_expression_and_pattern_form() {
    let source = r#"use crate::shared::{Notice, Helper as LocalHelper};
use crate::other::Thing as LocalThing;
pub use vendor::api::PublicType;

pub struct Item {
  value: Text,
  pair: (Nat, Text),
  nested: Outer::Inner<Text>,
}

enum Choice {
  Empty,
  Value { value: Text },
}

pub key ItemId(Text);
pub const DEFAULT_VALUE: Text = "line\n\"quoted\"";

pub fn expressions(value: Item, other: Item) -> Text {
  let unit: () = ();
  let sequence = [true, false, 0, 1, 1.5, "text"];
  let tuple: (Item, Item) = (value, other);
  let grouped = (value);
  let record = Item {
    value: other.value,
    pair: (1, "one"),
    nested: other.nested,
    ..value
  };
  let empty = Choice::Empty {};
  let block = {
    let inside = other;
    inside
  };
  let call = collect(value.member[0], |item| item + 1);
  let operators = !false || -1 * 2 + 3 - 4 == 5 && 6 != 7;
  let comparisons = 1 < 2 && 2 <= 3 && 3 > 2 && 3 >= 3;
  let tested = value is Item { value: name, .. };
  let selected = if true {
    value
  } else if false {
    other
  } else {
    record
  };
  if false {
    return;
  }
  let matched = match selected {
    true => "bool",
    false => "bool",
    -1 => "integer",
    -1.5 => "decimal",
    "text" => "text",
    () => "unit",
    (left, right) => "tuple",
    (single) => "group",
    Some(inner) => "some",
    None => "none",
    Choice::Empty => "constructor",
    Item { value, pair: renamed, .. } => "record",
    Choice::Empty | Choice::Value { .. } => "alternative",
    _ => "wildcard",
  };
  if true {} else {}
  match value {
    _ => (),
  }
  value;
  return matched
}

pub part Worker(seed: Text) {
  require seed != "";

  requires outcomes {
    commit Accepted,
    abort Refused(reason: Text),
  }

  const LIMIT: Nat = 2;

  fn normalize(value: Text) -> Text {
    value
  }

  events {
    Start(value: Text),
  }

  commands {
    Logged(value: Text),
  }

  port clock = ClockPort { zone: seed };

  state {
    current: Option<Text> = None,
  }

  pub computed visible: Bool = current is Some(_);

  invariant true;

  observe {
    current,
    ready: true,
  }

  on Start(value) {
    current = Some(value);
    emit Logged(value);
    emit clock.Logged(value);
    Accepted
  }

  on clock.Tick(now) {
    let ignored = now;
    Accepted
  }

  pub update clear() -> Outcome {
    current = None;
    Accepted
  }
}

pub machine Application {
  config {
    label: Text,
  }

  require label != "";

  const ZERO: Nat = 0;

  fn identity(value: Text) -> Text {
    return value;
  }

  part worker = Worker(label);

  events {
    Started,
  }

  commands {
    Ready,
  }

  port router = Router {};

  outcomes {
    commit Accepted,
    abort Refused(reason: Text),
  }

  state {
    count: Nat = ZERO,
  }

  computed doubled: Int = count * 2;

  computed unlabeled = count;

  invariant {
    count >= 0,
    doubled >= 0,
  }

  observe {
    count,
  }

  on Started {
    while count > 0 decreases(count) {
      count = count - 1;
      unreachable;
    }
    emit Ready;
    Accepted
  }

  update clear() {
    count = 0;
  }

  before commit {
    worker.clear();
  }
}
"#;

    let formatted = assert_round_trip("all-core.uhura", source);
    assert!(formatted.contains("pub part Worker(seed: Text) {"));
    assert!(formatted.contains("Choice::Empty | Choice::Value {"));
    assert!(formatted.contains("while count > 0 decreases(count) {"));
    assert!(formatted.contains("port router = Router {};"));
}

#[test]
fn inserts_only_the_parentheses_required_by_the_ast() {
    let source = r#"const VALUE: Bool = (a || b) && c || d && e;

fn arithmetic(a: Int, b: Int, c: Int) -> Int {
  a - (b - c) + a * (b + c)
}
"#;
    let formatted = assert_round_trip("precedence.uhura", source);
    assert!(formatted.contains("const VALUE: Bool = (a || b) && c || d && e;"));
    assert!(formatted.contains("a - (b - c) + a * (b + c)"));
}

#[test]
fn refuses_to_silently_delete_comments_until_attachment_is_modeled() {
    let source = r#"//! Module documentation.
// ordinary module note
/// Declaration documentation.
pub struct Item {
  value: Text,
}
"#;
    let parsed = parse_clean("comments.uhura", source);
    let error = format(&parsed.module).expect_err("comments must be refused explicitly");
    let FormatError::UnsupportedComments { comments } = error;
    assert_eq!(
        comments,
        vec![
            UnsupportedComment {
                kind: TriviaKind::InnerDoc,
                text: "//! Module documentation.".into(),
                span: comments[0].span,
            },
            UnsupportedComment {
                kind: TriviaKind::OrdinaryComment,
                text: "// ordinary module note".into(),
                span: comments[1].span,
            },
            UnsupportedComment {
                kind: TriviaKind::OuterDoc,
                text: "/// Declaration documentation.".into(),
                span: comments[2].span,
            },
        ]
    );
}
