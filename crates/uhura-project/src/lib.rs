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
        let kind_label = frame_kind_label(frame.kind);
        let row_key = format!("{kind_label} {}", frame.subject);
        match rows.iter_mut().find(|(key, _, _)| *key == row_key) {
            Some((_, _, list)) => list.push(frame),
            None => rows.push((row_key, kind_label.to_string(), vec![frame])),
        }
    }

    let mut body = String::new();
    let mut navigator = String::new();
    let mut frame_index = 0usize;
    for (row_index, (row_key, kind_label, list)) in rows.iter().enumerate() {
        let row_id = format!("preview-row-{row_index}");
        let _ = writeln!(
            body,
            "<section id=\"{row_id}\" class=\"row row-{kind_label}\" data-preview-row>\n<h2 class=\"row-title\">{}</h2>\n<div class=\"row-frames\">",
            esc(row_key)
        );
        let subject = list.first().map_or("", |frame| frame.subject.as_str());
        let _ = writeln!(
            navigator,
            "<section class=\"navigator-group\" data-navigator-group data-search=\"{}\">\n\
             <button class=\"navigator-row\" type=\"button\" data-row-target=\"{row_id}\" aria-controls=\"{row_id}\">\
             <span class=\"navigator-kind\" data-kind=\"{kind_label}\" aria-hidden=\"true\"></span>\
             <span class=\"navigator-row-title\">{}</span><span class=\"navigator-count\">{}</span></button>\n\
             <div class=\"navigator-frames\">",
            esc(row_key),
            esc(subject),
            list.len(),
        );
        for frame in list {
            let frame_id = format!("preview-frame-{frame_index}");
            body.push_str(&render_frame(frame, &frame_id));
            let marker = if frame.derived {
                "<span class=\"navigator-derived\" title=\"Replay-derived\">D</span>"
            } else if frame.is_default {
                "<span class=\"navigator-default\" title=\"Default preview\"></span>"
            } else {
                ""
            };
            let _ = writeln!(
                navigator,
                "<button class=\"navigator-frame\" type=\"button\" data-frame-target=\"{frame_id}\" data-search=\"{} {} {}\" aria-controls=\"{frame_id}\" aria-pressed=\"false\">\
                 <span class=\"navigator-frame-icon\" aria-hidden=\"true\"></span>\
                 <span class=\"navigator-frame-title\">{}</span>{marker}</button>",
                esc(kind_label),
                esc(&frame.subject),
                esc(&frame.example),
                esc(&frame.example),
            );
            frame_index += 1;
        }
        body.push_str("</div>\n</section>\n");
        navigator.push_str("</div>\n</section>\n");
    }

    let derived_count = frames.iter().filter(|frame| frame.derived).count();
    let pinned_count = frames.iter().filter(|frame| frame.pinned).count();
    let default_count = frames.iter().filter(|frame| frame.is_default).count();

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
{chrome_css}
:root {{
{asset_vars}}}
</style>
</head>
<body class="uhura-canvas">
<nav id="canvas-navigator" aria-label="Preview navigator">
  <div class="panel-heading"><div><span class="panel-eyebrow">{app}</span><h1>Previews</h1></div><span>{row_count} groups</span></div>
  <label class="navigator-search">
    <svg aria-hidden="true" viewBox="0 0 16 16"><circle cx="7" cy="7" r="4.25"/><path d="m10.25 10.25 3 3"/></svg>
    <input id="navigator-search" type="search" placeholder="Search previews" autocomplete="off" aria-label="Search previews">
  </label>
  <div id="navigator-results" class="navigator-results">
{navigator}
    <p id="navigator-empty" hidden>No matching previews</p>
  </div>
  <footer class="navigator-help">Wheel to pan <span>·</span> Pinch to zoom <span>·</span> Space for Hand</footer>
</nav>

