use std::collections::BTreeMap;

use uhura_base::{Ident, Severity};
use uhura_check::manifest::Manifest;
use uhura_check::{CheckInput, SourceInput, check};
use uhura_core::ir::{ExprIr, StmtIr};
use uhura_syntax::SourceKind;

const CATALOG: &str = include_str!("../../../examples/instagram-uhura/catalog/base.toml");

fn ident(value: &str) -> Ident {
    Ident::new(value).unwrap()
}

fn input(home: &str) -> CheckInput {
    let manifest = Manifest {
        app_name: ident("navigation-test"),
        entry: ident("home"),
        catalog_path: "catalog/base.toml".into(),
        ports: BTreeMap::new(),
        fixtures: BTreeMap::new(),
        assets_manifest: None,
        play: BTreeMap::new(),
    };
    CheckInput {
        manifest,
        manifest_rel_path: "uhura.toml".into(),
        manifest_text: "# constructed test manifest".into(),
        catalog_file: ("catalog/base.toml".into(), Some(CATALOG.into())),
        port_files: BTreeMap::new(),
        sources: vec![
            SourceInput {
                rel_path: "app/home/page.uhura".into(),
                text: home.into(),
                kind: SourceKind::Module,
            },
            SourceInput {
                rel_path: "app/profile/[user]/page.uhura".into(),
                text: "page\n\nparam user: id\n\n<view />\n".into(),
                kind: SourceKind::Module,
            },
        ],
        theme_css: None,
        fixture_files: BTreeMap::new(),
        lock_text: None,
    }
}

#[test]
fn replace_uses_route_checks_and_lowers_with_named_arguments() {
    let output = check(&input(
        "page\n\nstore {\n  on reset(target: id) {\n    navigate replace profile(user: target)\n  }\n}\n\n<view />\n",
    ));
    let errors: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "replace should check clean: {errors:?}");

    let program = output.lowered.expect("clean check lowers").program;
    assert!(
        program
            .to_canonical_string()
            .contains("\"navigate-replace\""),
        "the checked IR wire form keeps replace distinct"
    );
    let statement = &program.pages[&ident("home")].handlers[0].body[0];
    let StmtIr::NavigateReplace { route, args } = statement else {
        panic!("replace must remain distinct in checked IR: {statement:?}")
    };
    assert_eq!(route, &ident("profile"));
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].name, ident("user"));
    assert_eq!(args[0].value, ExprIr::BindingRef(ident("target")));
}

#[test]
fn replace_rejects_unknown_routes_like_push_navigation() {
    let output = check(&input(
        "page\n\nstore {\n  on reset() {\n    navigate replace nowhere()\n  }\n}\n\n<view />\n",
    ));
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH4001" && diagnostic.message.contains("nowhere")),
        "unknown replace route must be diagnosed: {:?}",
        output.diagnostics
    );
    assert!(output.lowered.is_none(), "errors gate lowering");
}
