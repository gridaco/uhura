use std::sync::atomic::{AtomicU64, Ordering};

use uhura_project::{
    AdmittedSourceKind, ResolvedProfile, ResolvedUiRole, capture_project_snapshot, resolve_project,
};

const MACHINE: &str = r#"pub machine Counter {
  events { Increment }
  outcomes { commit Accepted }
  state { count: Int = 0 }
  observe { count }
  on Increment { count = count + 1; Accepted }
}
"#;

fn project_root(label: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "uhura-project-resolution-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write_project(root: &std::path::Path, framework: &str) {
    std::fs::write(
        root.join("uhura.toml"),
        format!(
            r#"[project]
name = "test.counter"
version = 1
language = "0.4"

{framework}
[modules]
counter = "counter.uhura"
"#
        ),
    )
    .unwrap();
    std::fs::write(root.join("counter.uhura"), MACHINE).unwrap();
}

fn write_web_app(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("app/profile/[user]")).unwrap();
    std::fs::create_dir_all(root.join("components")).unwrap();
    std::fs::create_dir_all(root.join("surfaces")).unwrap();
    std::fs::write(
        root.join("uhura.toml"),
        r#"[project]
name = "test.web-app"
version = 1
language = "0.4"

[framework]
profile = "web-app"
version = 1
machine = "crate::program::App"
location = "crate::location::Location"

[modules]
program = "machine.uhura"
location = "location.uhura"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("location.uhura"),
        r#"pub enum Location {
  Home,
  Profile { user: Text },
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("machine.uhura"),
        r#"use uhura::web_router::Router;
use crate::framework::routes::APPLICATION_ROUTES;
use crate::location::Location;

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
    std::fs::write(
        root.join("app/page.uhura"),
        r#"use uhura::ui;
use crate::program::App;

pub ui HomePage for App(view) {
  <main>Home</main>
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("app/page.examples.uhura"),
        r#"use crate::program::App;
use crate::app::HomePage;

scenario home_scenario for App {
  start
  pin frame
}

example home
  for HomePage as page default
  = home_scenario::frame;
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("app/profile/[user]/page.uhura"),
        r#"use uhura::ui;
use crate::program::App;

pub ui ProfilePage for App(view) {
  <main>Profile</main>
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("components/post-card.uhura"),
        r#"use uhura::ui;

pub ui PostCard(title: Text) {
  <p>{title}</p>
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("surfaces/confirm-dialog.uhura"),
        r#"use uhura::ui;

pub ui ConfirmDialog(message: Text) {
  <section>{message}</section>
}
"#,
    )
    .unwrap();
}

fn resolve_clean(root: &std::path::Path) -> uhura_project::ResolvedProject {
    resolve_project(&capture_project_snapshot(root))
        .unwrap_or_else(|rejection| panic!("project rejection: {:#?}", rejection.diagnostics))
}

#[test]
fn explicit_project_resolves_to_a_stable_checked_inventory() {
    let root = project_root("explicit");
    write_project(&root, "");

    let snapshot = capture_project_snapshot(&root);
    let resolved = resolve_project(&snapshot)
        .unwrap_or_else(|rejection| panic!("project rejection: {:#?}", rejection.diagnostics));
    assert_eq!(resolved.application().profile, ResolvedProfile::Explicit);
    assert_eq!(resolved.sources().len(), 1);
    assert_eq!(resolved.sources()[0].file.0, 0);
    assert_eq!(
        resolved.source_map().path(resolved.manifest_file()),
        "uhura.toml"
    );
    assert!(resolved.check().diagnostics.is_empty());

    let artifact: serde_json::Value =
        serde_json::from_str(&resolved.application().canonical_json()).unwrap();
    assert_eq!(artifact["protocol"], "uhura-resolved-application/0");
    assert_eq!(artifact["profile"]["kind"], "explicit");
    assert_eq!(artifact["modules"][0]["logical"], "counter");
    assert_eq!(artifact["modules"][0]["path"], "counter.uhura");

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_generated_named_path_remains_authored() {
    let root = project_root("explicit-generated-named-path");
    std::fs::create_dir_all(root.join(".uhura/generated")).unwrap();
    std::fs::write(
        root.join("uhura.toml"),
        r#"[project]
name = "test.counter"
version = 1
language = "0.4"

[modules]
counter = ".uhura/generated/custom.uhura"
"#,
    )
    .unwrap();
    std::fs::write(root.join(".uhura/generated/custom.uhura"), MACHINE).unwrap();

    let resolved = resolve_clean(&root);
    assert_eq!(resolved.application().modules.len(), 1);
    assert_eq!(
        resolved.application().modules[0].source_kind,
        AdmittedSourceKind::Authored
    );
    assert_eq!(
        resolved
            .root_authored_sources()
            .map(|source| source.path.as_str())
            .collect::<Vec<_>>(),
        vec![".uhura/generated/custom.uhura"]
    );
    assert!(resolved.check().diagnostics.is_empty());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn web_app_discovers_roles_and_generates_a_checked_application() {
    let root = project_root("framework");
    write_web_app(&root);

    let snapshot = capture_project_snapshot(&root);
    let resolved = resolve_project(&snapshot)
        .unwrap_or_else(|rejection| panic!("project rejection: {:#?}", rejection.diagnostics));
    assert_eq!(
        resolved.application().profile,
        ResolvedProfile::WebApp { version: 1 }
    );
    assert_eq!(resolved.root_authored_sources().count(), 7);
    assert_eq!(resolved.non_generated_sources().count(), 7);
    assert_eq!(
        resolved
            .sources()
            .iter()
            .filter(|source| source.kind == AdmittedSourceKind::Generated)
            .count(),
        2
    );
    let application = resolved
        .application()
        .web_app
        .as_ref()
        .expect("web app metadata");
    assert_eq!(application.machine, "crate::program::App");
    assert_eq!(application.location, "crate::location::Location");
    assert_eq!(application.application, "test.web-app@1::Application");
    assert_eq!(application.root_page, "test.web-app@1::HomePage");
    assert_eq!(application.subjects.len(), 4);
    let profile = application
        .subjects
        .iter()
        .find(|subject| subject.declaration == "ProfilePage")
        .unwrap();
    assert_eq!(profile.logical, "app::profile::param__user");
    assert_eq!(profile.role, ResolvedUiRole::Page);
    assert_eq!(profile.route.as_ref().unwrap().constructor, "Profile");
    assert_eq!(profile.route.as_ref().unwrap().pattern, "/profile/{user}");
    let home = application
        .subjects
        .iter()
        .find(|subject| subject.declaration == "HomePage")
        .unwrap();
    assert_eq!(
        home.evidence_logical.as_deref(),
        Some("framework::evidence::app")
    );
    assert_eq!(
        home.evidence_path.as_deref(),
        Some("app/page.examples.uhura")
    );
    let generated_application = resolved
        .sources()
        .iter()
        .find(|source| source.path.ends_with("application.uhura"))
        .unwrap();
    assert!(
        generated_application
            .text
            .contains("pub ui Application for App(view)")
    );
    assert!(generated_application.text.contains("<ProfilePage />"));
    let checked = resolved.check();
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    assert!(checked.program.is_some());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn web_app_generation_uses_the_configured_location_declaration_name() {
    let root = project_root("framework-location-name");
    write_web_app(&root);
    for path in ["uhura.toml", "location.uhura", "machine.uhura"] {
        let source = std::fs::read_to_string(root.join(path)).unwrap();
        std::fs::write(root.join(path), source.replace("Location", "Route")).unwrap();
    }

    let resolved = resolve_clean(&root);
    let application = resolved
        .sources()
        .iter()
        .find(|source| source.path.ends_with("application.uhura"))
        .expect("generated Application source");
    assert!(application.text.contains("Some(Route::Profile { user })"));
    assert!(!application.text.contains("Some(Location::"));
    let checked = resolved.check();
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    assert!(checked.program.is_some());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn web_app_check_requires_the_generated_route_table_on_its_machine_router() {
    let missing = project_root("framework-router-missing");
    write_web_app(&missing);
    let machine = std::fs::read_to_string(missing.join("machine.uhura")).unwrap();
    std::fs::write(
        missing.join("machine.uhura"),
        machine
            .replace(
                "  port router = Router<Location> { routes: APPLICATION_ROUTES };\n",
                "",
            )
            .replace(
                r#"  on router.Changed(next) {
    location = Some(next);
    Accepted
  }
"#,
                "",
            ),
    )
    .unwrap();
    let missing_checked = resolve_clean(&missing).check();
    assert!(missing_checked.program.is_none());
    assert!(missing_checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/framework-router"
            && diagnostic.message.contains("must declare a Router port")
            && diagnostic.message.contains("APPLICATION_ROUTES")
    }));
    std::fs::remove_dir_all(missing).unwrap();

    let missing_handler = project_root("framework-router-missing-handler");
    write_web_app(&missing_handler);
    let machine = std::fs::read_to_string(missing_handler.join("machine.uhura")).unwrap();
    std::fs::write(
        missing_handler.join("machine.uhura"),
        machine.replace(
            r#"  on router.Changed(next) {
    location = Some(next);
    Accepted
  }
"#,
            "",
        ),
    )
    .unwrap();
    let missing_handler_checked = resolve_clean(&missing_handler).check();
    assert!(missing_handler_checked.program.is_none());
    assert!(
        missing_handler_checked
            .diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic.rule == "uhura/missing-handler"
                    && diagnostic.message.contains("router.changed")
            }),
        "missing Router handler diagnostics: {:#?}",
        missing_handler_checked.diagnostics
    );
    std::fs::remove_dir_all(missing_handler).unwrap();

    let wrong = project_root("framework-router-wrong-table");
    write_web_app(&wrong);
    let location = std::fs::read_to_string(wrong.join("location.uhura")).unwrap();
    std::fs::write(
        wrong.join("location.uhura"),
        format!(
            "use uhura::web_router::Routes;\n\n{location}\npub const OTHER_ROUTES: Routes<Location> = Routes::from([(\"Home\", \"/\"), (\"Profile\", \"/profile/{{user}}\")]);\n"
        ),
    )
    .unwrap();
    let machine = std::fs::read_to_string(wrong.join("machine.uhura")).unwrap();
    std::fs::write(
        wrong.join("machine.uhura"),
        machine
            .replace(
                "use crate::location::Location;",
                "use crate::location::{Location, OTHER_ROUTES};",
            )
            .replace("routes: APPLICATION_ROUTES", "routes: OTHER_ROUTES"),
    )
    .unwrap();
    let wrong_checked = resolve_clean(&wrong).check();
    assert!(wrong_checked.program.is_none());
    assert!(wrong_checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/framework-router"
            && diagnostic.message.contains("OTHER_ROUTES")
            && diagnostic.message.contains("APPLICATION_ROUTES")
    }));
    std::fs::remove_dir_all(wrong).unwrap();

    let duplicate = project_root("framework-router-duplicate");
    write_web_app(&duplicate);
    let machine = std::fs::read_to_string(duplicate.join("machine.uhura")).unwrap();
    std::fs::write(
        duplicate.join("machine.uhura"),
        machine
            .replace(
                "  port router = Router<Location> { routes: APPLICATION_ROUTES };",
                "  port router = Router<Location> { routes: APPLICATION_ROUTES };\n  port router_backup = Router<Location> { routes: APPLICATION_ROUTES };",
            )
            .replace(
                r#"  on router.Changed(next) {
    location = Some(next);
    Accepted
  }
"#,
                r#"  on router.Changed(next) {
    location = Some(next);
    Accepted
  }
  on router_backup.Changed(next) {
    location = Some(next);
    Accepted
  }
