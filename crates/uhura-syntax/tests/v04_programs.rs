use uhura_syntax::v04::ast::{DeclarationKind, Module, SourceIdentity};
use uhura_syntax::v04::parse;

const PROGRAMS: &str = include_str!("../../../examples/programs/answers/uhura-0.4/programs.uhura");

fn identity(path: &str) -> SourceIdentity {
    SourceIdentity::new(7, "examples.programs@1", "programs", path)
}

#[test]
fn parses_complete_l0_l1_l2_programs_losslessly() {
    let parsed = parse(identity("programs.uhura"), PROGRAMS);
    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    assert_eq!(parsed.source_from_tokens(), PROGRAMS);
    assert_eq!(parsed.module.source, PROGRAMS);
    assert_eq!(parsed.module.identity.package, "examples.programs@1");
    assert_eq!(parsed.module.identity.module, "programs");
    assert_eq!(
        parsed
            .module
            .declarations
            .iter()
            .filter(|declaration| matches!(declaration.kind, DeclarationKind::Machine(_)))
            .count(),
        3
    );
    assert!(parsed.tokens.iter().any(|token| !token.leading.is_empty()));
}

#[test]
fn parses_every_core_declaration_and_member_shape() {
    let source = r#"//! Module documentation.
use crate::shared::{Notice, Helper as LocalHelper};
pub use vendor::api::PublicType;

pub struct Message {
  text: Text,
  priority: Nat,
}

enum Delivery {
  Pending,
  Sent { at: Nat },
}

pub key MessageId(Text);
pub const DEFAULT_PRIORITY: Nat = 1;

pub fn choose(left: Text, right: Text) -> Text {
  if left == "" { right } else { left }
}

pub part Notice(seed: Text) {
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
    Show(value: Text),
    Hide,
  }

  commands {
    Logged(value: Text),
  }

  port clock = ClockPort { zone: seed, };

  state {
    message: Option<Text> = None,
    remaining: Nat = LIMIT,
  }

  pub computed current: Option<Text> = message;
  invariant remaining <= LIMIT;

  observe {
    message,
    visible: message is Some(_),
  }

  on Show(value) {
    message = Some(normalize(value));
    emit Logged(value);
    Accepted
  }

  on clock.Tick(now) {
    let pair: (Nat, Text) = (now, seed);
    if now == 0 {
      return Refused("early");
    }
    Accepted
  }

  pub update dismiss() {
    message = None;
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

  part notice = Notice(label);

  events {
    Started,
  }

  commands {
    Ready,
  }

  port router = Router { initial: label, };

  outcomes {
    commit Accepted,
    abort Refused(reason: Text),
  }

  state {
    count: Nat = ZERO,
  }

  computed doubled: Int = count * 2;

  invariant {
    count >= 0,
    doubled >= 0,
  }

  observe {
    count,
  }

  on Started {
    notice.dismiss();
    emit Ready;
    Accepted
  }

  on router.Changed(next) {
    let selected = match next {
      Some(value) => value,
      None => identity(label),
    };
    count = selected.len();
    Accepted
  }

  update clear() {
    count = 0;
  }

  before commit {
    while count > 0 decreases(count) {
      count = count - 1;
    }
  }
}
"#;
    let parsed = parse(identity("all-core.uhura"), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    assert_eq!(parsed.module.uses.len(), 2);
    assert_eq!(parsed.module.declarations.len(), 7);
    assert_eq!(parsed.source_from_tokens(), source);
}

#[test]
fn v04_module_is_serde_ready() {
    fn assert_wire<T: serde::Serialize + for<'de> serde::Deserialize<'de>>() {}
    assert_wire::<Module>();
}
