//! `uhura editor [path] [--port <n>] [--out=<dir>]` — host the deterministic,
//! read-only Canvas beside the real Play shell. Saved project changes rebuild
//! a complete Canvas generation; invalid candidates leave the last-good
//! previews visible with diagnostics.

use std::collections::BTreeMap;
use std::process::ExitCode;

use crate::CommonArgs;

pub fn run(common: &CommonArgs, port: u16, out_dir: Option<&str>) -> ExitCode {
    super::dev::run_with_editor(common, port, out_dir)
}

/// Keep the build-only Canvas artifact generic while making the hosted mode
/// explicit about the current editor's capabilities.
pub(super) fn editor_html(canvas: &str, active_generation: u64, active_build_id: &str) -> String {
    let host_head = format!(
        "<meta name=\"uhura-editor-host\" content=\"{active_generation}\">\n\
         <meta name=\"uhura-editor-build\" content=\"{active_build_id}\">\n{}",
        r#"<style>
.inspector-actions .canvas-play,
.inspector-actions .canvas-play:visited { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-block-size: 28px; padding: 0 10px; border: 1px solid #078ce9; border-radius: 7px; color: #fff; background: var(--canvas-accent, #0d99ff); box-shadow: 0 1px 2px rgb(13 90 145 / 20%); font-size: 11px; font-weight: 650; line-height: 1; text-decoration: none; white-space: nowrap; cursor: pointer; }
.inspector-actions .canvas-play:hover { border-color: #087ecf; color: #fff; background: #0b87e3; }
.inspector-actions .canvas-play:active { border-color: #0874bf; background: #0878cb; }
.inspector-actions .canvas-play:focus-visible { outline: 2px solid var(--canvas-accent, #0d99ff); outline-offset: 2px; }
.inspector-actions .canvas-play svg { inline-size: 11px; block-size: 11px; fill: currentColor; }
</style>"#
    );
    canvas
        .replace(
            " — uhura canvas</title>",
            " — uhura editor (read-only)</title>",
        )
        .replace(
            "<!-- uhura-editor-actions -->",
            r#"<a class="canvas-play" href="/play" aria-label="Open the interactive prototype">
    <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M5 3.25v9.5L12.5 8z"/></svg>
    <span>Play</span>
  </a>"#,
        )
        .replace(
            "Edit the <code>.uhura</code> sources and restart Editor to regenerate these snapshots.",
            "Saved project changes rebuild these snapshots automatically.",
        )
        .replace(
            "</head>",
            &format!("{host_head}\n</head>"),
        )
}

/// A cold-invalid project still receives the complete Editor shell so it can
/// show diagnostics and converge on the first valid Canvas generation.
pub(super) fn cold_html() -> String {
    let canvas = uhura_project::render_canvas("Uhura", &[], "", &BTreeMap::new());
    editor_html(&canvas, 0, "")
}

/// Add host navigation only when the shell is reached through the editor.
/// `uhura play` serves the same source document without replacing the marker.
pub(super) fn play_html(shell: &str) -> String {
    shell.replace(
        "<!-- uhura-editor-navigation -->",
        r#"<a class="uh-editor-link" href="/" aria-label="Return to Uhura Editor">
          <svg aria-hidden="true" viewBox="0 0 16 16"><path d="m9.5 4-4 4 4 4"/></svg>
          Editor
        </a>"#,
    )
}

#[cfg(test)]
mod tests {
    use super::{cold_html, editor_html, play_html};

    #[test]
    fn hosted_canvas_identifies_the_read_only_editor() {
        let canvas = r#"<head><title>Demo — uhura canvas</title></head>
<span class="inspector-actions"><!-- uhura-editor-actions --></span>"#;
        let editor = editor_html(canvas, 7, "abc123");

        assert!(editor.contains("Demo — uhura editor (read-only)</title>"));
        assert!(editor.contains("name=\"uhura-editor-host\" content=\"7\""));
        assert!(editor.contains("name=\"uhura-editor-build\" content=\"abc123\""));
        assert!(editor.contains("href=\"/play\""));
        assert!(editor.contains("Open the interactive prototype"));
        assert!(editor.contains("<span class=\"inspector-actions\"><a class=\"canvas-play\""));
        assert!(!editor.contains("class=\"canvas-actions\""));
        assert!(!editor.contains("uhura-editor-actions"));
        assert!(!editor.contains("uhura canvas"));
    }

    #[test]
    fn cold_invalid_editor_is_live_and_has_no_active_generation() {
        let editor = cold_html();

        assert!(editor.contains("name=\"uhura-editor-host\" content=\"0\""));
        assert!(editor.contains("name=\"uhura-editor-build\" content=\"\""));
        assert!(editor.contains("id=\"viewport\""));
        assert!(editor.contains("href=\"/play\""));
    }

    #[test]
    fn editor_hosted_play_shell_links_back_without_changing_the_source_shell() {
        let shell = "<div><!-- uhura-editor-navigation --></div>";
        let hosted = play_html(shell);

        assert!(hosted.contains("href=\"/\""));
        assert!(hosted.contains("Return to Uhura Editor"));
        assert!(!hosted.contains("uhura-editor-navigation"));
        assert!(shell.contains("<!-- uhura-editor-navigation -->"));
    }
}
