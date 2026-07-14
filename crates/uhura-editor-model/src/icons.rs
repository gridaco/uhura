//! The Editor catalog's closed icon set as structured vector commands.
//!
//! Keeping geometry as data lets the browser create SVG nodes without Rust
//! manufacturing trusted markup strings. Coordinates are strings because SVG
//! accepts decimal lengths while the Uhura value model intentionally has no
//! floating-point type.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub view_box: [i64; 4],
    pub commands: Vec<IconCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconCommand {
    Path {
        d: String,
        paint: IconPaint,
    },
    Circle {
        cx: String,
        cy: String,
        r: String,
        paint: IconPaint,
    },
    Rect {
        x: String,
        y: String,
        width: String,
        height: String,
        rx: Option<String>,
        paint: IconPaint,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IconPaint {
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width: Option<String>,
    pub line_cap: Option<String>,
    pub line_join: Option<String>,
    pub opacity: Option<String>,
}

impl Icon {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "viewBox": self.view_box,
            "commands": self.commands.iter().map(IconCommand::to_json).collect::<Vec<_>>(),
        })
    }
}

impl IconCommand {
    fn to_json(&self) -> serde_json::Value {
        let (mut json, paint) = match self {
            IconCommand::Path { d, paint } => (
                serde_json::json!({
                    "kind": "path",
                    "d": d,
                }),
                paint,
            ),
            IconCommand::Circle { cx, cy, r, paint } => (
                serde_json::json!({
                    "kind": "circle",
                    "cx": cx,
                    "cy": cy,
                    "r": r,
                }),
                paint,
            ),
            IconCommand::Rect {
                x,
                y,
                width,
                height,
                rx,
                paint,
            } => {
                let mut json = serde_json::json!({
                    "kind": "rect",
                    "x": x,
                    "y": y,
                    "width": width,
                    "height": height,
                });
                if let Some(rx) = rx {
                    json["rx"] = serde_json::Value::String(rx.clone());
                }
                (json, paint)
            }
        };
        paint.apply_json(&mut json);
        json
    }
}

impl IconPaint {
    fn apply_json(&self, json: &mut serde_json::Value) {
        for (name, value) in [
            ("fill", self.fill.as_ref()),
            ("stroke", self.stroke.as_ref()),
            ("strokeWidth", self.stroke_width.as_ref()),
            ("lineCap", self.line_cap.as_ref()),
            ("lineJoin", self.line_join.as_ref()),
            ("opacity", self.opacity.as_ref()),
        ] {
            if let Some(value) = value {
                json[name] = serde_json::Value::String(value.clone());
            }
        }
    }
}

/// All built-in icon geometry, ordered by icon name.
pub fn table() -> BTreeMap<String, Icon> {
    [
        ("home", home()),
        ("search", search()),
        ("plus", plus()),
        ("reels", reels()),
        ("profile", profile()),
        ("heart", heart()),
        ("heart-filled", heart_filled()),
        ("comment", comment()),
        ("close", close()),
        ("back", back()),
        ("grid", grid()),
        ("layers", layers()),
        ("video-off", video_off()),
        ("progress", progress()),
        ("bookmark", bookmark()),
        ("bookmark-filled", bookmark_filled()),
        ("chevron-left", chevron_left()),
        ("chevron-right", chevron_right()),
    ]
    .into_iter()
    .map(|(name, icon)| (name.to_string(), icon))
    .collect()
}

/// Geometry used by the browser when a semantic icon name has no catalog
/// glyph. It is data, not pre-rendered markup.
pub fn fallback() -> Icon {
    icon(vec![circle("12", "12", "8", outline("1.8"))])
}

fn icon(commands: Vec<IconCommand>) -> Icon {
    Icon {
        view_box: [0, 0, 24, 24],
        commands,
    }
}

fn path(d: &str, paint: IconPaint) -> IconCommand {
    IconCommand::Path {
        d: d.to_string(),
        paint,
    }
}

fn circle(cx: &str, cy: &str, r: &str, paint: IconPaint) -> IconCommand {
    IconCommand::Circle {
        cx: cx.to_string(),
        cy: cy.to_string(),
        r: r.to_string(),
        paint,
    }
}

fn rect(
    x: &str,
    y: &str,
    width: &str,
    height: &str,
    rx: Option<&str>,
    paint: IconPaint,
) -> IconCommand {
    IconCommand::Rect {
        x: x.to_string(),
        y: y.to_string(),
        width: width.to_string(),
        height: height.to_string(),
        rx: rx.map(str::to_string),
        paint,
    }
}

fn outline(width: &str) -> IconPaint {
    IconPaint {
        fill: Some("none".to_string()),
        stroke: Some("currentColor".to_string()),
        stroke_width: Some(width.to_string()),
        ..IconPaint::default()
    }
}

fn stroke(width: &str) -> IconPaint {
    IconPaint {
        stroke: Some("currentColor".to_string()),
        stroke_width: Some(width.to_string()),
        ..IconPaint::default()
    }
}

fn filled() -> IconPaint {
    IconPaint {
        fill: Some("currentColor".to_string()),
        ..IconPaint::default()
    }
}

