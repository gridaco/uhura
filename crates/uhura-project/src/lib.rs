//! uhura-project: static projection — resolved example previews → HTML
//! frames → one self-contained canvas.html. Pure string production; all
//! file I/O lives in uhura-cli (design §8.3).
//!
//! The mapping is plain HTML per §8.3: real elements with a correct a11y
//! tree inside `inert` frames, authored classes passed through beside a
//! stable `uh-<element>` base class, assets inlined exactly once as
//! `--asset-<id>` custom properties, and prerendered `data-note` hover
//! chrome ("would emit like-toggled {…}"). Zero transitions, zero
//! commands, zero network — structurally: nothing here can emit.

pub mod icons;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use uhura_core::view::{Descriptor, Node, Snapshot, VValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Page,
    Surface,
    Component,
}

pub struct PreviewFrame {
    pub kind: FrameKind,
    pub subject: String,
    pub example: String,
    pub is_default: bool,
    pub pinned: bool,
    /// Resolved by timeline replay — the caption shows the derivation.
    pub derived: bool,
    /// Commands unsettled at the end of a derived timeline (§6.2).
    pub in_flight: usize,
    pub from: Option<String>,
    pub note: Option<String>,
    pub content: FrameContent,
}

pub enum FrameContent {
    /// A page example: the full snapshot (surface stack included).
    Snapshot(Snapshot),
    /// A component or standalone surface example.
    Fragment(Node),
}

/// One embeddable asset. `data_uri` is the complete `data:image/…;base64,`
/// string; `alt` comes from the manifest (required, §8.3).
pub struct Asset {
    pub data_uri: String,
    pub alt: String,
}

/// Renders the whole board. Assets are embedded once each — only the ids
/// the frames actually reference; unknown ids get the duotone-SVG
/// fallback (§8.3).
pub fn render_canvas(
    app: &str,
    frames: &[PreviewFrame],
    stylesheet: &str,
    assets: &BTreeMap<String, Asset>,
) -> String {
    let mut used_assets = BTreeSet::new();
    for frame in frames {
        match &frame.content {
            FrameContent::Snapshot(s) => {
                collect_assets(&s.page.root, &mut used_assets);
                for surface in &s.surfaces {
                    collect_assets(&surface.root, &mut used_assets);
                }
            }
            FrameContent::Fragment(node) => collect_assets(node, &mut used_assets),
        }
    }

    let mut asset_vars = String::new();
    for id in &used_assets {
        let uri = assets
            .get(id)
            .map(|a| a.data_uri.clone())
            .unwrap_or_else(|| fallback_asset(id));
        let _ = writeln!(asset_vars, "  --asset-{id}: url(\"{uri}\");");
    }

    // ── group frames into board rows (§6.3) ────────────────────────────
    let mut rows: Vec<(String, String, Vec<&PreviewFrame>)> = Vec::new();
    for frame in frames {
        let kind_label = match frame.kind {
            FrameKind::Page => "page",
            FrameKind::Surface => "surface",
            FrameKind::Component => "component",
        };
        let row_key = format!("{kind_label} {}", frame.subject);
        match rows.iter_mut().find(|(key, _, _)| *key == row_key) {
            Some((_, _, list)) => list.push(frame),
            None => rows.push((row_key, kind_label.to_string(), vec![frame])),
        }
    }

    let mut body = String::new();
    for (row_key, kind_label, list) in &rows {
        let _ = writeln!(
            body,
            "<section class=\"row row-{kind_label}\">\n<h2 class=\"row-title\">{}</h2>\n<div class=\"row-frames\">",
            esc(row_key)
        );
        for frame in list {
            body.push_str(&render_frame(frame));
        }
        body.push_str("</div>\n</section>\n");
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{app} — uhura canvas</title>
<style>
{base_css}
/* ── compiled app stylesheet (theme.css + <style> blocks, §4.5) ── */
{stylesheet}
</style>
<style>
:root {{
{asset_vars}}}
</style>
</head>
<body>
<header class="canvas-bar">
  <strong>{app}</strong><span class="canvas-sub">uhura canvas — {frame_count} previews</span>
  <span class="canvas-hint">drag to pan · wheel to zoom · double-click to fit</span>
  <button id="fit" type="button">fit</button>
</header>
<div id="viewport"><div id="board">
{body}
</div></div>
<script>
{chrome_js}
</script>
</body>
</html>
"#,
        app = esc(app),
        base_css = BASE_CSS,
        stylesheet = stylesheet,
        asset_vars = asset_vars,
        frame_count = frames.len(),
        body = body,
        chrome_js = CHROME_JS,
    )
}