<main id="canvas-stage">
  <div class="ruler-corner" aria-hidden="true"></div>
  <canvas id="ruler-x" class="canvas-ruler ruler-x" aria-hidden="true"></canvas>
  <canvas id="ruler-y" class="canvas-ruler ruler-y" aria-hidden="true"></canvas>
  <div id="viewport" role="region" aria-label="Canvas viewport" tabindex="0">
    <div class="canvas-tools" role="group" aria-label="Canvas tools">
    <button id="tool-cursor" class="canvas-tool" type="button" aria-label="Cursor tool" aria-keyshortcuts="V" aria-pressed="true" title="Cursor (V)">
      <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3 2.25v11.5l3.05-3.05 2.15 3.72 2.08-1.2-2.13-3.69 4.15-1.12L3 2.25Z"/></svg>
    </button>
    <button id="tool-hand" class="canvas-tool" type="button" aria-label="Hand tool" aria-keyshortcuts="H" aria-pressed="false" title="Hand (H or hold Space)">
      <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M5.15 7.4V3.75a1 1 0 0 1 2 0V6.5h.35V2.75a1 1 0 0 1 2 0V6.5h.35V3.75a1 1 0 0 1 2 0V7h.35V5.25a1 1 0 0 1 2 0v3.8c0 3.05-1.8 5.2-4.9 5.2H8.2c-1.5 0-2.45-.65-3.35-1.8L2.3 9.2a1.13 1.13 0 0 1 1.75-1.42l1.1 1.2V7.4Z"/></svg>
    </button>
    <span class="tool-divider" aria-hidden="true"></span>
    <button id="zoom-out" class="canvas-tool" type="button" aria-label="Zoom out" title="Zoom out">
      <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3.5 8h9"/></svg>
    </button>
    <button id="canvas-zoom" class="canvas-zoom" type="button" aria-label="Reset zoom to 100%" title="Reset zoom to 100%">100%</button>
    <button id="zoom-in" class="canvas-tool" type="button" aria-label="Zoom in" title="Zoom in">
      <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3.5 8h9M8 3.5v9"/></svg>
    </button>
    <span class="tool-divider" aria-hidden="true"></span>
    <button id="focus-selection" class="canvas-tool" type="button" aria-label="Center selected preview" title="Center selected preview" disabled>
      <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M5.5 2.5h-3v3M10.5 2.5h3v3M13.5 10.5v3h-3M5.5 13.5h-3v-3"/></svg>
    </button>
  </div>
    <div id="board">
{body}
    </div>
  </div>
</main>

<aside id="canvas-inspector" aria-label="Preview details">
  <div class="panel-heading"><div><span class="panel-eyebrow">Inspect</span><h2>Preview details</h2></div><span class="inspector-actions"><!-- uhura-editor-actions --></span></div>
  <section id="inspector-overview" class="inspector-section">
    <div class="inspector-hero"><span class="inspector-hero-icon" aria-hidden="true">U</span><div><strong>{app}</strong><span>Deterministic Canvas</span></div></div>
    <dl class="inspector-grid">
      <div><dt>Previews</dt><dd>{frame_count}</dd></div>
      <div><dt>Groups</dt><dd>{row_count}</dd></div>
      <div><dt>Defaults</dt><dd>{default_count}</dd></div>
      <div><dt>Derived</dt><dd>{derived_count}</dd></div>
      <div><dt>Pinned</dt><dd>{pinned_count}</dd></div>
      <div><dt>Assets</dt><dd>{asset_count}</dd></div>
    </dl>
    <div class="inspector-callout"><strong>Read-only projection</strong><p>Edit the <code>.uhura</code> sources and restart Editor to regenerate these snapshots.</p></div>
  </section>
  <section id="inspector-selection" class="inspector-section" hidden>
    <div class="selection-heading"><div><span id="selection-kind" class="selection-kind">Page</span><h2 id="selection-name">Preview</h2></div><button id="clear-selection" class="inspector-icon-button" type="button" aria-label="Clear preview selection" title="Clear selection"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="m4 4 8 8m0-8-8 8"/></svg></button></div>
    <dl class="property-list">
      <div><dt>Subject</dt><dd id="selection-subject"></dd></div>
      <div><dt>Example</dt><dd id="selection-example"></dd></div>
      <div><dt>Size</dt><dd id="selection-size"></dd></div>
      <div><dt>Origin</dt><dd id="selection-origin"></dd></div>
      <div id="selection-from-row" hidden><dt>From</dt><dd id="selection-from"></dd></div>
      <div><dt>Status</dt><dd id="selection-status"></dd></div>
    </dl>
    <div id="selection-note-block" class="inspector-block" hidden><h3>Note</h3><p id="selection-note"></p></div>
    <div class="inspector-block"><h3>Declared interactions</h3><ul id="selection-interactions" class="interaction-list"></ul><p id="selection-no-interactions" class="inspector-muted">No interactions declared in this snapshot.</p></div>
  </section>
