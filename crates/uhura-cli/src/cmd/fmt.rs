//! `uhura fmt [path] [--check]` — canonical-format every source module.

use std::process::ExitCode;

use crate::CommonArgs;
use crate::fsio::SourceFile;

struct FormatFile {
    file: u32,
    logical: String,
    source: SourceFile,
}

pub fn run(common: &CommonArgs, check_only: bool) -> ExitCode {
    let snapshot = uhura_project::capture_project_snapshot(&common.root);
    let resolved = match uhura_project::resolve_project(&snapshot) {
        Ok(resolved) => resolved,
        Err(rejection) => {
            eprint!(
                "{}",
                uhura_base::render_text(&rejection.diagnostics, &rejection.source_map)
            );
            return ExitCode::from(1);
        }
    };
    let logical_by_path = resolved
        .manifest()
        .modules
        .iter()
        .map(|(logical, path)| (path.as_str(), logical.as_str()))
        .chain(
            resolved
                .manifest()
                .evidence
                .iter()
                .map(|(logical, path)| (path.as_str(), logical.as_str())),
        )
        .collect::<std::collections::BTreeMap<_, _>>();
    let files = resolved
        .root_authored_sources()
        .map(|source| FormatFile {
            file: source.file.0,
            logical: logical_by_path
                .get(source.path.as_str())
                .expect("every root authored source has one resolved logical module")
                .to_string(),
            source: SourceFile {
                rel_path: source.path.clone(),
                abs_path: common.root.join(&source.path),
                text: source.text.clone(),
            },
        })
        .collect::<Vec<_>>();

    format_files(
        &files,
        &resolved.manifest().project.package_id().to_string(),
        check_only,
    )
}

fn format_files(files: &[FormatFile], package: &str, check_only: bool) -> ExitCode {
    let mut broken = false;
    let mut drifted = Vec::new();

    for admitted in files {
        let file = &admitted.source;
        let identity = uhura_syntax::SourceIdentity::new(
            admitted.file,
            package,
            &admitted.logical,
            &file.rel_path,
        );
        let parsed = uhura_syntax::parse(identity, &file.text);
        if !parsed.diagnostics.is_empty() {
            for diagnostic in parsed.diagnostics {
                let (code, rule) = diagnostic.kind.diagnostic_identity();
                eprintln!(
                    "{}:{}..{}: {code} {rule} {}",
                    file.rel_path, diagnostic.span.start, diagnostic.span.end, diagnostic.message
                );
            }
            broken = true;
            continue;
        }
        let formatted = match uhura_syntax::format(&parsed.module) {
            Ok(formatted) => formatted,
            Err(error) => {
                eprintln!("{}: UH2002 {error}", file.rel_path);
                broken = true;
                continue;
            }
        };
        if formatted == file.text {
            continue;
        }
        drifted.push(file.rel_path.clone());
        if !check_only && let Err(error) = std::fs::write(&file.abs_path, formatted) {
            eprintln!("uhura fmt: cannot write {}: {error}", file.rel_path);
            return ExitCode::from(2);
        }
    }

    if broken {
        return ExitCode::from(1);
    }
    if check_only && !drifted.is_empty() {
        for path in drifted {
            println!("would reformat: {path}");
        }
        return ExitCode::from(1);
    }
    if !check_only {
        for path in drifted {
            println!("reformatted: {path}");
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    fn project_root(label: &str) -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "uhura-cli-fmt-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_manifest(root: &Path) {
        std::fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.format"
version = 1
language = "0.4"

[modules]
main = "main.uhura"
"#,
        )
        .unwrap();
    }

    fn common(root: &Path) -> CommonArgs {
        CommonArgs {
            root: root.to_path_buf(),
            format_json: false,
            deny_warnings: false,
            emit_ir: false,
        }
    }

    #[test]
    fn manifest_selected_sources_use_the_formatter() {
        let root = project_root("canonical");
        write_manifest(&root);
        std::fs::write(
            root.join("main.uhura"),
            "pub machine Counter{events{Increment,}outcomes{commit Accepted,}state{count:Int=0,}observe{count,}on Increment{count=count+1;Accepted}}\n",
        )
        .unwrap();

        assert_eq!(run(&common(&root), true), ExitCode::from(1));
        assert_eq!(run(&common(&root), false), ExitCode::SUCCESS);
        assert_eq!(run(&common(&root), true), ExitCode::SUCCESS);
        let formatted = std::fs::read_to_string(root.join("main.uhura")).unwrap();
        assert!(formatted.starts_with("pub machine Counter {\n"));
        assert!(formatted.contains("count = count + 1;"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn formatter_refuses_comments_without_changing_source() {
        let root = project_root("comments");
        write_manifest(&root);
        let source = "/// Counter\npub machine Counter {}\n";
        std::fs::write(root.join("main.uhura"), source).unwrap();

        assert_eq!(run(&common(&root), false), ExitCode::from(1));
        assert_eq!(
            std::fs::read_to_string(root.join("main.uhura")).unwrap(),
            source
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn web_app_formats_discovered_authored_sources_but_never_generated_sources() {
        let root = project_root("web-app");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("components")).unwrap();
        std::fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.format-web"
version = 1
language = "0.4"

[framework]
profile = "web-app"
version = 1
machine = "crate::program::App"
location = "crate::routing::Location"

[modules]
program = "machine.uhura"
routing = "routing.uhura"
"#,
        )
        .unwrap();
        std::fs::write(root.join("routing.uhura"), "pub enum Location{Home}\n").unwrap();
        std::fs::write(
            root.join("machine.uhura"),
            "use uhura::web_router::Router;use crate::framework::routes::APPLICATION_ROUTES;use crate::routing::Location;pub machine App{port router=Router<Location>{routes:APPLICATION_ROUTES};events{Refresh}outcomes{commit Accepted}state{location:Option<Location> =None}observe{location}on Refresh{Accepted}on router.Changed(next){location=Some(next);Accepted}}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("app/page.uhura"),
            "use uhura::ui;use crate::program::App;pub ui HomePage for App(view){<main>Home</main>}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("components/status-card.uhura"),
            "use uhura::ui;pub ui StatusCard(label:Text){<p>{label}</p>}\n",
        )
        .unwrap();

        assert_eq!(run(&common(&root), true), ExitCode::from(1));
        assert_eq!(run(&common(&root), false), ExitCode::SUCCESS);
        assert_eq!(run(&common(&root), true), ExitCode::SUCCESS);
        assert!(
            std::fs::read_to_string(root.join("app/page.uhura"))
                .unwrap()
                .contains("pub ui HomePage for App(view) {\n")
        );
        assert!(
            std::fs::read_to_string(root.join("components/status-card.uhura"))
                .unwrap()
                .contains("pub ui StatusCard(label: Text) {\n")
        );
        assert!(!root.join(".uhura/generated/web-app/routes.uhura").exists());
        assert!(
            !root
                .join(".uhura/generated/web-app/application.uhura")
                .exists()
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