fn render_frame(frame: &PreviewFrame) -> String {
    let mut badges = String::new();
    if frame.is_default {
        badges.push_str("<span class=\"badge badge-default\">default</span>");
    }
    let provenance = match (&frame.from, frame.derived) {
        (Some(parent), true) => format!("from {parent} → events…"),
        (Some(parent), false) => format!("from {parent}"),
        (None, true) => "derived".to_string(),
        (None, false) => "pinned".to_string(),
    };
    if frame.pinned {
        badges.push_str("<span class=\"badge badge-pinned\">pinned</span>");
    }
    if frame.in_flight > 0 {
        let _ = write!(
            badges,
            "<span class=\"badge badge-in-flight\">{} in flight</span>",
            frame.in_flight
        );
    }

    let (frame_class, content) = match &frame.content {
        FrameContent::Snapshot(snapshot) => {
            let mut html = format!(
                "<div class=\"screen-root\" inert>{}</div>",
                node_html(&snapshot.page.root, false)
            );
            for surface in &snapshot.surfaces {
                let _ = write!(
                    html,
                    "<div class=\"uh-surface-overlay\"><div class=\"uh-scrim\"></div>\
                     <div class=\"uh-surface uh-modality-{}\" role=\"dialog\" aria-modal=\"true\" inert>{}</div></div>",
                    esc(&surface.modality),
                    node_html(&surface.root, false)
                );
            }
            ("shell device", html)
        }
        FrameContent::Fragment(node) => {
            let class = match frame.kind {
                FrameKind::Surface => "shell sheet",
                _ => "shell component",
            };
            (
                class,
                format!(
                    "<div class=\"fragment-root\" inert>{}</div>",
                    node_html(node, false)
                ),
            )
        }
    };

    let note = frame
        .note
        .as_ref()
        .map(|n| format!("<p class=\"caption-note\">{}</p>", esc(n)))
        .unwrap_or_default();
    format!(
        "<figure class=\"frame\">\n<div class=\"{frame_class}\">{content}</div>\n\
         <figcaption><span class=\"caption-title\">{subject} / {example}</span>{badges}\
         <span class=\"caption-prov\">{prov}</span>{note}</figcaption>\n</figure>\n",
        frame_class = frame_class,
        content = content,
        subject = esc(&frame.subject),
        example = esc(&frame.example),
        badges = badges,
        prov = esc(&provenance),
        note = note,
    )
}

// ── V → HTML (§8.3) ─────────────────────────────────────────────────────

