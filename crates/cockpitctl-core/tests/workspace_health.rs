//! Workspace health checks for cockpitctl.
//!
//! Validates structural invariants: consistent metadata, acyclic dependency
//! graph, correct facade re-exports, feature-flag propagation, and schema
//! synchronisation between canonical and embedded copies.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

// ─── Helpers ───────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

fn read_toml(path: &Path) -> toml::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let table: toml::Table =
        toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    toml::Value::Table(table)
}

fn root_manifest() -> toml::Value {
    read_toml(&workspace_root().join("Cargo.toml"))
}

fn member_paths() -> Vec<String> {
    root_manifest()["workspace"]["members"]
        .as_array()
        .expect("workspace.members")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

struct MemberInfo {
    path: String,
    name: String,
    manifest: toml::Value,
}

fn all_members() -> Vec<MemberInfo> {
    member_paths()
        .into_iter()
        .map(|p| {
            let manifest = read_toml(&workspace_root().join(&p).join("Cargo.toml"));
            let name = manifest["package"]["name"]
                .as_str()
                .expect("package.name")
                .to_string();
            MemberInfo {
                path: p,
                name,
                manifest,
            }
        })
        .collect()
}

/// Returns `true` when a TOML value represents `field.workspace = true`.
fn is_workspace_inherited(val: &toml::Value) -> bool {
    val.as_table()
        .and_then(|t| t.get("workspace"))
        .and_then(|v| v.as_bool())
        == Some(true)
}

/// Extracts workspace-internal dependency names from a TOML deps table.
fn internal_dep_names(deps: &toml::Value, all_names: &HashSet<String>) -> BTreeSet<String> {
    deps.as_table()
        .map(|t| {
            t.keys()
                .filter(|k| all_names.contains(k.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// DFS cycle detection on a directed graph.
fn has_cycle(graph: &BTreeMap<String, BTreeSet<String>>) -> bool {
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for start in graph.keys() {
        if !visited.contains(start) && dfs_finds_cycle(start, graph, &mut visited, &mut in_stack) {
            return true;
        }
    }
    false
}

fn dfs_finds_cycle(
    node: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
) -> bool {
    visited.insert(node.to_string());
    in_stack.insert(node.to_string());

    if let Some(deps) = graph.get(node) {
        for dep in deps {
            if in_stack.contains(dep) {
                return true;
            }
            if !visited.contains(dep) && dfs_finds_cycle(dep, graph, visited, in_stack) {
                return true;
            }
        }
    }

    in_stack.remove(node);
    false
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[test]
fn all_crates_have_edition_2024() {
    let root = root_manifest();
    let ws_edition = root["workspace"]["package"]["edition"]
        .as_str()
        .expect("workspace.package.edition");
    assert_eq!(ws_edition, "2024");

    for m in all_members() {
        let ed = &m.manifest["package"]["edition"];
        assert!(
            is_workspace_inherited(ed) || ed.as_str() == Some("2024"),
            "{}: edition must be workspace-inherited or \"2024\"",
            m.path,
        );
    }
}

#[test]
fn all_crates_have_rust_version_1_92() {
    let root = root_manifest();
    let ws_msrv = root["workspace"]["package"]["rust-version"]
        .as_str()
        .expect("workspace.package.rust-version");
    assert_eq!(ws_msrv, "1.92");

    for m in all_members() {
        let ok = m.manifest["package"]
            .get("rust-version")
            .is_some_and(|v| is_workspace_inherited(v) || v.as_str() == Some("1.92"));
        assert!(
            ok,
            "{}: rust-version must be workspace-inherited or \"1.92\"",
            m.path,
        );
    }
}

#[test]
fn all_crates_have_license() {
    assert!(
        root_manifest()["workspace"]["package"]["license"]
            .as_str()
            .is_some(),
        "workspace.package.license must be set",
    );

    for m in all_members() {
        let ok = m.manifest["package"]
            .get("license")
            .is_some_and(|v| is_workspace_inherited(v) || v.as_str().is_some());
        assert!(ok, "{}: license must be workspace-inherited or set", m.path);
    }
}

#[test]
fn no_circular_dependencies() {
    let members = all_members();
    let all_names: HashSet<String> = members.iter().map(|m| m.name.clone()).collect();

    let graph: BTreeMap<String, BTreeSet<String>> = members
        .iter()
        .map(|m| {
            let deps = m
                .manifest
                .get("dependencies")
                .map(|d| internal_dep_names(d, &all_names))
                .unwrap_or_default();
            (m.name.clone(), deps)
        })
        .collect();

    assert!(
        !has_cycle(&graph),
        "workspace dependency graph contains a cycle",
    );
}

#[test]
fn core_facade_reexports_all_library_crates() {
    let lib_rs = fs::read_to_string(workspace_root().join("crates/cockpitctl-core/src/lib.rs"))
        .expect("read cockpitctl-core/src/lib.rs");

    let core_toml = read_toml(&workspace_root().join("crates/cockpitctl-core/Cargo.toml"));
    let deps = core_toml["dependencies"]
        .as_table()
        .expect("[dependencies]");

    for key in deps.keys().filter(|k| k.starts_with("cockpitctl-")) {
        let ident = key.replace('-', "_");
        assert!(
            lib_rs.contains(&format!("pub use {ident}"))
                || lib_rs.contains(&format!("pub mod {ident}")),
            "cockpitctl-core must re-export {key} (as `{ident}`)",
        );
    }
}

#[test]
fn feature_flags_propagate_to_feature_state() {
    let fs_toml = read_toml(&workspace_root().join("crates/cockpitctl-feature-state/Cargo.toml"));
    let state_features: BTreeSet<&str> = fs_toml
        .get("features")
        .and_then(|f| f.as_table())
        .expect("feature-state [features]")
        .keys()
        .map(String::as_str)
        .filter(|k| *k != "default")
        .collect();

    let cli_toml = read_toml(&workspace_root().join("crates/cockpitctl-cli/Cargo.toml"));
    let cli_features = cli_toml
        .get("features")
        .and_then(|f| f.as_table())
        .expect("cli [features]");

    for flag in &state_features {
        let target = format!("cockpitctl-feature-state/{flag}");
        let propagated = cli_features.values().any(|v| {
            v.as_array()
                .is_some_and(|arr| arr.iter().any(|i| i.as_str() == Some(&target)))
        });
        assert!(
            propagated,
            "feature-state flag `{flag}` is not propagated by any CLI feature",
        );
    }
}

#[test]
fn all_crate_versions_match() {
    let root = root_manifest();
    let ws_ver = root["workspace"]["package"]["version"]
        .as_str()
        .expect("workspace.package.version");

    for m in all_members() {
        let ver = &m.manifest["package"]["version"];
        assert!(
            is_workspace_inherited(ver) || ver.as_str() == Some(ws_ver),
            "{}: version must be workspace-inherited or match \"{ws_ver}\"",
            m.path,
        );
    }
}

#[test]
fn no_duplicate_workspace_crate_versions() {
    let lockfile_path = workspace_root().join("Cargo.lock");
    let Ok(lockfile) = fs::read_to_string(&lockfile_path) else {
        eprintln!("Cargo.lock not found — skipping lockfile check");
        return;
    };
    let table: toml::Table = toml::from_str(&lockfile).expect("parse Cargo.lock");
    let lock = toml::Value::Table(table);
    let packages = lock["package"].as_array().expect("[[package]] array");

    let ws_names: HashSet<String> = all_members().into_iter().map(|m| m.name).collect();

    let mut seen: HashMap<String, Vec<String>> = HashMap::new();
    for pkg in packages {
        let name = pkg["name"].as_str().unwrap().to_string();
        let version = pkg["version"].as_str().unwrap().to_string();
        if ws_names.contains(&name) {
            seen.entry(name).or_default().push(version);
        }
    }

    for (name, versions) in &seen {
        assert_eq!(
            versions.len(),
            1,
            "{name} appears {} times in Cargo.lock: {versions:?}",
            versions.len(),
        );
    }
}

#[test]
fn embedded_schemas_match_canonical() {
    let root = workspace_root();
    let canonical = root.join("contracts/schemas");
    let embedded = root.join("crates/cockpitctl-types/schemas");

    let json_names = |dir: &Path| -> BTreeSet<String> {
        fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };

    let canon_set = json_names(&canonical);
    let embed_set = json_names(&embedded);
    assert_eq!(canon_set, embed_set, "schema file sets must match");

    for name in &canon_set {
        let a = fs::read_to_string(canonical.join(name)).expect("read canonical");
        let b = fs::read_to_string(embedded.join(name)).expect("read embedded");
        assert_eq!(
            a, b,
            "schema `{name}` is out of sync — run `cargo run -p xtask -- schema-sync-fix`",
        );
    }
}