"#,
            ),
    )
    .unwrap();
    let duplicate_checked = resolve_clean(&duplicate).check();
    assert!(duplicate_checked.program.is_none());
    assert!(duplicate_checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/framework-router"
            && diagnostic.message.contains("exactly one Router port")
            && diagnostic.message.contains("router, router_backup")
    }));
    std::fs::remove_dir_all(duplicate).unwrap();
}

#[test]
fn web_app_reserved_evidence_namespace_preserves_examples_route() {
    let root = project_root("framework-reserved-encodings");
    write_web_app(&root);
    std::fs::create_dir_all(root.join("app/examples")).unwrap();
    std::fs::write(
        root.join("app/examples/page.uhura"),
        r#"use uhura::ui;
use crate::program::App;

pub ui ExamplesPage for App(view) {
  <main>Examples</main>
}
"#,
    )
    .unwrap();
    let location = std::fs::read_to_string(root.join("location.uhura")).unwrap();
    std::fs::write(
        root.join("location.uhura"),
        location.replace("  Home,", "  Home,\n  Examples,"),
    )
    .unwrap();

    let resolved = resolve_clean(&root);
    let application = resolved.application().web_app.as_ref().unwrap();
    assert!(
        application
            .subjects
            .iter()
            .any(|subject| subject.logical == "app::examples")
    );
    assert!(
        application
            .subjects
            .iter()
            .any(|subject| subject.evidence_logical.as_deref() == Some("framework::evidence::app"))
    );
    let checked = resolved.check();
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    assert!(checked.program.is_some());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn web_app_check_enforces_discovered_evidence_roles_and_sibling_ownership() {
    let role = project_root("framework-evidence-role");
    write_web_app(&role);
    std::fs::write(
        role.join("components/post-card.examples.uhura"),
        r#"use crate::components::post_card::PostCard;

example post_card for PostCard(title: "Hello") as surface = crate::framework::evidence::app::home_scenario::frame;
"#,
    )
    .unwrap();
    let role_resolved = resolve_clean(&role);
    let role_checked = role_resolved.check();
    assert!(role_checked.program.is_none());
    assert!(role_checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/framework-evidence-role"
            && diagnostic.message.contains("declares `surface`")
            && diagnostic.message.contains("role is `component`")
            && role_resolved.source_map().path(diagnostic.span.file)
                == "components/post-card.examples.uhura"
    }));
    std::fs::remove_dir_all(role).unwrap();

    let sibling = project_root("framework-evidence-sibling");
    write_web_app(&sibling);
    std::fs::write(
        sibling.join("components/status-card.uhura"),
        r#"use uhura::ui;

pub ui StatusCard(label: Text) {
  <p>{label}</p>
}
"#,
    )
    .unwrap();
    std::fs::write(
        sibling.join("components/post-card.examples.uhura"),
        r#"use crate::components::status_card::StatusCard;

example post_card for StatusCard(label: "Other") as component = crate::framework::evidence::app::home_scenario::frame;
"#,
    )
    .unwrap();
    let sibling_checked = resolve_clean(&sibling).check();
    assert!(sibling_checked.program.is_none());
    assert!(sibling_checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/framework-evidence-role"
            && diagnostic.message.contains("colocated subject")
            && diagnostic.message.contains("PostCard")
            && diagnostic.message.contains("StatusCard")
    }));
    std::fs::remove_dir_all(sibling).unwrap();

    let undiscovered = project_root("framework-evidence-undiscovered");
    write_web_app(&undiscovered);
    let manifest = std::fs::read_to_string(undiscovered.join("uhura.toml")).unwrap();
    std::fs::write(
        undiscovered.join("uhura.toml"),
        manifest.replace(
            "location = \"location.uhura\"",
            "location = \"location.uhura\"\nlegacy = \"legacy.uhura\"",
        ),
    )
    .unwrap();
    std::fs::write(
        undiscovered.join("legacy.uhura"),
        r#"use uhura::ui;

pub ui LegacyCard(label: Text) {
  <p>{label}</p>
}
"#,
    )
    .unwrap();
    std::fs::write(
        undiscovered.join("components/post-card.examples.uhura"),
        r#"use crate::legacy::LegacyCard;

example post_card for LegacyCard(label: "Legacy") as component = crate::framework::evidence::app::home_scenario::frame;
"#,
    )
    .unwrap();
    let undiscovered_checked = resolve_clean(&undiscovered).check();
    assert!(undiscovered_checked.program.is_none());
    assert!(undiscovered_checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/framework-evidence-role"
            && diagnostic
                .message
                .contains("not a discovered page, component, or surface")
            && diagnostic.message.contains("LegacyCard")
    }));
    std::fs::remove_dir_all(undiscovered).unwrap();
}

