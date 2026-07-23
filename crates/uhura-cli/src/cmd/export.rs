//! `uhura export [path] --out <directory>` — materialize one immutable,
//! listenerless Editor/Play web bundle from a checked project.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uhura_base::sha256_hex;
use uhura_host::{Host, StaticWebFile};

use crate::CommonArgs;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticBundleManifest {
    protocol: &'static str,
    bundle_id: String,
    source_id: String,
    tool_version: &'static str,
    mount_path: String,
    play_entry: String,
    entry_document: &'static str,
    history_fallback: HistoryFallback,
    editor_revision: u64,
    play_generation: u64,
    previews: usize,
    replay_derived_previews: usize,
    files: Vec<StaticBundleFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryFallback {
    scope: String,
    file: &'static str,
    methods: [&'static str; 2],
    only_when_file_missing: bool,
    exclude_prefixes: [&'static str; 2],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticBundleFile {
    path: String,
    sha256: String,
    bytes: usize,
    content_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebBuildMarker {
    protocol: String,
    profile: String,
    asset_base: String,
    host_config_protocol: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MaterializedWebBuildMarker<'a> {
    protocol: &'static str,
    profile: &'static str,
    asset_base: &'a str,
    host_config_protocol: &'static str,
    mount_path: &'a str,
    play_entry: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserHostConfig<'a> {
    protocol: &'static str,
    mount_path: &'a str,
    mode: &'static str,
    play_entry: &'a str,
}

const WEB_BUILD_PROTOCOL: &str = "uhura-web-build/1";
const HOST_CONFIG_PROTOCOL: &str = "uhura-host-config/0";
const HOST_CONFIG_OPEN: &str = "<script id=\"uhura-host-config\" type=\"application/json\">";
const BUNDLE_MANIFEST_PATH: &str = "uhura-static-bundle.json";

pub fn run(
    common: &CommonArgs,
    out: Option<&str>,
    mount: Option<&str>,
    play_entry: Option<&str>,
) -> ExitCode {
    let Some(out) = out else {
        eprintln!("uhura export: --out <directory> is required");
        return ExitCode::from(2);
    };
    let out = PathBuf::from(out);
    if let Err(message) = validate_output(&out) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    if let Err(message) = validate_output_topology(&common.root, &out) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    let mount_path = match normalize_mount_path(mount.unwrap_or("/")) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("uhura export: {message}");
            return ExitCode::from(2);
        }
    };
    let logical_play_entry = match normalize_play_entry(play_entry.unwrap_or("/play")) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("uhura export: {message}");
            return ExitCode::from(2);
        }
    };
    let public_play_entry = mounted_url(&mount_path, &logical_play_entry);

    let before = super::play::project_fingerprint(&common.root);
    let (candidate, _) = super::play::build_stable_candidate(&common.root, before, 1);
    let summary = candidate.summary();
    if !summary.editor_current || !summary.play_ok {
        let diagnostics = candidate.diagnostics();
        eprintln!("uhura export: Editor and Play must both check clean");
        eprintln!("Editor diagnostics: {}", diagnostics.editor);
        eprintln!("Play diagnostics: {}", diagnostics.play);
        return ExitCode::from(1);
    }
    let source_id = candidate.source_id();
    let web = match super::play::locate_export_web_assets() {
        Ok(web) => web,
        Err(message) => {
            eprintln!("uhura export: {message}");
            return ExitCode::from(2);
        }
    };
    let (host, report) = match Host::new(web, candidate) {
        Ok(host) => host,
        Err(message) => {
            eprintln!("uhura export: could not publish checked project: {message}");
            return ExitCode::from(2);
        }
    };
    let mut files = match host.static_files() {
        Ok(files) => files,
        Err(message) => {
            eprintln!("uhura export: {message}");
            return ExitCode::from(2);
        }
    };
    if let Err(message) = validate_reserved_output_paths(&files) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    if let Err(message) = validate_play_entry_target(&files, &logical_play_entry) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    if let Err(message) = materialize_static_web(
        &mut files,
        &mount_path,
        &logical_play_entry,
        &public_play_entry,
    ) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    if let Err(message) = validate_materialized_web(&files, &mount_path, &public_play_entry) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    if let Err(message) = validate_browser_runtime(&files) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    let file_manifest = files
        .iter()
        .map(|file| StaticBundleFile {
            path: file.path.clone(),
            sha256: sha256_hex(&file.bytes),
            bytes: file.bytes.len(),
            content_type: file.content_type.clone(),
        })
        .collect::<Vec<_>>();
    let bundle_id = sha256_hex(
        &serde_json::to_vec(&file_manifest).expect("static file manifest is always serializable"),
    );
    let manifest = StaticBundleManifest {
        protocol: "uhura-static-web-bundle/0",
        bundle_id,
        source_id,
        tool_version: env!("CARGO_PKG_VERSION"),
        mount_path: mount_path.clone(),
        play_entry: public_play_entry.clone(),
        entry_document: "index.html",
        history_fallback: HistoryFallback {
            scope: mount_path,
            file: "index.html",
            methods: ["GET", "HEAD"],
            only_when_file_missing: true,
            exclude_prefixes: ["api/", "assets/"],
        },
        editor_revision: report.source_revision,
        play_generation: report.play_generation,
        previews: report.preview_count.unwrap_or(0),
        replay_derived_previews: report.replay_derived_count.unwrap_or(0),
        files: file_manifest,
    };

    if let Err(message) = write_bundle(&out, &files, &manifest) {
        eprintln!("uhura export: {message}");
        return ExitCode::from(2);
    }
    println!(
        "uhura export: wrote {} files, {} previews, source {} to {} (mount {})",
        files.len(),
        manifest.previews,
        manifest.source_id,
        out.display(),
        manifest.mount_path,
    );
    ExitCode::SUCCESS
}

