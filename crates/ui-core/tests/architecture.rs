//! The workspace's layering (AGENTS.md, docs/architecture.md §4), read from each crate's
//! `Cargo.toml`: internal dependencies point one way only, and the crates below
//! `ui-render-wgpu` / `ui-platform` stay free of the GPU and the window system.

use std::collections::BTreeSet;
use std::path::Path;

/// Each library crate and the workspace crates it may depend on.
const ALLOWED: [(&str, &[&str]); 6] = [
    ("ui-scene", &[]),
    ("ui-text", &["ui-scene"]),
    ("ui-layout", &["ui-scene"]),
    ("ui-core", &["ui-scene", "ui-text", "ui-layout"]),
    ("ui-render-wgpu", &["ui-scene", "ui-text"]),
    (
        "ui-platform",
        &[
            "ui-scene",
            "ui-text",
            "ui-layout",
            "ui-core",
            "ui-render-wgpu",
        ],
    ),
];

/// Crates that must not reach the GPU, the window system or the browser.
const GPU_FREE: [&str; 4] = ["ui-scene", "ui-text", "ui-layout", "ui-core"];
const GPU_AND_WINDOW: [&str; 6] = [
    "wgpu",
    "glyphon",
    "winit",
    "raw-window-handle",
    "web-sys",
    "wasm-bindgen",
];

const WORKSPACE: [&str; 9] = [
    "ui-scene",
    "ui-text",
    "ui-layout",
    "ui-core",
    "ui-render-wgpu",
    "ui-platform",
    "demo",
    "desktop",
    "web",
];

/// The names of the normal and build dependencies in a `Cargo.toml` (every target),
/// leaving out dev-dependencies, which tests and benches may use freely.
fn dependencies(manifest: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut in_deps = false;
    for line in manifest.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']');
            // `[target.'cfg(..)'.dependencies.rfd]` names one dependency in its header.
            let (table, name) = match header.rsplit_once("dependencies.") {
                Some((table, name)) => (format!("{table}dependencies"), Some(name)),
                None => (header.to_owned(), None),
            };
            in_deps = table.ends_with("dependencies") && !table.ends_with("dev-dependencies");
            if let (true, Some(name)) = (in_deps, name) {
                names.insert(name.trim_matches('"').to_owned());
                in_deps = false;
            }
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.split(['=', '.', ' ']).next() {
            names.insert(name.trim_matches('"').to_owned());
        }
    }
    names
}

fn manifest(krate: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = if krate.starts_with("ui-") {
        root.join("crates").join(krate)
    } else {
        root.join("apps").join(krate)
    };
    let path = dir.join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn internal_dependencies_point_one_way() {
    for (krate, allowed) in ALLOWED {
        let deps = dependencies(&manifest(krate));
        let internal: Vec<_> = deps
            .iter()
            .filter(|d| WORKSPACE.contains(&d.as_str()))
            .filter(|d| !allowed.contains(&d.as_str()))
            .collect();
        assert!(
            internal.is_empty(),
            "{krate} depends on {internal:?}; it may only use {allowed:?}"
        );
    }
}

#[test]
fn core_crates_stay_free_of_gpu_and_window() {
    for krate in GPU_FREE {
        let deps = dependencies(&manifest(krate));
        let found: Vec<_> = GPU_AND_WINDOW
            .iter()
            .filter(|d| deps.contains(**d))
            .collect();
        assert!(found.is_empty(), "{krate} depends on {found:?}");
    }
}

#[test]
fn reads_every_dependency_form() {
    let deps = dependencies(
        r#"
[package]
name = "x"

[dependencies]
ui-scene.workspace = true
taffy = "0.15"
"quoted" = { version = "1" }

[dev-dependencies]
pollster.workspace = true

[target.'cfg(target_os = "linux")'.dependencies]
ksni = { workspace = true }

[target.'cfg(target_os = "linux")'.dependencies.rfd]
workspace = true

[build-dependencies]
cc = "1"
"#,
    );
    let expected = ["cc", "ksni", "quoted", "rfd", "taffy", "ui-scene"];
    assert_eq!(deps.into_iter().collect::<Vec<_>>(), expected);
}