#[test]
fn web_app_check_allows_explicit_shared_scenario_only_evidence() {
    let root = project_root("framework-shared-scenario");
    write_web_app(&root);
    let manifest = std::fs::read_to_string(root.join("uhura.toml")).unwrap();
    std::fs::write(
        root.join("uhura.toml"),
        format!("{manifest}\n[evidence.modules]\nshared = \"shared.evidence.uhura\"\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("shared.evidence.uhura"),
        r#"use crate::program::App;

scenario shared for App {
  start
  pin frame
}
"#,
    )
    .unwrap();

    let checked = resolve_clean(&root).check();
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    assert!(checked.program.is_some());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn web_app_rejects_invalid_dynamic_routes_and_orphan_evidence() {
    let malformed = project_root("malformed-route");
    write_web_app(&malformed);
    std::fs::create_dir_all(malformed.join("app/profile/[bad-name]")).unwrap();
    std::fs::write(
        malformed.join("app/profile/[bad-name]/page.uhura"),
        "use uhura::ui;\nuse crate::program::App;\npub ui BadPage for App(view) {<main/>}\n",
    )
    .unwrap();
    let rejection = resolve_project(&capture_project_snapshot(&malformed))
        .err()
        .expect("invalid route is rejected");
    assert!(
        rejection.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("[lower_snake]")
                && diagnostic
                    .message
                    .contains("app/profile/[bad-name]/page.uhura")
        }),
        "{:#?}",
        rejection.diagnostics
    );
    std::fs::remove_dir_all(malformed).unwrap();

    let orphan = project_root("orphan-evidence");
    write_web_app(&orphan);
    std::fs::write(
        orphan.join("components/missing.examples.uhura"),
        "use uhura::evidence;\n",
    )
    .unwrap();
    let rejection = resolve_project(&capture_project_snapshot(&orphan))
        .err()
        .expect("orphan evidence is rejected");
    assert!(
        rejection
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("is orphaned"))
    );
    std::fs::remove_dir_all(orphan).unwrap();
}

