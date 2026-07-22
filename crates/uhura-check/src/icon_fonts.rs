//! Checked icon-family registries. The checker owns names and codepoints;
//! renderers receive only validated, content-addressed font resources.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::io::{self, Read};
use std::sync::{Arc, OnceLock};

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use ttf_parser::{Face, Tag};
use uhura_base::{Diagnostic, FileId, Ident, Severity, Span, codes, hash_json, sha256_hex};
use uhura_core::ir::{Expr, SourceRef, UiAttribute, UiAttributeValue, UiNode};
use uhura_core::{Program, Value};
use wuff::decompress_woff2_with_custom_brotli;

use crate::resource_manifest::IconsConfig;

const LUCIDE_FONT: &[u8] = include_bytes!("../../../resources/icon-fonts/lucide/lucide.woff2");
const LUCIDE_GLYPHS: &str = include_str!("../../../resources/icon-fonts/lucide/glyphs.json");
pub const MAX_ICON_FONT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DECODED_ICON_FONT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_ICON_GLYPH_MAP_BYTES: usize = 4 * 1024 * 1024;
const MAX_ICON_FONT_TABLE_BYTES: usize = 2 * 1024 * 1024;

static LUCIDE_CHECKED: OnceLock<Result<CheckedIconFamily, Vec<IconFontIssue>>> = OnceLock::new();