fn validate_export_web_template(files: &[StaticWebFile]) -> Result<WebBuildMarker, String> {
    let marker = files
        .iter()
        .find(|file| file.path == "uhura-web-build.json")
        .ok_or_else(|| {
            "static export requires the packaged export Web template \
             (`uhura-web-build.json` is missing)"
                .to_string()
        })?;
    let marker: WebBuildMarker = serde_json::from_slice(&marker.bytes)
        .map_err(|error| format!("invalid Uhura web build marker: {error}"))?;
    if marker.protocol != WEB_BUILD_PROTOCOL {
        return Err(format!(
            "unsupported Uhura web build marker protocol `{}`",
            marker.protocol
        ));
    }
    if marker.profile != "export-template" {
        return Err(format!(
            "Uhura export requires the export Web template, got profile `{}`",
            marker.profile
        ));
    }
    if marker.asset_base != "./" {
        return Err(format!(
            "Uhura export template must use relative assets, got `{}`",
            marker.asset_base
        ));
    }
    if marker.host_config_protocol != HOST_CONFIG_PROTOCOL {
        return Err(format!(
            "unsupported Uhura host config protocol `{}`",
            marker.host_config_protocol
        ));
    }
    Ok(marker)
}

fn materialize_static_web(
    files: &mut [StaticWebFile],
    mount_path: &str,
    logical_play_entry: &str,
    public_play_entry: &str,
) -> Result<(), String> {
    validate_export_web_template(files)?;
    let index = files
        .iter_mut()
        .find(|file| file.path == "index.html")
        .ok_or_else(|| "export Web template is missing index.html".to_string())?;
    let index_text = std::str::from_utf8(&index.bytes)
        .map_err(|error| format!("export Web template index.html is not UTF-8: {error}"))?;
    let double_quoted = index_text.matches("=\"./assets/").count();
    let single_quoted = index_text.matches("='./assets/").count();
    if double_quoted + single_quoted == 0 {
        return Err(
            "export Web template index.html has no relative application asset reference"
                .to_string(),
        );
    }
    let asset_prefix = format!("{}assets/", escape_html_attribute(mount_path));
    let materialized = index_text
        .replace("=\"./assets/", &format!("=\"{asset_prefix}"))
        .replace("='./assets/", &format!("='{asset_prefix}"));
    let config_start = materialized
        .find(HOST_CONFIG_OPEN)
        .ok_or_else(|| "export Web template is missing #uhura-host-config".to_string())?
        + HOST_CONFIG_OPEN.len();
    let config_end = materialized[config_start..]
        .find("</script>")
        .map(|offset| config_start + offset)
        .ok_or_else(|| "export Web template has an unterminated #uhura-host-config".to_string())?;
    let config = serde_json::to_string(&BrowserHostConfig {
        protocol: HOST_CONFIG_PROTOCOL,
        mount_path,
        mode: "static",
        play_entry: logical_play_entry,
    })
    .map_err(|error| format!("could not encode browser host config: {error}"))?;
    let config = escape_html_script_json(&config);
    let mut configured = String::with_capacity(materialized.len() + config.len());
    configured.push_str(&materialized[..config_start]);
    configured.push_str(&config);
    configured.push_str(&materialized[config_end..]);
    index.bytes = configured.into_bytes();

    let marker = files
        .iter_mut()
        .find(|file| file.path == "uhura-web-build.json")
        .expect("validated export Web marker exists");
    marker.bytes = serde_json::to_vec_pretty(&MaterializedWebBuildMarker {
        protocol: WEB_BUILD_PROTOCOL,
        profile: "static-export",
        asset_base: mount_path,
        host_config_protocol: HOST_CONFIG_PROTOCOL,
        mount_path,
        play_entry: public_play_entry,
    })
    .map_err(|error| format!("could not encode materialized Web marker: {error}"))?;
    marker.bytes.push(b'\n');
    Ok(())
}

