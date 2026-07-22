//! Shared normalization policy for source-authored markup metadata.

/// Normalize line endings, edge whitespace, and common multiline indentation.
pub(crate) fn normalize_markup_text(value: &str) -> String {
    let value = value.replace("\r\n", "\n").replace('\r', "\n");
    if !value.contains('\n') {
        return value.trim_matches([' ', '\t']).to_string();
    }

    let mut lines = value.split('\n').collect::<Vec<_>>();
    while lines
        .first()
        .is_some_and(|line| line.chars().all(|value| matches!(value, ' ' | '\t')))
    {
        lines.remove(0);
    }
    while lines
        .last()
        .is_some_and(|line| line.chars().all(|value| matches!(value, ' ' | '\t')))
    {
        lines.pop();
    }
    let mut lines = lines
        .into_iter()
        .map(|line| line.trim_end_matches([' ', '\t']).to_string())
        .collect::<Vec<_>>();
    let common = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.bytes().take_while(|value| *value == b' ').count())
        .min()
        .unwrap_or(0);
    for line in &mut lines {
        let remove = line
            .bytes()
            .take_while(|value| *value == b' ')
            .count()
            .min(common);
        line.drain(..remove);
    }
    lines.join("\n")
}
