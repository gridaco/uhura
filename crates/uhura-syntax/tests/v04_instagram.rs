use uhura_syntax::v04::{SourceIdentity, format, parse};

const MACHINE: &str = include_str!("../../../examples/instagram/client/machine.uhura");

#[test]
fn parses_and_formats_the_complete_instagram_machine_losslessly() {
    let parsed = parse(
        SourceIdentity::new(41, "app.instagram@1", "machine", "client/machine.uhura"),
        MACHINE,
    );

    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    assert_eq!(parsed.source_from_tokens(), MACHINE);

    let formatted = format(&parsed.module).expect("comment-free source must format");
    let reparsed = parse(
        SourceIdentity::new(42, "app.instagram@1", "machine", "client/machine.uhura"),
        &formatted,
    );
    assert!(
        reparsed.diagnostics.is_empty(),
        "formatted source must reparse:\n{:#?}",
        reparsed.diagnostics
    );
    assert_eq!(
        format(&reparsed.module).expect("formatted source must format again"),
        formatted,
        "formatter must be idempotent"
    );
}