/// App-provided bytes for one family declared as `[icons.<alias>]`.
/// The duplicated paths make the pure checker verify that a host associated
/// the bytes with the manifest declaration it claims to satisfy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconFontInput {
    pub font_path: String,
    pub font_bytes: Option<Arc<[u8]>>,
    pub glyphs_path: String,
    pub glyphs_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedIconFamily {
    pub font: Arc<[u8]>,
    pub glyphs: BTreeMap<Ident, u32>,
    pub font_hash: String,
    pub glyphs_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedIconFonts {
    pub default: Ident,
    pub families: BTreeMap<Ident, CheckedIconFamily>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconFontIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconTokenIssue {
    pub code: &'static str,
    pub rule: &'static str,
    pub source: SourceRef,
    pub message: String,
}

/// Check every logical icon token against the exact project registry before
/// any renderer receives the program.
///
/// `family` is deliberately literal before v1. `name` may be a literal or a
/// finite expression composed from literals, constants, `if`, and `match`.
pub fn check_program_icon_tokens(
    program: &Program,
    fonts: &CheckedIconFonts,
) -> Vec<IconTokenIssue> {
    let mut issues = Vec::new();
    for presentation in program.presentations.values() {
        check_icon_nodes(program, fonts, &presentation.nodes, &mut issues);
    }
    for component in program.components.values() {
        check_icon_nodes(program, fonts, &component.nodes, &mut issues);
    }
    issues
}

fn check_icon_nodes(
    program: &Program,
    fonts: &CheckedIconFonts,
    nodes: &[UiNode],
    issues: &mut Vec<IconTokenIssue>,
) {
    for node in nodes {
        match node {
            UiNode::Element {
                name,
                attributes,
                children,
                source,
            } => {
                if name == "icon" {
                    check_icon_element(program, fonts, attributes, source, issues);
                }
                check_icon_nodes(program, fonts, children, issues);
            }
            UiNode::If { children, .. } | UiNode::Each { children, .. } => {
                check_icon_nodes(program, fonts, children, issues);
            }
            UiNode::Match { cases, .. } => {
                for case in cases {
                    check_icon_nodes(program, fonts, &case.children, issues);
                }
            }
            UiNode::Call { .. } | UiNode::Text { .. } | UiNode::Interpolation { .. } => {}
        }
    }
}

fn check_icon_element(
    program: &Program,
    fonts: &CheckedIconFonts,
    attributes: &[UiAttribute],
    element_source: &SourceRef,
    issues: &mut Vec<IconTokenIssue>,
) {
    let family_attribute = attributes
        .iter()
        .find(|attribute| attribute.name == "family");
    let family = match family_attribute.map(|attribute| &attribute.value) {
        None => fonts.default.as_str(),
        Some(UiAttributeValue::Text { value }) => value,
        Some(UiAttributeValue::Expression { .. } | UiAttributeValue::Event { .. }) => {
            issues.push(IconTokenIssue {
                code: codes::UNKNOWN_ICON_FAMILY.0,
                rule: "uhura/dynamic-icon-family",
                source: family_attribute.map_or_else(
                    || element_source.clone(),
                    |attribute| attribute.source.clone(),
                ),
                message: "icon `family` must be a quoted project icon-family name".into(),
            });
            return;
        }
    };

    let Some((_, checked_family)) = fonts
        .families
        .iter()
        .find(|(name, _)| name.as_str() == family)
    else {
        issues.push(IconTokenIssue {
            code: codes::UNKNOWN_ICON_FAMILY.0,
            rule: "uhura/unknown-icon-family",
            source: family_attribute.map_or_else(
                || element_source.clone(),
                |attribute| attribute.source.clone(),
            ),
            message: format!("unknown checked icon family `{family}`"),
        });
        return;
    };

    let Some(name_attribute) = attributes.iter().find(|attribute| attribute.name == "name") else {
        // The UI catalogue reports the more direct missing-required-attribute
        // error. Avoid a second resource diagnostic during recovery.
        return;
    };
    let names = match &name_attribute.value {
        UiAttributeValue::Text { value } => BTreeSet::from([value.clone()]),
        UiAttributeValue::Expression { value } => {
            let mut names = BTreeSet::new();
            if !finite_icon_names(program, value, &mut names) || names.is_empty() {
                issues.push(IconTokenIssue {
                    code: codes::UNKNOWN_ICON.0,
                    rule: "uhura/unbounded-icon-name",
                    source: name_attribute.source.clone(),
                    message: "icon `name` must be a literal or a finite expression of checked glyph names"
                        .into(),
                });
                return;
            }
            names
        }
        UiAttributeValue::Event { .. } => return,
    };
    for name in names {
        if !checked_family
            .glyphs
            .keys()
            .any(|glyph| glyph.as_str() == name)
        {
            issues.push(IconTokenIssue {
                code: codes::UNKNOWN_ICON.0,
                rule: "uhura/unknown-icon",
                source: name_attribute.source.clone(),
                message: format!("unknown icon glyph `{name}` in family `{family}`"),
            });
        }
    }
}

/// Convert icon-registry findings into ordinary source diagnostics.
///
/// Hosts provide the same admitted path-to-file mapping used to compile the
/// program. A missing source is an internal coverage failure rather than a
/// user-authored unknown-icon error.
pub fn icon_token_diagnostics<'a>(
    program: &Program,
    fonts: &CheckedIconFonts,
    sources: impl IntoIterator<Item = (FileId, &'a str)>,
) -> Vec<Diagnostic> {
    let files = sources
        .into_iter()
        .map(|(file, path)| (path, file))
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = check_program_icon_tokens(program, fonts)
        .into_iter()
        .map(|issue| {
            let Some(file) = files.get(issue.source.path.as_str()).copied() else {
                return Diagnostic::new(
                    codes::ICON_SOURCE_COVERAGE.0,
                    codes::ICON_SOURCE_COVERAGE.1,
                    Severity::Error,
                    format!(
                        "checked icon source `{}` is absent from the admitted source inventory",
                        issue.source.path
                    ),
                    Span::new(FileId(0), 0, 0),
                );
            };
            Diagnostic::new(
                issue.code,
                issue.rule,
                Severity::Error,
                issue.message,
                Span::new(file, issue.source.start, issue.source.end),
            )
        })
        .collect::<Vec<_>>();
    diagnostics.sort_by(|left, right| {
        (
            left.span.file,
            left.span.start,
            left.span.end,
            left.code,
            left.rule,
            left.message.as_str(),
        )
            .cmp(&(
                right.span.file,
                right.span.start,
                right.span.end,
                right.code,
                right.rule,
                right.message.as_str(),
            ))
    });
    diagnostics
}

fn finite_icon_names(program: &Program, expression: &Expr, names: &mut BTreeSet<String>) -> bool {
    if names.len() > 256 {
        return false;
    }
    match expression {
        Expr::Literal {
            value: Value::Text(value),
        } => {
            names.insert(value.clone());
            true
        }
        Expr::Name { name } => match program.machine_program.constants.get(name) {
            Some(Value::Text(value)) => {
                names.insert(value.clone());
                true
            }
            _ => false,
        },
        Expr::If {
            then_value,
            else_value,
            ..
        } => {
            finite_icon_names(program, then_value, names)
                && finite_icon_names(program, else_value, names)
        }
        Expr::Match { arms, .. } => arms
            .iter()
            .all(|arm| finite_icon_names(program, &arm.value, names)),
        Expr::Let { value, .. } => finite_icon_names(program, value, names),
        _ => false,
    }
}

/// Validate the built-in Lucide family and every app-local family. The
/// registry is all-or-nothing so invalid font inputs cannot reach lowering.
pub fn load_icon_fonts(
    config: &IconsConfig,
    inputs: &BTreeMap<Ident, IconFontInput>,
) -> Result<CheckedIconFonts, Vec<IconFontIssue>> {
    let mut issues = Vec::new();
    let mut families = BTreeMap::new();

    match LUCIDE_CHECKED
        .get_or_init(|| {
            let mut issues = Vec::new();
            match check_family(
                "icons.lucide",
                Arc::<[u8]>::from(LUCIDE_FONT),
                LUCIDE_GLYPHS,
                &mut issues,
            ) {
                Some(family) => Ok(family),
                None => Err(issues),
            }
        })
        .clone()
    {
        Ok(lucide) => {
            families.insert(
                Ident::new("lucide").expect("built-in icon family is a valid identifier"),
                lucide,
            );
        }
        Err(mut lucide_issues) => issues.append(&mut lucide_issues),
    }

    for (name, declaration) in &config.families {
        let path = format!("icons.{name}");
        let Some(input) = inputs.get(name) else {
            issues.push(IconFontIssue {
                path,
                message: "declared icon family has no supplied font or glyph-map input".into(),
            });
            continue;
        };

        if input.font_path != declaration.font {
            issues.push(IconFontIssue {
                path: format!("icons.{name}.font"),
                message: format!(
                    "host supplied `{}` for manifest path `{}`",
                    input.font_path, declaration.font
                ),
            });
        }
        if input.glyphs_path != declaration.glyphs {
            issues.push(IconFontIssue {
                path: format!("icons.{name}.glyphs"),
                message: format!(
                    "host supplied `{}` for manifest path `{}`",
                    input.glyphs_path, declaration.glyphs
                ),
            });
        }

        if input.font_bytes.is_none() {
            issues.push(IconFontIssue {
                path: declaration.font.clone(),
                message: "icon font is missing or unreadable".into(),
            });
        }
        if input.glyphs_text.is_none() {
            issues.push(IconFontIssue {
                path: declaration.glyphs.clone(),
                message: "icon glyph map is missing or unreadable".into(),
            });
        }
        if let (Some(font), Some(glyphs)) = (&input.font_bytes, &input.glyphs_text)
            && let Some(checked) = check_family(&path, Arc::clone(font), glyphs, &mut issues)
        {
            families.insert(name.clone(), checked);
        }
    }

    for name in inputs.keys() {
        if !config.families.contains_key(name) {
            issues.push(IconFontIssue {
                path: format!("icons.{name}"),
                message: "host supplied an undeclared local icon family".into(),
            });
        }
    }

    if !families.contains_key(&config.default) {
        issues.push(IconFontIssue {
            path: "icons.default".into(),
            message: format!("icon family `{}` did not load successfully", config.default),
        });
    }

    if issues.is_empty() {
        Ok(CheckedIconFonts {
            default: config.default.clone(),
            families,
        })
    } else {
        Err(issues)
    }
}

fn check_family(
    path: &str,
    font: Arc<[u8]>,
    glyphs_text: &str,
    issues: &mut Vec<IconFontIssue>,
) -> Option<CheckedIconFamily> {
    let start = issues.len();
    let raw = if glyphs_text.len() > MAX_ICON_GLYPH_MAP_BYTES {
        issues.push(IconFontIssue {
            path: format!("{path}.glyphs"),
            message: format!(
                "glyph map is too large ({} bytes; maximum is {MAX_ICON_GLYPH_MAP_BYTES})",
                glyphs_text.len()
            ),
        });
        None
    } else {
        match serde_json::from_str::<UniqueGlyphMap>(glyphs_text) {
            Ok(raw) => Some(raw.0),
            Err(error) => {
                issues.push(IconFontIssue {
                    path: format!("{path}.glyphs"),
                    message: format!(
                        "expected a top-level JSON object mapping names to decimal codepoints: {error}"
                    ),
                });
                None
            }
        }
    };
    if raw.as_ref().is_some_and(BTreeMap::is_empty) {
        issues.push(IconFontIssue {
            path: format!("{path}.glyphs"),
            message: "glyph map must not be empty".into(),
        });
    }

    let mut glyphs = BTreeMap::new();
    for (name, codepoint) in raw.unwrap_or_default() {
        let glyph_path = format!("{path}.glyphs.{name}");
        let name = match Ident::new(&name) {
            Ok(name) => name,
            Err(error) => {
                issues.push(IconFontIssue {
                    path: glyph_path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if !is_private_use(codepoint) {
            issues.push(IconFontIssue {
                path: glyph_path,
                message: format!(
                    "codepoint {codepoint} (U+{codepoint:04X}) is outside Unicode private-use areas"
                ),
            });
            continue;
        }
        glyphs.insert(name, codepoint);
    }

    validate_font(path, &font, &glyphs, issues);

    if issues.len() != start {
        return None;
    }
    let glyph_json = serde_json::to_value(&glyphs)
        .expect("a validated icon glyph map always serializes as JSON");
    Some(CheckedIconFamily {
        font_hash: sha256_hex(&font),
        glyphs_hash: hash_json(&glyph_json),
        font,
        glyphs,
    })
}

fn validate_font(
    path: &str,
    font: &[u8],
    glyphs: &BTreeMap<Ident, u32>,
    issues: &mut Vec<IconFontIssue>,
) {
    let font_path = format!("{path}.font");
    if font.len() > MAX_ICON_FONT_BYTES {
        issues.push(IconFontIssue {
            path: font_path,
            message: format!(
                "WOFF2 font is too large ({} bytes; maximum is {MAX_ICON_FONT_BYTES})",
                font.len()
            ),
        });
        return;
    }
    if font.len() <= 4 || !font.starts_with(b"wOF2") {
        issues.push(IconFontIssue {
            path: font_path,
            message: "expected a non-empty WOFF2 font (missing `wOF2` signature)".into(),
        });
        return;
    }
    if font.len() < 20 {
        issues.push(IconFontIssue {
            path: font_path,
            message: "WOFF2 header is truncated".into(),
        });
        return;
    }
    if &font[4..8] == b"ttcf" {
        issues.push(IconFontIssue {
            path: font_path,
            message: "font collections are not supported; expected exactly one font face".into(),
        });
        return;
    }
    let declared_size = u32::from_be_bytes(font[16..20].try_into().expect("four-byte range"));
    if declared_size as usize > MAX_DECODED_ICON_FONT_BYTES {
        issues.push(IconFontIssue {
            path: font_path,
            message: format!(
                "decoded font would be too large ({declared_size} bytes; maximum is {MAX_DECODED_ICON_FONT_BYTES})"
            ),
        });
        return;
    }

    let decoded = std::panic::catch_unwind(|| {
        let mut brotli = decode_brotli_bounded;
        decompress_woff2_with_custom_brotli(font, &mut brotli)
    });
    let sfnt = match decoded {
        Ok(Ok(sfnt)) => sfnt,
        Ok(Err(error)) => {
            issues.push(IconFontIssue {
                path: font_path,
                message: format!("could not decode WOFF2 font: {error}"),
            });
            return;
        }
        Err(_) => {
            issues.push(IconFontIssue {
                path: font_path,
                message: "WOFF2 decoder rejected the malformed font".into(),
            });
            return;
        }
    };
    if sfnt.len() > MAX_DECODED_ICON_FONT_BYTES {
        issues.push(IconFontIssue {
            path: font_path,
            message: format!(
                "decoded font is too large ({} bytes; maximum is {MAX_DECODED_ICON_FONT_BYTES})",
                sfnt.len()
            ),
        });
        return;
    }

    if ttf_parser::fonts_in_collection(&sfnt).is_some() {
        issues.push(IconFontIssue {
            path: font_path,
            message: "font collections are not supported; expected exactly one font face".into(),
        });
        return;
    }
    let face = match Face::parse(&sfnt, 0) {
        Ok(face) => face,
        Err(error) => {
            issues.push(IconFontIssue {
                path: font_path,
                message: format!("decoded WOFF2 contains an invalid OpenType font: {error}"),
            });
            return;
        }
    };

    const PROHIBITED_TABLES: [&[u8; 4]; 19] = [
        b"SVG ", b"COLR", b"CPAL", b"CBDT", b"CBLC", b"EBDT", b"EBLC", b"EBSC", b"sbix", b"bdat",
        b"bloc", b"fvar", b"avar", b"gvar", b"HVAR", b"VVAR", b"MVAR", b"cvar", b"CFF2",
    ];
    let prohibited: Vec<String> = PROHIBITED_TABLES
        .iter()
        .filter(|tag| face.raw_face().table(Tag::from_bytes(tag)).is_some())
        .map(|tag| String::from_utf8_lossy(&tag[..]).trim_end().to_string())
        .collect();
    if !prohibited.is_empty() {
        issues.push(IconFontIssue {
            path: font_path.clone(),
            message: format!(
                "icon fonts cannot contain SVG, bitmap, color, or variable-font tables; found {}",
                prohibited.join(", ")
            ),
        });
    }

    if face.tables().cmap.is_none() {
        issues.push(IconFontIssue {
            path: font_path.clone(),
            message: "icon font has no usable Unicode `cmap` table".into(),
        });
        return;
    }
    if face.tables().glyf.is_none() && face.tables().cff.is_none() {
        issues.push(IconFontIssue {
            path: font_path,
            message: "icon font has no supported monochrome outline table (`glyf` or `CFF`)".into(),
        });
        return;
    }

    for (name, codepoint) in glyphs {
        let glyph = char::from_u32(*codepoint).and_then(|value| face.glyph_index(value));
        if glyph.is_none() {
            issues.push(IconFontIssue {
                path: format!("{path}.glyphs.{name}"),
                message: format!(
                    "codepoint U+{codepoint:04X} is absent from the font `cmap` or resolves to `.notdef`"
                ),
            });
        }
    }
}

fn decode_brotli_bounded(compressed: &[u8], expected: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    // `expected` is derived by wuff from the checked table directory. A
    // transformed `glyf` stream can temporarily expand each encoded point
    // into several output vectors, so its input ceiling is deliberately
    // tighter than the final decoded-font ceiling.
    if expected > MAX_ICON_FONT_TABLE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "WOFF2 table data would be too large ({expected} bytes; maximum is {MAX_ICON_FONT_TABLE_BYTES})"
            ),
        )
        .into());
    }
    let decoder = brotli::Decompressor::new(compressed, 4096);
    let mut decoded = Vec::with_capacity(expected);
    decoder
        .take(expected as u64 + 1)
        .read_to_end(&mut decoded)?;
    if decoded.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "WOFF2 Brotli output has {} bytes, but its table directory requires {expected}",
                decoded.len()
            ),
        )
        .into());
    }
    Ok(decoded)
}

