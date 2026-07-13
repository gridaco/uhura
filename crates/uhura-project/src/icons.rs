//! The base catalog's closed icon set as inline SVG glyphs (24×24,
//! stroke/fill from `currentColor`). Hand-authored: the static canvas is
//! self-contained by contract (§8.3) — no icon font, no fetches.

/// The inner SVG markup for a catalog icon name, if we ship a glyph.
pub fn glyph(name: &str) -> Option<&'static str> {
    let body = match name {
        "home" => {
            r#"<path d="M4 11 12 4l8 7v8a1 1 0 0 1-1 1h-4v-6h-6v6H5a1 1 0 0 1-1-1z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#
        }
        "search" => {
            r#"<circle cx="10.5" cy="10.5" r="6" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="m15.5 15.5 5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#
        }
        "plus" => {
            r#"<rect x="3.5" y="3.5" width="17" height="17" rx="4" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M12 8v8M8 12h8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#
        }
        "reels" => {
            r#"<rect x="3.5" y="3.5" width="17" height="17" rx="4" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M3.5 8.5h17M8.5 3.5l3 5M14 3.5l3 5" stroke="currentColor" stroke-width="1.6"/><path d="m10.5 12.2 4.4 2.6-4.4 2.6z" fill="currentColor"/>"#
        }
        "profile" => {
            r#"<circle cx="12" cy="8.6" r="3.6" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M4.8 20a7.4 7.4 0 0 1 14.4 0" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#
        }
        "heart" => {
            r#"<path d="M12 20.3 5 13.6a4.6 4.6 0 0 1 6.5-6.5l.5.5.5-.5a4.6 4.6 0 0 1 6.5 6.5z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#
        }
        "heart-filled" => {
            r#"<path d="M12 20.3 5 13.6a4.6 4.6 0 0 1 6.5-6.5l.5.5.5-.5a4.6 4.6 0 0 1 6.5 6.5z" fill="currentColor"/>"#
        }
        "comment" => {
            r#"<path d="M20 11.6A8 8 0 1 0 7 17.9L4.5 20l.6-3.2A8 8 0 0 0 20 11.6z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#
        }
        "close" => {
            r#"<path d="m6 6 12 12M18 6 6 18" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#
        }
        "back" => {
            r#"<path d="M14.5 5 8 12l6.5 7" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"#
        }
        "grid" => {
            r#"<path d="M4 4h16v16H4zM4 10.7h16M4 17.3h16M10.7 4v16M17.3 4v16" fill="none" stroke="currentColor" stroke-width="1.5"/>"#
        }
        "layers" => {
            r#"<path d="m12 4 8 4.5-8 4.5-8-4.5z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><path d="m5.2 12.8 6.8 3.8 6.8-3.8" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><path d="m5.2 16.3 6.8 3.8 6.8-3.8" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/>"#
        }
        "video-off" => {
            r#"<path d="M4 7.5A1.5 1.5 0 0 1 5.5 6h8A1.5 1.5 0 0 1 15 7.5v9a1.5 1.5 0 0 1-1.5 1.5h-8A1.5 1.5 0 0 1 4 16.5zM15 10.5l5-2.5v8l-5-2.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><path d="m3.5 3.5 17 17" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>"#
        }
        "progress" => {
            r#"<circle cx="12" cy="12" r="7.5" fill="none" stroke="currentColor" stroke-width="1.8" opacity="0.25"/><path d="M12 4.5a7.5 7.5 0 0 1 7.5 7.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#
        }
        _ => return None,
    };
    Some(body)
}
