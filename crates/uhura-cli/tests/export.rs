use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uhura_base::sha256_hex;

const MANIFEST_PATH: &str = "uhura-static-bundle.json";
const WEB_SENTINEL: &str = "export const packagedWeb = true;";
const WASM_JS_SENTINEL: &str = "export default async()=>({packaged:true});";
const WASM_BINARY_SENTINEL: &[u8] = b"packaged-wasm";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportManifest {
    bundle_id: String,
    mount_path: String,
    play_entry: String,
    entry_document: String,
    history_fallback: Value,
    files: Vec<ManifestFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    path: String,
    sha256: String,
    bytes: usize,
    content_type: String,
}

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "uhura-cli-export-test-{}-{nonce}",
        std::process::id()
    ))
}

fn write_export_template(root: &Path, profile: &str) {
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(
        root.join("index.html"),
        r#"<!doctype html><div id="uhura-root"></div><script id="uhura-host-config" type="application/json">{"protocol":"uhura-host-config/0","mountPath":"/","mode":"live","playEntry":"/play"}</script><script type="module" src="./assets/app.js"></script>"#,
    )
    .unwrap();
    fs::write(root.join("assets/app.js"), WEB_SENTINEL).unwrap();
    fs::write(
        root.join("uhura-web-build.json"),
        format!(
            r#"{{"protocol":"uhura-web-build/1","profile":"{profile}","assetBase":"./","hostConfigProtocol":"uhura-host-config/0"}}"#
        ),
    )
    .unwrap();
}

fn write_wasm(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("uhura_wasm.js"), WASM_JS_SENTINEL).unwrap();
    fs::write(root.join("uhura_wasm_bg.wasm"), WASM_BINARY_SENTINEL).unwrap();
}

fn assemble_package(root: &Path) -> PathBuf {
    let package = root.join("package");
    let binary = package.join("bin/uhura");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    let source_binary = Path::new(env!("CARGO_BIN_EXE_uhura"));
    fs::copy(source_binary, &binary).unwrap();
    fs::set_permissions(&binary, fs::metadata(source_binary).unwrap().permissions()).unwrap();
    write_export_template(&package.join("share/uhura/web-export"), "export-template");
    write_wasm(&package.join("share/uhura/wasm"));
    package
}