</aside>
<script>
{chrome_js}
</script>
</body>
</html>
"#,
        app = esc(app),
        base_css = BASE_CSS,
        chrome_css = CHROME_CSS,
        stylesheet = stylesheet,
        asset_vars = asset_vars,
        frame_count = frames.len(),
        row_count = rows.len(),
        default_count = default_count,
        derived_count = derived_count,
        pinned_count = pinned_count,
        asset_count = used_assets.len(),
        navigator = navigator,
        body = body,
        chrome_js = CHROME_JS,
    )
}

fn render_frame(frame: &PreviewFrame, frame_id: &str) -> String {
    let mut badges = String::new();
    if frame.is_default {
        badges.push_str("<span class=\"badge badge-default\">default</span>");
    }
    let provenance = frame_provenance(frame);
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
    let note_data = frame.note.as_deref().unwrap_or_default();
    let from_data = frame.from.as_deref().unwrap_or_default();
    format!(
        "<figure id=\"{frame_id}\" class=\"frame\" data-frame data-kind=\"{kind}\" data-subject=\"{subject}\" data-example=\"{example}\" data-provenance=\"{prov}\" data-default=\"{is_default}\" data-pinned=\"{pinned}\" data-derived=\"{derived}\" data-in-flight=\"{in_flight}\" data-from=\"{from_data}\" data-preview-note=\"{note_data}\" role=\"button\" tabindex=\"0\" aria-pressed=\"false\" aria-labelledby=\"{frame_id}-caption\">\n<div class=\"{frame_class}\">{content}</div>\n\
         <figcaption id=\"{frame_id}-caption\"><span class=\"caption-title\">{subject} / {example}</span>{badges}\
         <span class=\"caption-prov\">{prov}</span>{note}</figcaption>\n</figure>\n",
        frame_id = frame_id,
        kind = frame_kind_label(frame.kind),
        frame_class = frame_class,
        content = content,
        subject = esc(&frame.subject),
        example = esc(&frame.example),
        is_default = frame.is_default,
        pinned = frame.pinned,
        derived = frame.derived,
        in_flight = frame.in_flight,
        from_data = esc(from_data),
        note_data = esc(note_data),
        badges = badges,
        prov = esc(&provenance),
        note = note,
    )
}

fn frame_kind_label(kind: FrameKind) -> &'static str {
    match kind {
        FrameKind::Page => "page",
        FrameKind::Surface => "surface",
        FrameKind::Component => "component",
    }
}