fn node_html(node: &Node, parent_is_list: bool) -> String {
    let element = node.element.as_str();
    let mut classes = format!("uh-{element}");
    if let Some(authored) = &node.class {
        classes.push(' ');
        classes.push_str(authored);
    }

    let mut attrs = String::new();
    let _ = write!(
        attrs,
        " class=\"{}\" data-key=\"{}\"",
        esc(&classes),
        esc(&node.key)
    );
    if parent_is_list {
        attrs.push_str(" role=\"listitem\"");
    }
    if let Some(note) = descriptor_note(&node.on) {
        let _ = write!(attrs, " data-note=\"{}\"", esc(&note));
    }

    let prop_text = |name: &str| -> Option<String> {
        node.props.iter().find_map(|(k, v)| {
            if k.as_str() != name {
                return None;
            }
            match v {
                VValue::Text(s) | VValue::Plain(s) => Some(s.clone()),
                VValue::Bool(b) => Some(b.to_string()),
                VValue::Int(i) => Some(i.to_string()),
                VValue::Image(a) => Some(a.clone()),
            }
        })
    };
    let prop_bool = |name: &str| -> bool {
        node.props
            .iter()
            .any(|(k, v)| k.as_str() == name && matches!(v, VValue::Bool(true)))
    };

    let is_list = prop_text("role").as_deref() == Some("list");
    let children: String = node
        .children
        .iter()
        .map(|child| node_html(child, is_list))
        .collect();

    match element {
        "view" => {
            match prop_text("role").as_deref() {
                Some("list") => attrs.push_str(" role=\"list\""),
                Some("navigation") => attrs.push_str(" role=\"navigation\""),
                Some("tablist") => attrs.push_str(" role=\"tablist\""),
                _ => {}
            }
            format!("<div{attrs}>{children}</div>")
        }
        "scroll" => {
            let direction = prop_text("direction").unwrap_or_else(|| "vertical".into());
            format!(
                "<div{attrs} data-direction=\"{}\">{children}</div>",
                esc(&direction)
            )
        }
        "pager" => {
            if let Some(label) = prop_text("label") {
                let _ = write!(attrs, " role=\"group\" aria-label=\"{}\"", esc(&label));
            }
            let dots = if prop_text("indicator").as_deref() == Some("dots") {
                let count = node.children.len();
                let mut dots = String::from("<div class=\"uh-dots\" aria-hidden=\"true\">");
                for i in 0..count {
                    dots.push_str(if i == 0 {
                        "<span class=\"uh-dot on\"></span>"
                    } else {
                        "<span class=\"uh-dot\"></span>"
                    });
                }
                dots.push_str("</div>");
                dots
            } else {
                String::new()
            };
            format!("<div{attrs}><div class=\"uh-track\">{children}</div>{dots}</div>")
        }
        "text" => {
            let content = prop_text("content").unwrap_or_default();
            format!("<p{attrs}>{}</p>", esc(&content))
        }
        "image" => {
            let asset = prop_text("src").unwrap_or_default();
            let _ = write!(
                attrs,
                " style=\"background-image: var(--asset-{})\"",
                esc(&asset)
            );
            if prop_bool("decorative") {
                attrs.push_str(" aria-hidden=\"true\"");
            } else if let Some(alt) = prop_text("alt") {
                let _ = write!(attrs, " role=\"img\" aria-label=\"{}\"", esc(&alt));
            }
            format!("<div{attrs}></div>")
        }
        "icon" => {
            let name = prop_text("name").unwrap_or_default();
            let glyph = icons::glyph(&name).unwrap_or(
                r#"<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"/>"#,
            );
            format!(
                "<span{attrs} aria-hidden=\"true\"><svg viewBox=\"0 0 24 24\" width=\"24\" height=\"24\">{glyph}</svg></span>"
            )
        }
        "button" => {
            if prop_bool("disabled") {
                attrs.push_str(" disabled");
            }
            if prop_bool("busy") {
                attrs.push_str(" aria-busy=\"true\"");
            }
            if let Some((_, VValue::Bool(pressed))) =
                node.props.iter().find(|(k, _)| k.as_str() == "pressed")
            {
                let _ = write!(attrs, " aria-pressed=\"{pressed}\"");
            }
            if prop_bool("current") {
                attrs.push_str(" aria-current=\"true\"");
            }
            if let Some(label) = prop_text("label") {
                let _ = write!(attrs, " aria-label=\"{}\"", esc(&label));
            }
            format!("<button type=\"button\"{attrs}>{children}</button>")
        }
        "text-field" => {
            let mut input_attrs = String::new();
            if let Some(value) = prop_text("value") {
                let _ = write!(input_attrs, " value=\"{}\"", esc(&value));
            }
            if let Some(placeholder) = prop_text("placeholder") {
                let _ = write!(input_attrs, " placeholder=\"{}\"", esc(&placeholder));
            }
            if let Some(label) = prop_text("label") {
                let _ = write!(input_attrs, " aria-label=\"{}\"", esc(&label));
            }
            if prop_bool("disabled") {
                input_attrs.push_str(" disabled");
            }
            format!("<div{attrs}><input type=\"text\"{input_attrs}></div>")
        }
        "region" => {
            if let Some(label) = prop_text("label") {
                let _ = write!(attrs, " aria-label=\"{}\"", esc(&label));
            }
            format!("<div{attrs} role=\"button\" tabindex=\"0\">{children}</div>")
        }
        // Honest labeled placeholder for anything unsupported (§8.2).
        other => format!(
            "<div{attrs} data-unsupported=\"{}\">{children}</div>",
            esc(other)
        ),
    }
}

