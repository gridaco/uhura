//! `uhura fmt [path] [--check]` — canonical-format every source module.

use std::collections::BTreeMap;
use std::process::ExitCode;

use uhura_base::FileId;
use uhura_check::project_manifest::{LoadedProjectManifest, load_project_manifest};
use uhura_syntax::{SourceId, format as format_v03, parse as parse_v03};

use crate::CommonArgs;
use crate::fsio::SourceFile;

pub fn run(common: &CommonArgs, check_only: bool) -> ExitCode {
    if let Ok(retired) = crate::fsio::walk_retired_sources(&common.root)
        && let Some(path) = retired.first()
    {
        eprintln!(
            "uhura fmt: retired `.relay` source `{}`; rename it to `.uhura`",
            path.display()
        );
        return ExitCode::from(2);
    }
    let files = match crate::fsio::walk_sources(&common.root) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("uhura fmt: {}: {error}", common.root.display());
            return ExitCode::from(2);
        }
    };
    if files.is_empty() {
        eprintln!(
            "uhura fmt: no .uhura sources under {}",
            common.root.display()
        );
        return ExitCode::from(2);
    }

    let manifest_text = match std::fs::read_to_string(common.root.join("uhura.toml")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            eprintln!(
                "uhura fmt: cannot read {}: {error}",
                common.root.join("uhura.toml").display()
            );
            return ExitCode::from(2);
        }
    };
    let manifest = match load_project_manifest(&manifest_text) {
        Ok(manifest) => manifest,
        Err(issues) => {
            for issue in issues {
                if issue.path.is_empty() {
                    eprintln!("uhura.toml: UH2001 {}", issue.message);
                } else {
                    eprintln!("uhura.toml: UH2001 {}: {}", issue.path, issue.message);
                }
            }
            return ExitCode::from(1);
        }
    };

    match manifest {
        LoadedProjectManifest::Legacy03(_) => format_v03_files(&files, check_only),
        LoadedProjectManifest::V04(manifest) => format_v04_files(&files, &manifest, check_only),
    }
}

fn format_v03_files(files: &[SourceFile], check_only: bool) -> ExitCode {
    let mut broken = false;
    let mut drifted = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let parsed = parse_v03(
            SourceId::new(FileId(index as u32), file.rel_path.clone()),
            &file.text,
        );
        if !parsed.diagnostics.is_empty() {
            for diagnostic in parsed.diagnostics {
                eprintln!(
                    "{}:{}..{}: R1001 {}",
                    file.rel_path, diagnostic.span.start, diagnostic.span.end, diagnostic.message
                );
            }
            broken = true;
            continue;
        }
        let Some(module) = parsed.module else {
            eprintln!("{}: R1001 parser produced no module", file.rel_path);
            broken = true;
            continue;
        };
        let formatted = format_v03(&module);
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

fn format_v04_files(
    files: &[SourceFile],
    manifest: &uhura_check::project_manifest::ProjectManifest,
    check_only: bool,
) -> ExitCode {
    let discovered = files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.rel_path.as_str(), (index, file)))
        .collect::<BTreeMap<_, _>>();
    let mut broken = false;
    let mut drifted = Vec::new();

    for (logical, physical) in &manifest.modules {
        let Some((index, file)) = discovered.get(physical.as_str()).copied() else {
            eprintln!(
                "{}: UH2001 mapped Uhura 0.4 source is missing from the project",
                physical.as_str()
            );
            broken = true;
            continue;
        };
        let identity = uhura_syntax::v04::SourceIdentity::new(
            index as u32,
            manifest.project.package_id().to_string(),
            logical.as_str(),
            physical.as_str(),
        );
        let parsed = uhura_syntax::v04::parse(identity, &file.text);
        if !parsed.diagnostics.is_empty() {
            for diagnostic in parsed.diagnostics {
                eprintln!(
                    "{}:{}..{}: R1001 {}",
                    file.rel_path, diagnostic.span.start, diagnostic.span.end, diagnostic.message
                );
            }
            broken = true;
            continue;
        }
        let formatted = match uhura_syntax::v04::format(&parsed.module) {
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

    for physical in &manifest.evidence {
        let Some((index, file)) = discovered.get(physical.as_str()).copied() else {
            eprintln!(
                "{}: UH2001 mapped evidence source is missing from the project",
                physical.as_str()
            );
            broken = true;
            continue;
        };
        let parsed = parse_v03(
            SourceId::new(FileId(index as u32), file.rel_path.clone()),
            &file.text,
        );
        if !parsed.diagnostics.is_empty() {
            for diagnostic in parsed.diagnostics {
                eprintln!(
                    "{}:{}..{}: R1001 {}",
                    file.rel_path, diagnostic.span.start, diagnostic.span.end, diagnostic.message
                );
            }
            broken = true;
            continue;
        }
        let Some(module) = parsed.module else {
            eprintln!(
                "{}: R1001 parser produced no evidence module",
                file.rel_path
            );
            broken = true;
            continue;
        };
        let formatted = format_v03(&module);
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
            "uhura-cli-fmt-v04-{label}-{}-{}",
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
    fn manifest_selected_v04_uses_the_v04_formatter() {
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
    fn v04_formatter_refuses_comments_without_changing_source() {
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
}
