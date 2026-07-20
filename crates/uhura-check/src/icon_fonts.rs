//! Checked icon-family registries. The checker owns names and codepoints;
//! renderers receive only validated, content-addressed font resources.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{self, Read};
use std::sync::{Arc, OnceLock};

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use ttf_parser::{Face, Tag};
use uhura_base::{Ident, hash_json, sha256_hex};
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