fn validate_materialized_web(
    files: &[StaticWebFile],
    mount_path: &str,
    public_play_entry: &str,
) -> Result<(), String> {
    let index = files
        .iter()
        .find(|file| file.path == "index.html")
        .ok_or_else(|| "static web build is missing index.html".to_string())?;
    let index = std::str::from_utf8(&index.bytes)
        .map_err(|error| format!("static web index.html is not UTF-8: {error}"))?;
    if !index.contains(&format!("{}assets/", escape_html_attribute(mount_path))) {
        return Err(format!(
            "static web index.html does not use declared mount path `{mount_path}`"
        ));
    }
    if index.contains("./assets/") {
        return Err("static web index.html still contains relative entry assets".to_string());
    }
    let expected_config = escape_html_script_json(
        &serde_json::to_string(&BrowserHostConfig {
            protocol: HOST_CONFIG_PROTOCOL,
            mount_path,
            mode: "static",
            play_entry: strip_mount_from_url(mount_path, public_play_entry)
                .expect("public Play entry was mounted from the same path"),
        })
        .expect("browser host config is serializable"),
    );
    if !index.contains(&format!("{HOST_CONFIG_OPEN}{expected_config}</script>")) {
        return Err("static web index.html has inconsistent host configuration".to_string());
    }
    Ok(())
}

fn escape_html_script_json(json: &str) -> String {
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn strip_mount_from_url<'a>(mount_path: &str, url: &'a str) -> Option<&'a str> {
    if mount_path == "/" {
        return Some(url);
    }
    url.strip_prefix(mount_path.trim_end_matches('/'))
}

fn mounted_url(mount_path: &str, logical_url: &str) -> String {
    if mount_path == "/" {
        logical_url.to_string()
    } else {
        format!("{}{}", mount_path.trim_end_matches('/'), logical_url)
    }
}

fn normalize_mount_path(value: &str) -> Result<String, String> {
    if value.is_empty() || value != value.trim() {
        return Err("mount path must be an origin-local path".to_string());
    }
    let candidate = if value == "/" || value.ends_with('/') {
        value.to_string()
    } else {
        format!("{value}/")
    };
    normalize_origin_path(&candidate, "mount path", true, false)
}

fn normalize_play_entry(value: &str) -> Result<String, String> {
    if value.is_empty() || value != value.trim() {
        return Err("Play entry must be an origin-local path".to_string());
    }
    let boundary = value
        .char_indices()
        .find_map(|(index, character)| matches!(character, '?' | '#').then_some(index))
        .unwrap_or(value.len());
    let pathname = &value[..boundary];
    let pathname = normalize_play_entry_path(pathname, "Play entry")?;
    if play_entry_path_is_reserved(&pathname) {
        return Err("Play entry must select the Play surface".to_string());
    }
    let suffix = normalize_play_entry_suffix(&value[boundary..], "Play entry")?;
    Ok(format!("{pathname}{suffix}"))
}