fn cap(mut paint: IconPaint) -> IconPaint {
    paint.line_cap = Some("round".to_string());
    paint
}

fn join(mut paint: IconPaint) -> IconPaint {
    paint.line_join = Some("round".to_string());
    paint
}

fn home() -> Icon {
    icon(vec![path(
        "M4 11 12 4l8 7v8a1 1 0 0 1-1 1h-4v-6h-6v6H5a1 1 0 0 1-1-1z",
        join(outline("1.8")),
    )])
}

fn search() -> Icon {
    icon(vec![
        circle("10.5", "10.5", "6", outline("1.8")),
        path("m15.5 15.5 5 5", cap(stroke("1.8"))),
    ])
}

fn plus() -> Icon {
    icon(vec![
        rect("3.5", "3.5", "17", "17", Some("4"), outline("1.8")),
        path("M12 8v8M8 12h8", cap(stroke("1.8"))),
    ])
}

fn reels() -> Icon {
    icon(vec![
        rect("3.5", "3.5", "17", "17", Some("4"), outline("1.8")),
        path("M3.5 8.5h17M8.5 3.5l3 5M14 3.5l3 5", stroke("1.6")),
        path("m10.5 12.2 4.4 2.6-4.4 2.6z", filled()),
    ])
}

fn profile() -> Icon {
    icon(vec![
        circle("12", "8.6", "3.6", outline("1.8")),
        path("M4.8 20a7.4 7.4 0 0 1 14.4 0", cap(outline("1.8"))),
    ])
}

fn heart() -> Icon {
    icon(vec![path(
        "M12 20.3 5 13.6a4.6 4.6 0 0 1 6.5-6.5l.5.5.5-.5a4.6 4.6 0 0 1 6.5 6.5z",
        join(outline("1.8")),
    )])
}

fn heart_filled() -> Icon {
    icon(vec![path(
        "M12 20.3 5 13.6a4.6 4.6 0 0 1 6.5-6.5l.5.5.5-.5a4.6 4.6 0 0 1 6.5 6.5z",
        filled(),
    )])
}

fn comment() -> Icon {
    icon(vec![path(
        "M20 11.6A8 8 0 1 0 7 17.9L4.5 20l.6-3.2A8 8 0 0 0 20 11.6z",
        join(outline("1.8")),
    )])
}

fn close() -> Icon {
    icon(vec![path("m6 6 12 12M18 6 6 18", cap(stroke("1.8")))])
}

fn back() -> Icon {
    icon(vec![path("M14.5 5 8 12l6.5 7", join(cap(outline("1.8"))))])
}

fn grid() -> Icon {
    icon(vec![path(
        "M4 4h16v16H4zM4 10.7h16M4 17.3h16M10.7 4v16M17.3 4v16",
        outline("1.5"),
    )])
}

fn layers() -> Icon {
    icon(vec![
        path("m12 4 8 4.5-8 4.5-8-4.5z", join(outline("1.7"))),
        path("m5.2 12.8 6.8 3.8 6.8-3.8", join(outline("1.7"))),
        path("m5.2 16.3 6.8 3.8 6.8-3.8", join(outline("1.7"))),
    ])
}

fn video_off() -> Icon {
    icon(vec![
        path(
            "M4 7.5A1.5 1.5 0 0 1 5.5 6h8A1.5 1.5 0 0 1 15 7.5v9a1.5 1.5 0 0 1-1.5 1.5h-8A1.5 1.5 0 0 1 4 16.5zM15 10.5l5-2.5v8l-5-2.5",
            join(outline("1.7")),
        ),
        path("m3.5 3.5 17 17", cap(stroke("1.7"))),
    ])
}

fn progress() -> Icon {
    let mut background = outline("1.8");
    background.opacity = Some("0.25".to_string());
    icon(vec![
        circle("12", "12", "7.5", background),
        path("M12 4.5a7.5 7.5 0 0 1 7.5 7.5", cap(outline("1.8"))),
    ])
}

fn bookmark() -> Icon {
    icon(vec![path(
        "M6.5 4.5h11a1 1 0 0 1 1 1v15L12 16.2l-6.5 4.3v-15a1 1 0 0 1 1-1z",
        join(outline("1.8")),
    )])
}

fn bookmark_filled() -> Icon {
    icon(vec![path(
        "M6.5 4.5h11a1 1 0 0 1 1 1v15L12 16.2l-6.5 4.3v-15a1 1 0 0 1 1-1z",
        filled(),
    )])
}

fn chevron_left() -> Icon {
    icon(vec![path("m14.5 5-6.5 7 6.5 7", join(cap(outline("1.8"))))])
}

fn chevron_right() -> Icon {
    icon(vec![path("m9.5 5 6.5 7-6.5 7", join(cap(outline("1.8"))))])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_table_is_structured_and_complete() {
        let icons = table();
        assert_eq!(icons.len(), 18);
        assert_eq!(icons["home"].view_box, [0, 0, 24, 24]);
        let json = icons["search"].to_json();
        assert_eq!(json["commands"][0]["kind"], "circle");
        assert_eq!(json["commands"][1]["kind"], "path");
        assert!(!json.to_string().contains('<'));
    }
}