fn frame_provenance(frame: &PreviewFrame) -> String {
    match (&frame.from, frame.derived) {
        (Some(parent), true) => format!("from {parent} → events…"),
        (Some(parent), false) => format!("from {parent}"),
        (None, true) => "derived".to_string(),
        (None, false) if frame.pinned => "pinned".to_string(),
        (None, false) => "checked example".to_string(),
    }
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
        "video" => {
            // Canvas is a static, network-free projection: render the poster
            // rather than a media element that could load or autoplay. The
            // semantic label keeps the inert preview meaningful to AT.
            if let Some(poster) = prop_text("poster") {
                let _ = write!(
                    attrs,
                    " style=\"background-image: var(--asset-{})\"",
                    esc(&poster)
                );
            }
            if let Some(label) = prop_text("label") {
                let _ = write!(attrs, " role=\"img\" aria-label=\"{}\"", esc(&label));
            }
            attrs.push_str(" data-video-preview=\"poster\"");
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
    for (name, value) in &node.props {
        // A Canvas never embeds or fetches playable media. Video `poster`
        // remains an ordinary image asset; its `src` is Play-only.
        if node.element.as_str() == "video" && name.as_str() == "src" {
            continue;
        }
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
body.uhura-canvas { font: 14px/1.45 -apple-system, "Segoe UI", sans-serif; background: #09090a; color: #e8e8ec; overflow: hidden; }

/* semantic element bases — layout/aesthetics stay authored (§10) */
.uh-view { display: block; min-inline-size: 0; }
.uh-scroll { overflow-y: auto; overflow-x: hidden; min-block-size: 0; }
.uh-scroll[data-direction="horizontal"] { overflow-x: auto; overflow-y: hidden; }
.uh-text { margin: 0; overflow-wrap: anywhere; }
.uh-image { background-size: cover; background-position: center; background-color: #d9d9de; display: block; }
.uh-video { display: block; inline-size: 100%; background: #111 center / cover no-repeat; object-fit: cover; }
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
.canvas-tools { display: inline-flex; gap: 2px; padding: 2px; border: 1px solid #303139; border-radius: 7px; background: #111217; }
.canvas-tool svg { inline-size: 15px; block-size: 15px; fill: currentColor; }
#canvas-zoom { min-inline-size: 42px; color: #9a9aa5; font: 12px/1 ui-monospace, SFMono-Regular, Menlo, monospace; text-align: end; }
.canvas-actions { display: inline-flex; align-items: center; }
#viewport { position: absolute; inset: 0; touch-action: none; cursor: default; }
#viewport[data-tool="hand"] { cursor: grab; }
#viewport.panning { cursor: grabbing; user-select: none; }
#viewport:focus-visible { outline: 2px solid #7aa7ff; outline-offset: -2px; }
#board { position: absolute; transform-origin: 0 0; padding: 72px 48px 48px; }
#board > [data-preview-row] { margin-block-end: 56px; }
[data-preview-row] > .row-title { font-size: 13px; text-transform: uppercase; letter-spacing: 0.1em; color: #8a8a96; margin-block-end: 14px; }
[data-preview-row] > .row-frames { display: flex; align-items: flex-start; gap: 32px; }
[data-preview-row] > .row-frames > [data-frame] { flex: none; }
#board > [data-preview-row] > .row-frames > [data-frame] > .shell { background: #fff; color: #16181c; border-radius: 24px; overflow: hidden; position: relative; }
[data-frame] > .shell.device { inline-size: 390px; block-size: 844px; }
[data-frame] > .shell.device .screen-root { block-size: 100%; overflow: hidden; }
[data-frame] > .shell.device .screen-root > * { block-size: 100%; }
[data-frame] > .shell.sheet { inline-size: 390px; block-size: 560px; border-radius: 16px; }
[data-frame] > .shell.sheet .fragment-root { block-size: 100%; }
[data-frame] > .shell.sheet .fragment-root > * { block-size: 100%; }
[data-frame] > .shell.component { inline-size: 390px; border-radius: 12px; background:
  #fff radial-gradient(circle, #d8d8de 1px, transparent 1px);
  background-size: 16px 16px; }
[data-frame] > figcaption { margin-block-start: 10px; max-inline-size: 390px; }
[data-frame] > figcaption > .caption-title { font-weight: 600; margin-inline-end: 8px; }
[data-frame] > figcaption > .badge { font-size: 11px; border-radius: 999px; padding: 1px 8px; margin-inline-end: 6px; }
[data-frame] > figcaption > .badge-default { background: #2e5c34; color: #b7edbe; }
[data-frame] > figcaption > .badge-pinned { background: #4a4430; color: #e6d9a3; }
[data-frame] > figcaption > .badge-in-flight { background: #2f3f52; color: #a3c6e6; }
[data-frame] > figcaption > .caption-prov { color: #8a8a96; font-size: 12px; }
[data-frame] > figcaption > .caption-note { color: #a9a9b4; font-size: 12px; font-style: italic; margin-block-start: 2px; }
"#;

const CHROME_CSS: &str = include_str!("canvas-chrome.css");

/// Read-only editor chrome. It never reads or mutates Uhura state (§8.3):
/// navigation, selection, rulers, and camera controls operate on the emitted DOM.
const CHROME_JS: &str = include_str!("canvas-chrome.js");

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use uhura_base::Ident;
    use uhura_core::view::{Node, VValue};

    use super::{Asset, FrameContent, FrameKind, PreviewFrame, render_canvas};

    fn frame(kind: FrameKind, subject: &str, example: &str) -> PreviewFrame {
        PreviewFrame {
            kind,
            subject: subject.to_string(),
            example: example.to_string(),
            is_default: true,
            pinned: false,
            derived: true,
            in_flight: 2,
            from: Some("base <state>".to_string()),
            note: Some("A checked & safe note".to_string()),
            content: FrameContent::Fragment(Node {
                key: "root".to_string(),
                element: Ident::new("view").expect("valid element"),
                class: None,
                props: BTreeMap::new(),
                children: Vec::new(),
                on: Vec::new(),
            }),
        }
    }

    #[test]
    fn canvas_exposes_read_only_editor_chrome_and_tools_without_fit() {
        let canvas = render_canvas("Demo", &[], "", &BTreeMap::new());

        assert!(canvas.contains("id=\"canvas-navigator\""));
        assert!(canvas.contains("id=\"canvas-inspector\""));
        assert!(!canvas.contains("class=\"canvas-bar\""));
        assert!(!canvas.contains("id=\"toggle-navigator\""));
        assert!(!canvas.contains("id=\"toggle-inspector\""));
        assert!(canvas.contains("id=\"navigator-search\""));
        assert!(canvas.contains("id=\"ruler-x\""));
        assert!(canvas.contains("id=\"ruler-y\""));
        assert!(canvas.contains("Read-only projection"));
        assert!(canvas.contains("role=\"group\" aria-label=\"Canvas tools\""));
        assert!(canvas.contains("id=\"tool-cursor\""));
        assert!(canvas.contains("aria-keyshortcuts=\"V\" aria-pressed=\"true\""));
        assert!(canvas.contains("id=\"tool-hand\""));
        assert!(canvas.contains("aria-keyshortcuts=\"H\" aria-pressed=\"false\""));
        assert!(canvas.contains("Wheel to pan"));
        assert!(canvas.contains("Pinch to zoom"));
        assert!(canvas.contains("Space for Hand"));
        assert_eq!(canvas.matches("<!-- uhura-editor-actions -->").count(), 1);
        assert!(canvas.contains("class=\"inspector-actions\""));
        assert!(!canvas.contains("id=\"fit\""));
        assert!(!canvas.contains("dblclick"));
        assert!(!canvas.contains("const fit"));
        assert!(!canvas.contains("--canvas-shadow"));
        assert!(canvas.contains("[data-frame] > .shell { border:"));
        assert!(canvas.contains("box-shadow: none;"));
    }

    #[test]
    fn canvas_chrome_supports_navigation_selection_rulers_and_gestures() {
        let canvas = render_canvas("Demo", &[], "", &BTreeMap::new());

        assert!(canvas.contains("if (event.ctrlKey || event.metaKey)"));
        assert!(canvas.contains("const WHEEL_ZOOM_SENSITIVITY = 0.01"));
        assert!(canvas.contains("Math.exp(exponent)"));
        assert!(canvas.contains("x -= event.deltaX * unit"));
        assert!(canvas.contains("const shouldPan = event.button === 1"));
        assert!(canvas.contains("if (touches.size === 2)"));
        assert!(canvas.contains("event.code === \"Space\""));
        assert!(canvas.contains("event.code === \"KeyH\""));
        assert!(canvas.contains("event.code === \"KeyV\""));
        assert!(canvas.contains("pointercancel"));
        assert!(canvas.contains("lostpointercapture"));
        assert!(canvas.contains("const drawRulers"));
        assert!(canvas.contains("const selectFrame"));
        assert!(canvas.contains(
            "selectFrame(document.getElementById(frameButton.dataset.frameTarget), true)"
        ));
        assert!(!canvas.contains("selectFrame(frame, true)"));
        assert!(!canvas.contains("board.addEventListener(\"focusin\""));
        assert!(canvas.contains("navigatorSearch.addEventListener(\"input\""));
        assert!(canvas.contains("uhura.editor.ui-visible"));
        assert!(canvas.contains("event.code === \"Backslash\""));
        assert!(canvas.contains("event.metaKey || event.ctrlKey"));
        assert!(canvas.contains("document.body.classList.toggle(\"ui-hidden\""));
        assert!(!canvas.contains("PANEL_KEYS"));
        assert!(!canvas.contains("navigator-closed"));
        assert!(!canvas.contains("inspector-closed"));
    }

    #[test]
    fn frames_have_stable_navigation_ids_and_escaped_inspector_metadata() {
        let frames = [
            frame(FrameKind::Page, "Feed & home", "default \"wide\""),
            frame(FrameKind::Page, "Feed & home", "after like"),
            frame(FrameKind::Component, "Post card", "compact"),
        ];
        let canvas = render_canvas("Demo", &frames, "", &BTreeMap::new());

        assert!(canvas.contains("id=\"preview-row-0\""));
        assert!(canvas.contains("id=\"preview-row-1\""));
        assert!(canvas.contains("id=\"preview-frame-0\""));
        assert!(canvas.contains("id=\"preview-frame-2\""));
        assert!(canvas.contains("data-frame-target=\"preview-frame-1\""));
        assert!(canvas.contains("data-subject=\"Feed &amp; home\""));
        assert!(canvas.contains("data-example=\"default &quot;wide&quot;\""));
        assert!(canvas.contains("data-from=\"base &lt;state&gt;\""));
        assert!(canvas.contains("data-preview-note=\"A checked &amp; safe note\""));
        assert!(canvas.contains("aria-labelledby=\"preview-frame-0-caption\""));
        assert!(canvas.contains("role=\"button\" tabindex=\"0\" aria-pressed=\"false\""));
        assert_eq!(canvas.matches("class=\"navigator-group\"").count(), 2);
    }

    #[test]
    fn canvas_renders_a_video_as_an_inert_poster_without_embedding_its_source() {
        let props = BTreeMap::from([
            (
                Ident::new("src").expect("valid prop"),
                VValue::Image("clip-aurora".to_string()),
            ),
            (
                Ident::new("poster").expect("valid prop"),
                VValue::Image("poster-aurora".to_string()),
            ),
            (
                Ident::new("label").expect("valid prop"),
                VValue::Plain("Aurora <above> the fjord".to_string()),
            ),
            (
                Ident::new("autoplay").expect("valid prop"),
                VValue::Bool(true),
            ),
        ]);
        let frame = PreviewFrame {
            kind: FrameKind::Component,
            subject: "Video".to_string(),
            example: "poster".to_string(),
            is_default: true,
            pinned: false,
            derived: false,
            in_flight: 0,
            from: None,
            note: None,
            content: FrameContent::Fragment(Node {
                key: "video".to_string(),
                element: Ident::new("video").expect("valid element"),
                class: Some("hero".to_string()),
                props,
                children: Vec::new(),
                on: Vec::new(),
            }),
        };
        let assets = BTreeMap::from([(
            "poster-aurora".to_string(),
            Asset {
                data_uri: "data:image/jpeg;base64,poster".to_string(),
                alt: "Aurora poster".to_string(),
            },
        )]);

        let canvas = render_canvas("Demo", &[frame], "", &assets);

        assert!(canvas.contains("--asset-poster-aurora: url(\"data:image/jpeg;base64,poster\")"));
        assert!(!canvas.contains("--asset-clip-aurora"));
        assert!(!canvas.contains("<video"));
        assert!(canvas.contains("class=\"uh-video hero\""));
        assert!(canvas.contains("background-image: var(--asset-poster-aurora)"));
        assert!(canvas.contains("role=\"img\" aria-label=\"Aurora &lt;above&gt; the fjord\""));
        assert!(canvas.contains("data-video-preview=\"poster\""));
    }
}