struct UniqueGlyphMap(BTreeMap<String, u32>);

impl<'de> Deserialize<'de> for UniqueGlyphMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct GlyphMapVisitor;

        impl<'de> Visitor<'de> for GlyphMapVisitor {
            type Value = UniqueGlyphMap;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object mapping unique icon names to codepoints")
            }

            fn visit_map<M>(self, mut entries: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut glyphs = BTreeMap::new();
                while let Some((name, codepoint)) = entries.next_entry::<String, u32>()? {
                    if glyphs.insert(name.clone(), codepoint).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate glyph name `{name}`"
                        )));
                    }
                }
                Ok(UniqueGlyphMap(glyphs))
            }
        }

        deserializer.deserialize_map(GlyphMapVisitor)
    }
}

fn is_private_use(codepoint: u32) -> bool {
    (0xE000..=0xF8FF).contains(&codepoint)
        || (0xF0000..=0xFFFFD).contains(&codepoint)
        || (0x100000..=0x10FFFD).contains(&codepoint)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{MAX_ICON_FONT_TABLE_BYTES, decode_brotli_bounded};

    #[test]
    fn bounded_brotli_rejects_oversized_table_allocation() {
        let error = decode_brotli_bounded(&[], MAX_ICON_FONT_TABLE_BYTES + 1).unwrap_err();
        assert!(error.to_string().contains("table data would be too large"));
    }

    #[test]
    fn bounded_brotli_rejects_output_larger_than_the_directory() {
        let mut compressed = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
            writer.write_all(b"ab").unwrap();
        }

        let error = decode_brotli_bounded(&compressed, 1).unwrap_err();
        assert!(error.to_string().contains("has 2 bytes"));
    }
}