fn normalize_play_entry_path(value: &str, label: &str) -> Result<String, String> {
    if !value.starts_with('/') || value.starts_with("//") || value.contains(['\\', '?', '#']) {
        return Err(format!("{label} must be an origin-local path"));
    }
    if value == "/" {
        return Ok("/".to_string());
    }
    if value.ends_with('/') && value != "/play/" {
        return Err(format!("{label} contains an empty path segment"));
    }
    let body = value
        .strip_prefix('/')
        .expect("origin-local path has a leading slash");
    let body = body.strip_suffix('/').unwrap_or(body);
    let mut normalized = Vec::new();
    for segment in body.split('/') {
        if segment.is_empty() {
            return Err(format!("{label} contains an empty path segment"));
        }
        let (canonical, _) = normalize_url_component(
            segment,
            label,
            is_route_path_component_char,
            is_route_path_component_char,
        )?;
        uhura_port::decode_opaque_path_component(&canonical)
            .map_err(|_| format!("{label} contains a non-canonical route component"))?;
        normalized.push(canonical);
    }
    let mut path = format!("/{}", normalized.join("/"));
    if value.ends_with('/') {
        path.push('/');
    }
    Ok(path)
}

fn normalize_origin_path(
    value: &str,
    label: &str,
    directory: bool,
    allow_encoded_slashes: bool,
) -> Result<String, String> {
    if !value.starts_with('/') || value.starts_with("//") || value.contains(['\\', '?', '#']) {
        return Err(format!("{label} must be an origin-local path"));
    }
    if directory && !value.ends_with('/') {
        return Err(format!("{label} must end with /"));
    }
    if value == "/" {
        return Ok("/".to_string());
    }
    let body = value
        .strip_prefix('/')
        .expect("origin-local path has a leading slash");
    let body = body.strip_suffix('/').unwrap_or(body);
    let mut normalized = Vec::new();
    for segment in body.split('/') {
        if segment.is_empty() {
            return Err(format!("{label} contains an empty path segment"));
        }
        normalized.push(normalize_url_segment(
            segment,
            label,
            allow_encoded_slashes,
        )?);
    }
    let mut path = format!("/{}", normalized.join("/"));
    if value.ends_with('/') {
        path.push('/');
    }
    Ok(path)
}

fn normalize_url_segment(
    segment: &str,
    label: &str,
    allow_encoded_slashes: bool,
) -> Result<String, String> {
    let (canonical, decoded) =
        normalize_url_component(segment, label, is_path_segment_char, is_unreserved)?;
    if decoded == "." || decoded == ".." || decoded.contains('\\') {
        return Err(format!("{label} contains an unsafe path segment"));
    }
    if decoded.contains('/')
        && (!allow_encoded_slashes
            || decoded
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | "..")))
    {
        return Err(format!("{label} contains an unsafe path segment"));
    }
    Ok(canonical)
}

fn normalize_play_entry_suffix(value: &str, label: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    if let Some(query) = value.strip_prefix('?') {
        let (query, fragment) = query
            .split_once('#')
            .map_or((query, None), |(query, fragment)| (query, Some(fragment)));
        let mut normalized = normalize_route_query(query, label)?;
        if let Some(fragment) = fragment {
            let fragment = normalize_fragment(fragment, label)?;
            if !fragment.is_empty() {
                normalized.push('#');
                normalized.push_str(&fragment);
            }
        }
        return Ok(normalized);
    }
    if let Some(fragment) = value.strip_prefix('#') {
        let fragment = normalize_fragment(fragment, label)?;
        return Ok(if fragment.is_empty() {
            String::new()
        } else {
            format!("#{fragment}")
        });
    }
    Err(format!("{label} has an invalid URL suffix"))
}

fn normalize_route_query(query: &str, label: &str) -> Result<String, String> {
    if query.is_empty() {
        return Ok(String::new());
    }
    let mut normalized = Vec::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            return Err(format!("{label} query contains an empty pair"));
        }
        let Some((key, value)) = pair.split_once('=') else {
            return Err(format!("{label} query pairs must contain `=`"));
        };
        if key.is_empty() || value.contains('=') {
            return Err(format!("{label} query contains a malformed pair"));
        }
        let key = normalize_route_query_component(key, label)?;
        let value = normalize_route_query_component(value, label)?;
        normalized.push(format!("{key}={value}"));
    }
    Ok(format!("?{}", normalized.join("&")))
}