#[test]
fn web_app_rejects_framework_sources_that_are_also_explicitly_mapped() {
    let root = project_root("explicit-duplicate");
    write_web_app(&root);
    let manifest = std::fs::read_to_string(root.join("uhura.toml")).unwrap();
    std::fs::write(
        root.join("uhura.toml"),
        manifest.replace(
            "location = \"location.uhura\"",
            "location = \"location.uhura\"\nhome = \"app/page.uhura\"",
        ),
    )
    .unwrap();
    let rejection = resolve_project(&capture_project_snapshot(&root))
        .err()
        .expect("explicit framework duplicate is rejected");
    assert!(rejection.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("must not also be mapped explicitly")
    }));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn web_app_rejects_route_shape_collisions_and_malformed_roles() {
    let collision = project_root("route-collision");
    write_web_app(&collision);
    std::fs::create_dir_all(collision.join("app/profile/[member]")).unwrap();
    std::fs::write(
        collision.join("app/profile/[member]/page.uhura"),
        r#"use uhura::ui;
use crate::program::App;

pub ui MemberPage for App(view) {
  <main>Member</main>
}
"#,
    )
    .unwrap();
    let rejection = resolve_project(&capture_project_snapshot(&collision))
        .err()
        .expect("ambiguous route shape is rejected");
    assert!(
        rejection
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("same match shape"))
    );
    std::fs::remove_dir_all(collision).unwrap();

    let malformed = project_root("malformed-role");
    write_web_app(&malformed);
    std::fs::write(
        malformed.join("components/post-card.uhura"),
        "use uhura::ui;\npub ui WrongName(title: Text) {<p>{title}</p>}\n",
    )
    .unwrap();
    let rejection = resolve_project(&capture_project_snapshot(&malformed))
        .err()
        .expect("role-name mismatch is rejected");
    assert!(rejection.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("must declare `PostCard`")
            && diagnostic.message.contains("found `WrongName`")
    }));
    std::fs::remove_dir_all(malformed).unwrap();
}