fn write_test_project(root: &Path) -> PathBuf {
    let project = root.join("project");
    fs::create_dir_all(project.join("app")).unwrap();
    fs::write(
        project.join("uhura.toml"),
        r#"[project]
name = "test.export"
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
    fs::write(
        project.join("routing.uhura"),
        "pub enum Location { Home }\n",
    )
    .unwrap();
    fs::write(
        project.join("machine.uhura"),
        r#"use uhura::web_router::Router;
use crate::framework::routes::APPLICATION_ROUTES;
use crate::routing::Location;

pub machine App {
  port router = Router<Location> { routes: APPLICATION_ROUTES };
  events { Refresh }
  outcomes { commit Accepted }
  state { location: Option<Location> = None }
  observe { location }
  on Refresh { Accepted }
  on router.Changed(next) {
    location = Some(next);
    Accepted
  }
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("app/page.uhura"),
        r#"use uhura::ui;
use crate::program::App;

pub ui HomePage for App(view) {
  <main>Home</main>
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("app/page.examples.uhura"),
        r#"use uhura::web_router::Router;
use crate::framework::routes::APPLICATION_ROUTES;
use crate::program::App;
use crate::app::HomePage;

scenario home_scenario for App {
  bind router = Router.fixture(APPLICATION_ROUTES)
  start
  pin frame
}

example home
  for HomePage as page default
  = home_scenario::frame;
"#,
    )
    .unwrap();
    fs::write(
        project.join("host.toml"),
        r#"[entry.app]
machine = "crate::App"
presentation = "crate::Application"
lifetime = "application-session"

[entry.app.ports]
router = "web.history"
"#,
    )
    .unwrap();
    project
}

fn run_export(package: &Path, project: &Path, out: &Path) -> std::process::Output {
    Command::new(package.join("bin/uhura"))
        .current_dir(package)
        .env_remove("UHURA_EXPORT_WEB_DIST")
        .env_remove("UHURA_WEB_DIST")
        .env_remove("UHURA_WASM_DIST")
        .arg("export")
        .arg(project)
        .arg("--out")
        .arg(out)
        .arg("--mount")
        .arg("/products/uhura/")
        .arg("--play-entry")
        .arg("/returns?status=open")
        .output()
        .unwrap()
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        if file_type.is_dir() {
            collect_files(root, &entry.path(), files);
        } else {
            assert!(file_type.is_file(), "export contains a non-file entry");
            let relative = entry.path().strip_prefix(root).unwrap().to_path_buf();
            files.push(
                relative
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}

fn file_paths(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files);
    files.sort();
    files
}

fn directory_snapshot(root: &Path) -> BTreeMap<String, String> {
    file_paths(root)
        .into_iter()
        .map(|path| {
            let digest = sha256_hex(&fs::read(root.join(&path)).unwrap());
            (path, digest)
        })
        .collect()
}

fn read_manifest(out: &Path) -> (Vec<u8>, ExportManifest) {
    let bytes = fs::read(out.join(MANIFEST_PATH)).unwrap();
    let manifest = serde_json::from_slice(&bytes).unwrap();
    (bytes, manifest)
}

fn assert_exact_manifest(out: &Path, manifest: &ExportManifest) {
    let declared = manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    assert!(
        declared.windows(2).all(|pair| pair[0] < pair[1]),
        "manifest paths must be unique and strictly sorted"
    );
    assert!(!declared.iter().any(|path| path == MANIFEST_PATH));

    let actual = file_paths(out)
        .into_iter()
        .filter(|path| path != MANIFEST_PATH)
        .collect::<Vec<_>>();
    assert_eq!(
        actual, declared,
        "manifest must inventory every payload file"
    );

    for file in &manifest.files {
        let bytes = fs::read(out.join(&file.path)).unwrap();
        assert_eq!(
            file.bytes,
            bytes.len(),
            "wrong byte length for {}",
            file.path
        );
        assert_eq!(
            file.sha256,
            sha256_hex(&bytes),
            "wrong digest for {}",
            file.path
        );
    }
    assert_eq!(
        manifest.bundle_id,
        sha256_hex(&serde_json::to_vec(&manifest.files).unwrap()),
        "bundle identity must cover the exact ordered inventory"
    );
}

#[test]
fn packaged_cli_export_materializes_and_replaces_one_verified_bundle() {
    let root = temporary_root();
    let package = assemble_package(&root);
    let project = write_test_project(&root);
    let out = root.join("published");

    let nested_out = project.join("dist");
    let rejected_nested = run_export(&package, &project, &nested_out);
    assert!(!rejected_nested.status.success());
    assert!(String::from_utf8_lossy(&rejected_nested.stderr).contains("outside the project root"));
    assert!(!nested_out.exists());

    let output = run_export(&package, &project, &out);
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    for required in [
        "index.html",
        "assets/app.js",
        "api/editor/state",
        "api/play/ir.json",
        "api/play/static.json",
        "api/play/wasm/uhura_wasm.js",
        "api/play/wasm/uhura_wasm_bg.wasm",
        MANIFEST_PATH,
    ] {
        assert!(out.join(required).is_file(), "missing {required}");
    }
    assert!(!out.join("api/editor/events").exists());
    assert!(!out.join("api/play/events").exists());
    assert_eq!(
        fs::read_to_string(out.join("assets/app.js")).unwrap(),
        WEB_SENTINEL
    );
    assert_eq!(
        fs::read_to_string(out.join("api/play/wasm/uhura_wasm.js")).unwrap(),
        WASM_JS_SENTINEL
    );
    assert_eq!(
        fs::read(out.join("api/play/wasm/uhura_wasm_bg.wasm")).unwrap(),
        WASM_BINARY_SENTINEL
    );

    let index = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(index.contains("src=\"/products/uhura/assets/app.js\""));
    assert!(index.contains(r#""mountPath":"/products/uhura/""#));
    assert!(index.contains(r#""mode":"static""#));
    assert!(index.contains(r#""playEntry":"/returns?status=open""#));

    let editor: Value =
        serde_json::from_slice(&fs::read(out.join("api/editor/state")).unwrap()).unwrap();
    let play: Value =
        serde_json::from_slice(&fs::read(out.join("api/play/static.json")).unwrap()).unwrap();
    assert_eq!(editor["sourceRevision"], 1);
    assert_eq!(editor["render"]["revision"], 1);
    assert_eq!(play["playGeneration"], 1);

    let (manifest_bytes, manifest) = read_manifest(&out);
    assert_eq!(manifest.mount_path, "/products/uhura/");
    assert_eq!(manifest.play_entry, "/products/uhura/returns?status=open");
    assert_eq!(manifest.entry_document, "index.html");
    assert_eq!(manifest.history_fallback["scope"], "/products/uhura/");
    assert_exact_manifest(&out, &manifest);

    fs::write(out.join("obsolete.txt"), "old generation").unwrap();
    let repeated = run_export(&package, &project, &out);
    assert!(
        repeated.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert!(!out.join("obsolete.txt").exists());
    let (repeated_manifest_bytes, repeated_manifest) = read_manifest(&out);
    assert_eq!(
        repeated_manifest_bytes, manifest_bytes,
        "identical inputs and topology must produce one manifest"
    );
    assert_eq!(repeated_manifest.bundle_id, manifest.bundle_id);
    assert_exact_manifest(&out, &repeated_manifest);

    let accepted_snapshot = directory_snapshot(&out);
    write_export_template(&package.join("share/uhura/web-export"), "live");
    let rejected = run_export(&package, &project, &out);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("profile `live`"));
    assert_eq!(
        directory_snapshot(&out),
        accepted_snapshot,
        "a rejected replacement must leave every published byte untouched"
    );

    fs::remove_dir_all(root).unwrap();
}