fn normalize_route_query_component(value: &str, label: &str) -> Result<String, String> {
    let (canonical, _) = normalize_url_component(
        value,
        label,
        is_route_query_component_char,
        is_route_query_component_char,
    )?;
    uhura_port::decode_query_value(&canonical)
        .map_err(|_| format!("{label} contains a non-canonical route query component"))?;
    Ok(canonical)
}

fn normalize_fragment(value: &str, label: &str) -> Result<String, String> {
    normalize_url_component(value, label, is_url_suffix_char, is_unreserved)
        .map(|(canonical, _)| canonical)
}

fn normalize_url_component(
    value: &str,
    label: &str,
    raw_allowed: fn(u8) -> bool,
    escaped_raw_allowed: fn(u8) -> bool,
) -> Result<(String, String), String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut canonical = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            if !bytes[index].is_ascii() || !raw_allowed(bytes[index]) {
                return Err(format!(
                    "{label} contains a character that must be percent-encoded"
                ));
            }
            decoded.push(bytes[index]);
            canonical.push(char::from(bytes[index]));
            index += 1;
            continue;
        }
        let Some(high) = bytes.get(index + 1).and_then(|byte| hex_value(*byte)) else {
            return Err(format!("{label} contains an invalid percent escape"));
        };
        let Some(low) = bytes.get(index + 2).and_then(|byte| hex_value(*byte)) else {
            return Err(format!("{label} contains an invalid percent escape"));
        };
        let byte = (high << 4) | low;
        decoded.push(byte);
        if escaped_raw_allowed(byte) {
            canonical.push(char::from(byte));
        } else {
            canonical.push('%');
            canonical.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
            canonical.push(char::from(b"0123456789ABCDEF"[(byte & 0x0f) as usize]));
        }
        index += 3;
    }
    let decoded = String::from_utf8(decoded)
        .map_err(|_| format!("{label} contains an invalid UTF-8 escape"))?;
    if decoded.chars().any(char::is_control) {
        return Err(format!("{label} contains an unsafe control character"));
    }
    Ok((canonical, decoded))
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_route_path_component_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.' | b'_' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

fn is_route_query_component_char(byte: u8) -> bool {
    is_route_path_component_char(byte) && byte != b'\''
}

fn is_path_segment_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
        )
}

fn is_url_suffix_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'/'
                | b'?'
        )
}

fn play_entry_path_is_reserved(pathname: &str) -> bool {
    matches!(pathname, "/" | "/_uhura/editor" | "/_uhura/editor/")
        || pathname == "/api"
        || pathname.starts_with("/api/")
        || pathname == "/assets"
        || pathname.starts_with("/assets/")
}

fn validate_reserved_output_paths(files: &[StaticWebFile]) -> Result<(), String> {
    if files.iter().any(|file| file.path == BUNDLE_MANIFEST_PATH) {
        return Err(format!(
            "static Web payload uses reserved output path `{BUNDLE_MANIFEST_PATH}`"
        ));
    }
    Ok(())
}

fn validate_play_entry_target(
    files: &[StaticWebFile],
    logical_play_entry: &str,
) -> Result<(), String> {
    let boundary = logical_play_entry
        .char_indices()
        .find_map(|(index, character)| matches!(character, '?' | '#').then_some(index))
        .unwrap_or(logical_play_entry.len());
    let relative = logical_play_entry[..boundary]
        .strip_prefix('/')
        .expect("normalized Play entry is origin-local");
    if relative == BUNDLE_MANIFEST_PATH || files.iter().any(|file| file.path == relative) {
        return Err(format!(
            "Play entry `{}` selects an exported file instead of an application route",
            &logical_play_entry[..boundary]
        ));
    }
    Ok(())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_browser_runtime(files: &[StaticWebFile]) -> Result<(), String> {
    for required in [
        "api/play/static.json",
        "api/play/wasm/uhura_wasm.js",
        "api/play/wasm/uhura_wasm_bg.wasm",
    ] {
        if !files.iter().any(|file| file.path == required) {
            return Err(format!(
                "static Play export requires the browser runtime file `{required}`"
            ));
        }
    }
    Ok(())
}

fn validate_output(out: &Path) -> Result<(), String> {
    if out.as_os_str().is_empty() || out == Path::new("/") {
        return Err("refusing unsafe output directory".to_string());
    }
    if out
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "output directory may not contain `..`: {}",
            out.display()
        ));
    }
    let name = out.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if name.is_empty() || matches!(name, "." | "..") {
        return Err(format!("unsafe output directory: {}", out.display()));
    }
    if let Ok(metadata) = fs::symlink_metadata(out)
        && (metadata.file_type().is_symlink() || !metadata.is_dir())
    {
        return Err(format!(
            "existing output must be a regular directory, not {}",
            out.display()
        ));
    }
    Ok(())
}

