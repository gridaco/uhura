use uhura_base::FileId;
use uhura_syntax::ast::{NavTarget, Stmt};
use uhura_syntax::{Parsed, SourceKind, format_module, parse};

const SOURCE: &str = r#"page

store {
  on reset(target: id) {
    navigate replace profile(user: target)
  }
}

<view />
"#;

#[test]
fn navigate_replace_parses_and_formats_as_a_distinct_target() {
    let parsed = parse(FileId(0), SOURCE, SourceKind::Module);
    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        parsed.diagnostics
    );
    let Parsed::Module(file) = parsed.parsed else {
        panic!("expected module")
    };
    let store = file.store.as_ref().expect("store");
    let Stmt::Navigate { target, .. } = &store.handlers[0].body[0] else {
        panic!("expected navigate statement")
    };
    let NavTarget::Replace { name, args } = target else {
        panic!("expected replace target")
    };
    assert_eq!(name, "profile");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].name, "user");

    let formatted = format_module(&file);
    assert!(
        formatted.contains("navigate replace profile(user: target)"),
        "{formatted}"
    );
    let reparsed = parse(FileId(1), &formatted, SourceKind::Module);
    assert!(
        reparsed.diagnostics.is_empty(),
        "formatted source must reparse: {:?}",
        reparsed.diagnostics
    );
}