#[test]
fn web_app_requires_the_root_page_and_separate_machine_location_modules() {
    let missing = project_root("missing-root");
    write_web_app(&missing);
    std::fs::remove_file(missing.join("app/page.uhura")).unwrap();
    let rejection = resolve_project(&capture_project_snapshot(&missing))
        .err()
        .expect("missing root is rejected");
    assert!(
        rejection
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("requires root page"))
    );
    std::fs::remove_dir_all(missing).unwrap();

    let cycle = project_root("locator-cycle");
    write_web_app(&cycle);
    let manifest = std::fs::read_to_string(cycle.join("uhura.toml")).unwrap();
    std::fs::write(
        cycle.join("uhura.toml"),
        manifest.replace(
            "location = \"crate::location::Location\"",
            "location = \"crate::program::Location\"",
        ),
    )
    .unwrap();
    let rejection = resolve_project(&capture_project_snapshot(&cycle))
        .err()
        .expect("machine/location module cycle is rejected");
    assert!(rejection.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("must be declared outside framework.machine")
    }));
    std::fs::remove_dir_all(cycle).unwrap();
}

#[test]
fn unlisted_sources_are_rejected_before_checking() {
    let root = project_root("unlisted");
    write_project(&root, "");
    std::fs::write(root.join("stray.uhura"), MACHINE).unwrap();

    let snapshot = capture_project_snapshot(&root);
    let rejection = resolve_project(&snapshot)
        .err()
        .expect("project is rejected");
    assert!(rejection.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "UH2001"
            && diagnostic.rule == "contract/invalid-project"
            && diagnostic.message.contains("`stray.uhura` is not listed")
    }));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn vendored_dependencies_cannot_select_a_framework_profile() {
    let root = project_root("dependency-framework");
    write_project(
        &root,
        r#"[dependencies.shared]
package = "test.shared"
version = 1
path = "vendor/shared"
"#,
    );
    std::fs::create_dir_all(root.join("vendor/shared")).unwrap();
    std::fs::write(
        root.join("vendor/shared/uhura.toml"),
        r#"[project]
name = "test.shared"
version = 1
language = "0.4"

[framework]
profile = "web-app"
version = 1
machine = "crate::program::App"
location = "crate::location::Location"

[modules]
values = "values.uhura"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("vendor/shared/values.uhura"),
        "pub const INITIAL: Int = 0;\n",
    )
    .unwrap();
    std::fs::write(
        root.join("uhura.lock"),
        format!(
            r#"protocol = "uhura-lock/0"

[root]
package = "test.counter@1"
dependencies = {{ shared = "test.shared@1" }}

[[package]]
package = "test.shared@1"
source = {{ kind = "path", path = "vendor/shared" }}
integrity = "sha256:{}"
dependencies = {{}}
"#,
            "0".repeat(64)
        ),
    )
    .unwrap();

    let rejection = resolve_project(&capture_project_snapshot(&root))
        .err()
        .expect("vendored framework configuration is rejected");
    assert!(rejection.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("package.test.shared@1.manifest.framework")
            && diagnostic
                .message
                .contains("framework profiles are root-project configuration")
    }));

    std::fs::remove_dir_all(root).unwrap();
}