fn validate_output_topology(project: &Path, out: &Path) -> Result<(), String> {
    let project = fs::canonicalize(project).map_err(|error| {
        format!(
            "could not resolve project root {}: {error}",
            project.display()
        )
    })?;
    let out = resolve_output_path(out)?;
    if project == out {
        return Err("output directory must not replace the project root".to_string());
    }
    if project.starts_with(&out) {
        return Err("output directory must not contain the project root".to_string());
    }
    if out.starts_with(&project) {
        return Err("output directory must be outside the project root".to_string());
    }
    Ok(())
}

fn resolve_output_path(out: &Path) -> Result<PathBuf, String> {
    let absolute = if out.is_absolute() {
        out.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("could not resolve current directory: {error}"))?
            .join(out)
    };
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| format!("could not resolve output directory {}", out.display()))?;
        missing.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| format!("could not resolve output directory {}", out.display()))?;
    }
    let mut resolved = fs::canonicalize(existing).map_err(|error| {
        format!(
            "could not resolve output directory ancestor {}: {error}",
            existing.display()
        )
    })?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn write_bundle(
    out: &Path,
    files: &[uhura_host::StaticWebFile],
    manifest: &StaticBundleManifest,
) -> Result<(), String> {
    let parent = out.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_nanos();
    let name = out
        .file_name()
        .and_then(|name| name.to_str())
        .expect("validated output has a UTF-8 name");
    let staging = parent.join(format!(".{name}.tmp-{}-{nonce}", std::process::id()));
    let backup = parent.join(format!(".{name}.old-{}-{nonce}", std::process::id()));
    fs::create_dir(&staging)
        .map_err(|error| format!("could not create {}: {error}", staging.display()))?;

    let result = (|| {
        for file in files {
            let relative = validated_relative(&file.path)?;
            let destination = staging.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
            }
            fs::write(&destination, &file.bytes)
                .map_err(|error| format!("could not write {}: {error}", destination.display()))?;
        }
        let manifest_json = serde_json::to_vec_pretty(manifest)
            .map_err(|error| format!("could not encode bundle manifest: {error}"))?;
        fs::write(staging.join(BUNDLE_MANIFEST_PATH), manifest_json)
            .map_err(|error| format!("could not write static bundle manifest: {error}"))?;

        let existed = out.exists();
        if existed {
            fs::rename(out, &backup)
                .map_err(|error| format!("could not stage existing {}: {error}", out.display()))?;
        }
        if let Err(error) = fs::rename(&staging, out) {
            if existed {
                let _ = fs::rename(&backup, out);
            }
            return Err(format!(
                "could not publish static bundle to {}: {error}",
                out.display()
            ));
        }
        if existed && let Err(error) = fs::remove_dir_all(&backup) {
            eprintln!(
                "uhura export: published bundle but could not remove {}: {error}",
                backup.display()
            );
        }
        Ok(())
    })();

    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn validated_relative(path: &str) -> Result<&Path, String> {
    let relative = Path::new(path);
    if path.is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("static file has an unsafe path: {path}"));
    }
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_file(path: &str) -> StaticWebFile {
        StaticWebFile {
            path: path.to_string(),
            content_type: "application/octet-stream".to_string(),
            bytes: vec![1],
        }
    }

    #[test]
    fn export_output_must_be_disjoint_from_the_project_tree() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-export-topology-{}-{nonce}",
            std::process::id()
        ));
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();

        assert!(validate_output_topology(&project, &root.join("published")).is_ok());
        assert!(validate_output_topology(&project, &project).is_err());
        assert!(validate_output_topology(&project, &root).is_err());
        assert!(validate_output_topology(&project, &project.join("dist")).is_err());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn static_export_requires_the_complete_browser_runtime() {
        let metadata = runtime_file("api/play/static.json");
        let glue = runtime_file("api/play/wasm/uhura_wasm.js");
        let wasm = runtime_file("api/play/wasm/uhura_wasm_bg.wasm");

        assert!(validate_browser_runtime(&[metadata.clone(), glue.clone(), wasm]).is_ok());
        let error = validate_browser_runtime(&[metadata, glue]).unwrap_err();
        assert!(error.contains("uhura_wasm_bg.wasm"), "{error}");
    }

    fn export_template_files() -> Vec<StaticWebFile> {
        let index = StaticWebFile {
            path: "index.html".to_string(),
            content_type: "text/html".to_string(),
            bytes: format!(
                r#"{HOST_CONFIG_OPEN}{{"protocol":"uhura-host-config/0","mountPath":"/","mode":"live","playEntry":"/play"}}</script><script src="./assets/app.js"></script>"#
            )
            .into_bytes(),
        };
        let marker = StaticWebFile {
            path: "uhura-web-build.json".to_string(),
            content_type: "application/json".to_string(),
            bytes: br#"{"protocol":"uhura-web-build/1","profile":"export-template","assetBase":"./","hostConfigProtocol":"uhura-host-config/0"}"#.to_vec(),
        };
        vec![index, marker]
    }

    #[test]
    fn materializes_one_export_template_for_any_mount() {
        let mut files = export_template_files();
        materialize_static_web(
            &mut files,
            "/products/uhura/",
            "/orders/100?step=items",
            "/products/uhura/orders/100?step=items",
        )
        .unwrap();
        validate_materialized_web(
            &files,
            "/products/uhura/",
            "/products/uhura/orders/100?step=items",
        )
        .unwrap();

        let index = files.iter().find(|file| file.path == "index.html").unwrap();
        let index = std::str::from_utf8(&index.bytes).unwrap();
        assert!(index.contains("src=\"/products/uhura/assets/app.js\""));
        assert!(index.contains(
            r#""mountPath":"/products/uhura/","mode":"static","playEntry":"/orders/100?step=items""#
        ));
        let marker: serde_json::Value = serde_json::from_slice(
            &files
                .iter()
                .find(|file| file.path == "uhura-web-build.json")
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!(marker["profile"], "static-export");
        assert_eq!(marker["playEntry"], "/products/uhura/orders/100?step=items");

        let mut root_files = export_template_files();
        materialize_static_web(&mut root_files, "/", "/play", "/play").unwrap();
        validate_materialized_web(&root_files, "/", "/play").unwrap();
        let root_index = root_files
            .iter()
            .find(|file| file.path == "index.html")
            .unwrap();
        assert!(
            std::str::from_utf8(&root_index.bytes)
                .unwrap()
                .contains("src=\"/assets/app.js\"")
        );

        let mut escaped_files = export_template_files();
        materialize_static_web(
            &mut escaped_files,
            "/research&proof/",
            "/play",
            "/research&proof/play",
        )
        .unwrap();
        let escaped_index = escaped_files
            .iter()
            .find(|file| file.path == "index.html")
            .unwrap();
        assert!(
            std::str::from_utf8(&escaped_index.bytes)
                .unwrap()
                .contains("src=\"/research&amp;proof/assets/app.js\"")
        );
    }

    #[test]
    fn static_export_requires_the_dedicated_export_template() {
        let index = export_template_files().remove(0);
        let error = validate_export_web_template(&[index]).unwrap_err();
        assert!(error.contains("packaged export Web template"), "{error}");

        let mut files = export_template_files();
        files
            .iter_mut()
            .find(|file| file.path == "uhura-web-build.json")
            .unwrap()
            .bytes = br#"{"protocol":"uhura-web-build/1","profile":"live","assetBase":"/","hostConfigProtocol":"uhura-host-config/0"}"#.to_vec();
        let error = validate_export_web_template(&files).unwrap_err();
        assert!(error.contains("profile `live`"), "{error}");
    }

    #[test]
    fn mount_and_play_entry_paths_are_canonical_and_origin_local() {
        assert_eq!(normalize_mount_path("/").unwrap(), "/");
        assert_eq!(normalize_mount_path("/demo").unwrap(), "/demo/");
        assert_eq!(
            normalize_mount_path("/space%20name").unwrap(),
            "/space%20name/"
        );
        assert_eq!(
            normalize_mount_path("/%eb%8d%b0%eb%aa%a8/").unwrap(),
            "/%EB%8D%B0%EB%AA%A8/"
        );
        assert_eq!(normalize_mount_path("/%41%3a/").unwrap(), "/A%3A/");
        assert_eq!(
            normalize_play_entry("/orders/100?step=%69tems#sum%6dary").unwrap(),
            "/orders/100?step=items#summary"
        );
        assert_eq!(
            normalize_play_entry("/orders/%E2%82%AC?note=%27ok%27").unwrap(),
            "/orders/%E2%82%AC?note=%27ok%27"
        );
        assert_eq!(
            normalize_play_entry("/search?q=a%26b%3Dc").unwrap(),
            "/search?q=a%26b%3Dc"
        );
        assert_eq!(
            normalize_play_entry("/orders/return%2fwith%2fslash?step=review%2fconfirm%2b").unwrap(),
            "/orders/return%2Fwith%2Fslash?step=review%2Fconfirm%2B"
        );
        assert_eq!(normalize_play_entry("/author's").unwrap(), "/author's");

        for invalid in [
            "demo",
            "//example.com/",
            "/demo//",
            "/./",
            "/../",
            "/%2e%2e/",
            "/a%2fb/",
            "/a%5cb/",
            "/demo/?query",
            "/demo/#fragment",
            "/demo\\",
            "/demo space/",
            "/데모/",
            "/demo\"/",
            "/demo`/",
            "/%00/",
            "/%7f/",
            "/%c2%85/",
            "/%ff/",
        ] {
            assert!(
                normalize_mount_path(invalid).is_err(),
                "accepted mount {invalid}"
            );
        }
        for invalid in [
            "/",
            "/_uhura/editor",
            "//example.com/play",
            "/../play",
            "/%2e%2e/play",
            "/api",
            "/api/play/config.json",
            "/assets",
            "/assets/app.js",
            "/orders/",
            "/orders/$identity",
            "/play?query",
            "/play?=value",
            "/play?state=open&&step=review",
            "/play?state=open=again",
            "/play?unsafe=+",
            "/play?unsafe='",
            "/play?unsafe=\u{7f}",
            "/play?unsafe=%C2%85",
            "/play?unsafe=raw space",
            "/play#unsafe#fragment",
        ] {
            assert!(
                normalize_play_entry(invalid).is_err(),
                "accepted Play entry {invalid}"
            );
        }
    }

    #[test]
    fn export_reserves_its_manifest_and_application_entry_routes() {
        let manifest = runtime_file(BUNDLE_MANIFEST_PATH);
        let error = validate_reserved_output_paths(&[manifest]).unwrap_err();
        assert!(error.contains("reserved output path"), "{error}");

        let files = [
            runtime_file("index.html"),
            runtime_file("favicon.svg"),
            runtime_file("api/play/config.json"),
        ];
        for entry in [
            "/index.html",
            "/favicon.svg?theme=dark",
            "/uhura-static-bundle.json",
        ] {
            let error = validate_play_entry_target(&files, entry).unwrap_err();
            assert!(error.contains("exported file"), "{error}");
        }
        assert!(validate_play_entry_target(&files, "/returns/100").is_ok());
    }

    #[test]
    fn internal_resource_paths_allow_identity_slashes_but_not_traversal() {
        assert_eq!(
            normalize_origin_path("/api/play/assets/poster%2fone.jpg", "resource", false, true,)
                .unwrap(),
            "/api/play/assets/poster%2Fone.jpg"
        );
        for path in [
            "/api/play/assets/%2e%2e/outside",
            "/api/play/assets/poster%2F..%2Foutside",
            "/api/play/assets/%2Foutside",
        ] {
            assert!(
                normalize_origin_path(path, "resource", false, true).is_err(),
                "accepted resource {path}"
            );
        }
    }
}
