// Shared corpus loading for gate tests — included via include!() so it is
// not its own test target (same trick as uhura-syntax's normative sources).

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/instagram-uhura")
}

/// Assembles the pipeline input exactly the way the CLI does, with an
/// optional per-file mutation and an examples on/off switch.
fn corpus_input(include_examples: bool, mutate: &dyn Fn(&str, String) -> String) -> CheckInput {
    let root = corpus_root();
    let read = |rel: &str| -> String {
        std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    };

    let manifest_text = read("uhura.toml");
    let manifest = load_manifest(&manifest_text).expect("corpus manifest is valid");

    let catalog_file = (
        manifest.catalog_path.clone(),
        Some(mutate(&manifest.catalog_path, read(&manifest.catalog_path))),
    );
    let port_files: BTreeMap<_, _> = manifest
        .ports
        .iter()
        .map(|(name, rel)| (name.clone(), (rel.clone(), Some(mutate(rel, read(rel))))))
        .collect();

    let mut sources = Vec::new();
    for dir in ["app", "components", "surfaces"] {
        walk(&root, &root.join(dir), &mut sources);
    }
    sources.sort_by(|a: &SourceInput, b| a.rel_path.cmp(&b.rel_path));
    let sources = sources
        .into_iter()
        .filter(|s| include_examples || s.kind == SourceKind::Module)
        .map(|s| SourceInput {
            text: mutate(&s.rel_path, s.text),
            ..s
        })
        .collect();

    let fixture_files = manifest
        .fixtures
        .iter()
        .map(|(name, rel)| {
            (
                name.clone(),
                (
                    rel.clone(),
                    std::fs::read_to_string(root.join(rel)).ok().map(|t| mutate(rel, t)),
                ),
            )
        })
        .collect();
    CheckInput {
        manifest,
        manifest_rel_path: "uhura.toml".into(),
        manifest_text,
        catalog_file,
        port_files,
        sources,
        theme_css: Some(("styles/theme.css".into(), read("styles/theme.css"))),
        fixture_files,
        lock_text: std::fs::read_to_string(root.join("uhura.lock")).ok(),
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<SourceInput>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".uhura") {
            continue;
        }
        let kind = if name.ends_with(".examples.uhura") {
            SourceKind::Examples
        } else {
            SourceKind::Module
        };
        let rel_path = path
            .strip_prefix(root)
            .expect("under root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        out.push(SourceInput {
            rel_path,
            text: std::fs::read_to_string(&path).expect("readable"),
            kind,
        });
    }
}

