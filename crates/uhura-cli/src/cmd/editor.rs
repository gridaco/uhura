//! `uhura editor [path] [--port <n>] [--out=<dir>]` — build the deterministic
//! Canvas and host it as the read-only editor placeholder. The editor and the
//! real Play shell share one server: `/` is the Canvas and `/play` is the live
//! prototype. The Canvas is held in memory; restart to rebuild its previews.

use std::process::ExitCode;

use crate::CommonArgs;

pub fn run(common: &CommonArgs, port: u16, out_dir: Option<&str>) -> ExitCode {
    let code = super::project::run_as(common, out_dir, "uhura editor");
    if code != ExitCode::SUCCESS {
        return code;
    }

    let canvas_path = super::project::output_path(out_dir);
    let canvas = match std::fs::read_to_string(&canvas_path) {
        Ok(canvas) => editor_html(&canvas),
        Err(error) => {
            eprintln!("uhura editor: {}: {error}", canvas_path.display());
            return ExitCode::from(2);
        }
    };
    super::dev::run_with_editor(common, port, canvas.into_bytes())
}

/// Keep the build-only Canvas artifact generic while making the hosted mode
/// explicit about the current editor's capabilities.
fn editor_html(canvas: &str) -> String {
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
            "</head>",
            r#"<style>
.inspector-actions .canvas-play,
.inspector-actions .canvas-play:visited { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-block-size: 28px; padding: 0 10px; border: 1px solid #078ce9; border-radius: 7px; color: #fff; background: var(--canvas-accent, #0d99ff); box-shadow: 0 1px 2px rgb(13 90 145 / 20%); font-size: 11px; font-weight: 650; line-height: 1; text-decoration: none; white-space: nowrap; cursor: pointer; }
.inspector-actions .canvas-play:hover { border-color: #087ecf; color: #fff; background: #0b87e3; }
.inspector-actions .canvas-play:active { border-color: #0874bf; background: #0878cb; }
.inspector-actions .canvas-play:focus-visible { outline: 2px solid var(--canvas-accent, #0d99ff); outline-offset: 2px; }
.inspector-actions .canvas-play svg { inline-size: 11px; block-size: 11px; fill: currentColor; }
</style>
</head>"#,
        )
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
    use super::{editor_html, play_html};

    #[test]
    fn hosted_canvas_identifies_the_read_only_editor() {
        let canvas = r#"<head><title>Demo — uhura canvas</title></head>
<span class="inspector-actions"><!-- uhura-editor-actions --></span>"#;
        let editor = editor_html(canvas);

        assert!(editor.contains("Demo — uhura editor (read-only)</title>"));
        assert!(editor.contains("href=\"/play\""));
        assert!(editor.contains("Open the interactive prototype"));
        assert!(editor.contains("<span class=\"inspector-actions\"><a class=\"canvas-play\""));
        assert!(!editor.contains("class=\"canvas-actions\""));
        assert!(!editor.contains("uhura-editor-actions"));
        assert!(!editor.contains("uhura canvas"));
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