fn descriptor_note(on: &[Descriptor]) -> Option<String> {
    if on.is_empty() {
        return None;
    }
    let parts: Vec<String> = on
        .iter()
        .map(|d| format!("would emit {} {}", d.emit, d.payload))
        .collect();
    Some(parts.join(" · "))
}

fn collect_assets(node: &Node, out: &mut BTreeSet<String>) {
    for value in node.props.values() {
        if let VValue::Image(asset) = value {
            out.insert(asset.clone());
        }
    }
    for child in &node.children {
        collect_assets(child, out);
    }
}

/// Duotone-SVG fallback for a missing asset id (§8.3) — deterministic hue
/// from the id bytes.
fn fallback_asset(id: &str) -> String {
    let hue: u32 = id
        .bytes()
        .fold(0u32, |h, b| (h.wrapping_mul(31) + b as u32) % 360);
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'>\
         <rect width='64' height='64' fill='hsl({hue},35%,72%)'/>\
         <path d='M0 64 64 0v64z' fill='hsl({hue},40%,48%)'/></svg>"
    );
    format!("data:image/svg+xml;utf8,{}", svg.replace('#', "%23"))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ── canvas chrome ───────────────────────────────────────────────────────

/// Base element affordances + board chrome. Authored CSS is embedded after
/// this block, so the app always wins on shared selectors.
const BASE_CSS: &str = r#"
* { box-sizing: border-box; margin: 0; }
html, body { height: 100%; }
body { font: 14px/1.45 -apple-system, "Segoe UI", sans-serif; background: #14151a; color: #e8e8ec; overflow: hidden; }

/* semantic element bases — layout/aesthetics stay authored (§10) */
.uh-view { display: block; min-inline-size: 0; }
.uh-scroll { overflow-y: auto; overflow-x: hidden; min-block-size: 0; }
.uh-scroll[data-direction="horizontal"] { overflow-x: auto; overflow-y: hidden; }
.uh-text { margin: 0; overflow-wrap: anywhere; }
.uh-image { background-size: cover; background-position: center; background-color: #d9d9de; display: block; }
.uh-icon { display: inline-flex; }
.uh-icon svg { display: block; }
button.uh-button { appearance: none; background: none; border: 0; padding: 6px; font: inherit; color: inherit; display: inline-flex; align-items: center; gap: 6px; border-radius: 8px; }
button.uh-button[disabled] { opacity: 0.35; }
button.uh-button[aria-busy="true"] { opacity: 0.6; }
.uh-text-field input { font: inherit; inline-size: 100%; border: 1px solid #d5d5da; border-radius: 999px; padding: 8px 14px; background: #fff; color: #222; }
.uh-region { display: block; }
.uh-pager .uh-track { display: flex; overflow-x: auto; scroll-snap-type: x mandatory; }
.uh-pager .uh-track > * { flex: 0 0 100%; scroll-snap-align: center; }
.uh-pager { position: relative; }
.uh-dots { position: absolute; inset-block-end: 10px; inset-inline: 0; display: flex; justify-content: center; gap: 5px; }
.uh-dot { inline-size: 6px; block-size: 6px; border-radius: 999px; background: rgba(255,255,255,0.55); }
.uh-dot.on { background: #fff; }

/* surface overlay inside a device frame */
.uh-surface-overlay { position: absolute; inset: 0; display: flex; flex-direction: column; justify-content: flex-end; }
.uh-scrim { position: absolute; inset: 0; background: rgba(0,0,0,0.4); }
.uh-surface { position: relative; background: #fff; border-radius: 16px 16px 0 0; max-block-size: 72%; block-size: 72%; box-shadow: 0 -8px 32px rgba(0,0,0,0.35); }

/* hover chrome: prerendered notes (§8.3) */
[data-note] { position: relative; }
[data-note]:hover::after { content: attr(data-note); position: absolute; inset-block-start: 100%; inset-inline-start: 0; z-index: 30; background: #111; color: #9fe0a8; font: 11px/1.4 ui-monospace, monospace; padding: 4px 8px; border-radius: 6px; white-space: pre; max-inline-size: 320px; overflow: hidden; text-overflow: ellipsis; pointer-events: none; }

/* board */
.canvas-bar { position: fixed; inset-block-start: 0; inset-inline: 0; z-index: 50; display: flex; gap: 12px; align-items: baseline; padding: 10px 16px; background: rgba(20,21,26,0.9); backdrop-filter: blur(6px); border-block-end: 1px solid #2a2b33; }
.canvas-sub { color: #9a9aa5; }
.canvas-hint { margin-inline-start: auto; color: #6d6d78; font-size: 12px; }
.canvas-bar button { font: inherit; background: #2a2b33; color: #e8e8ec; border: 0; border-radius: 6px; padding: 3px 12px; }
#viewport { position: absolute; inset: 0; cursor: grab; }
#viewport.panning { cursor: grabbing; }
#board { position: absolute; transform-origin: 0 0; padding: 72px 48px 48px; }
.row { margin-block-end: 56px; }
.row-title { font-size: 13px; text-transform: uppercase; letter-spacing: 0.1em; color: #8a8a96; margin-block-end: 14px; }
.row-frames { display: flex; align-items: flex-start; gap: 32px; }
.frame { flex: none; }
.shell { background: #fff; color: #16181c; border-radius: 24px; box-shadow: 0 12px 48px rgba(0,0,0,0.5), 0 0 0 1px #2e2f38; overflow: hidden; position: relative; }
.shell.device { inline-size: 390px; block-size: 844px; }
.shell.device .screen-root { block-size: 100%; overflow: hidden; }
.shell.device .screen-root > * { block-size: 100%; }
.shell.sheet { inline-size: 390px; block-size: 560px; border-radius: 16px; }
.shell.sheet .fragment-root { block-size: 100%; }
.shell.sheet .fragment-root > * { block-size: 100%; }
.shell.component { inline-size: 390px; border-radius: 12px; background:
  #fff radial-gradient(circle, #d8d8de 1px, transparent 1px);
  background-size: 16px 16px; }
figcaption { margin-block-start: 10px; max-inline-size: 390px; }
.caption-title { font-weight: 600; margin-inline-end: 8px; }
.badge { font-size: 11px; border-radius: 999px; padding: 1px 8px; margin-inline-end: 6px; }
.badge-default { background: #2e5c34; color: #b7edbe; }
.badge-pinned { background: #4a4430; color: #e6d9a3; }
.badge-in-flight { background: #2f3f52; color: #a3c6e6; }
.caption-prov { color: #8a8a96; font-size: 12px; }
.caption-note { color: #a9a9b4; font-size: 12px; font-style: italic; margin-block-start: 2px; }
"#;

/// Pan/zoom/fit. Never reads V or Uhura data (§8.3) — pure chrome.
const CHROME_JS: &str = r#"
(() => {
  const viewport = document.getElementById("viewport");
  const board = document.getElementById("board");
  let x = 0, y = 0, scale = 1;
  const apply = () => { board.style.transform = `translate(${x}px, ${y}px) scale(${scale})`; };
  const fit = () => {
    const rect = board.getBoundingClientRect();
    const w = rect.width / scale, h = rect.height / scale;
    scale = Math.min(viewport.clientWidth / w, viewport.clientHeight / h, 1) * 0.96;
    x = (viewport.clientWidth - w * scale) / 2;
    y = 24;
    apply();
  };
  let panning = false, px = 0, py = 0;
  viewport.addEventListener("pointerdown", (e) => {
    panning = true; px = e.clientX - x; py = e.clientY - y;
    viewport.classList.add("panning"); viewport.setPointerCapture(e.pointerId);
  });
  viewport.addEventListener("pointermove", (e) => {
    if (!panning) return;
    x = e.clientX - px; y = e.clientY - py; apply();
  });
  viewport.addEventListener("pointerup", () => { panning = false; viewport.classList.remove("panning"); });
  viewport.addEventListener("wheel", (e) => {
    e.preventDefault();
    const factor = Math.exp(-e.deltaY * 0.0015);
    const next = Math.min(Math.max(scale * factor, 0.08), 3);
    x = e.clientX - (e.clientX - x) * (next / scale);
    y = e.clientY - (e.clientY - y) * (next / scale);
    scale = next; apply();
  }, { passive: false });
  viewport.addEventListener("dblclick", fit);
  document.getElementById("fit").addEventListener("click", fit);
  fit();
})();
"#;
