//! Workspace validation — structural correctness of the workspace manifest,
//! valid publish order, absence of dev-dependency cycles, and publish-readiness
//! checks (metadata completeness, release tier ordering, package hygiene).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

// ─── Helpers ───────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
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

fn declared_members() -> Vec<String> {
    let root = read_toml(&workspace_root().join("Cargo.toml"));
    root["workspace"]["members"]
        .as_array()
        .expect("workspace.members")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

struct MemberInfo {
    name: String,
    manifest: toml::Value,
}

fn all_members() -> Vec<MemberInfo> {
    let root = workspace_root();
    declared_members()
        .into_iter()
        .map(|p| {
            let manifest = read_toml(&root.join(&p).join("Cargo.toml"));
            let name = manifest["package"]["name"]
                .as_str()
                .expect("package.name")
                .to_string();
            MemberInfo { name, manifest }
        })
        .collect()
}

fn internal_dep_names(table: &toml::Value, all_names: &HashSet<String>) -> BTreeSet<String> {
    table
        .as_table()
        .map(|t| {
            t.keys()
                .filter(|k| all_names.contains(k.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
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
fn workspace_members_lists_all_crate_dirs() {
    let root = workspace_root();
    let declared: BTreeSet<String> = declared_members().into_iter().collect();

    let mut discovered = BTreeSet::new();
    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for entry in fs::read_dir(&crates_dir).expect("read crates/") {
            let entry = entry.expect("dir entry");
            if entry.path().join("Cargo.toml").is_file() {
                discovered.insert(format!("crates/{}", entry.file_name().to_string_lossy()));
            }
        }
    }
    if root.join("xtask/Cargo.toml").is_file() {
        discovered.insert("xtask".to_string());
    }

    let missing: Vec<_> = discovered.difference(&declared).collect();
    let extra: Vec<_> = declared.difference(&discovered).collect();

    assert!(
        missing.is_empty(),
        "crate dirs not in workspace.members: {missing:?}",
    );
    assert!(
        extra.is_empty(),
        "workspace.members without matching dirs: {extra:?}",
    );
}

#[test]
fn publish_order_is_valid() {
    let members = all_members();
    let all_names: HashSet<String> = members.iter().map(|m| m.name.clone()).collect();

    let publishable: BTreeSet<String> = members
        .iter()
        .filter(|m| {
            m.manifest["package"]
                .get("publish")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        })
        .map(|m| m.name.clone())
        .collect();

    // Build dep graph (name → workspace deps) for publishable crates.
    let mut deps_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &members {
        if !publishable.contains(&m.name) {
            continue;
        }
        let deps = m
            .manifest
            .get("dependencies")
            .map(|d| {
                internal_dep_names(d, &all_names)
                    .into_iter()
                    .filter(|d| publishable.contains(d))
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        deps_of.insert(m.name.clone(), deps);
    }

    // Kahn's topological sort — each dep must be published before its dependent.
    let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();
    for name in &publishable {
        in_degree.insert(name.clone(), 0);
    }
    let mut reverse: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, deps) in &deps_of {
        for dep in deps {
            *in_degree.entry(name.clone()).or_insert(0) += 1;
            reverse.entry(dep.clone()).or_default().push(name.clone());
        }
    }

    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|entry| *entry.1 == 0)
        .map(|entry| entry.0.clone())
        .collect();
    let mut sorted = 0usize;

    while let Some(node) = queue.pop_front() {
        sorted += 1;
        if let Some(dependents) = reverse.get(&node) {
            for dependent in dependents {
                if let Some(d) = in_degree.get_mut(dependent) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }
    }

    assert_eq!(
        sorted,
        publishable.len(),
        "no valid publish order — cycle among publishable crates ({sorted}/{})",
        publishable.len(),
    );
}

#[test]
fn no_dev_dependency_cycles() {
    let members = all_members();
    let all_names: HashSet<String> = members.iter().map(|m| m.name.clone()).collect();

    let graph: BTreeMap<String, BTreeSet<String>> = members
        .iter()
        .map(|m| {
            let mut deps = BTreeSet::new();
            for section in ["dependencies", "dev-dependencies"] {
                if let Some(table) = m.manifest.get(section) {
                    deps.extend(internal_dep_names(table, &all_names));
                }
            }
            (m.name.clone(), deps)
        })
        .collect();

    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for start in graph.keys() {
        if !visited.contains(start) {
            assert!(
                !dfs_finds_cycle(start, &graph, &mut visited, &mut in_stack),
                "dev-dependency cycle detected involving `{start}`",
            );
        }
    }
}

// ─── Publish-readiness helpers ─────────────────────────────────────────────

fn workspace_toml() -> toml::Value {
    read_toml(&workspace_root().join("Cargo.toml"))
}

fn publishable_members() -> Vec<MemberInfo> {
    all_members()
        .into_iter()
        .filter(|m| {
            m.manifest["package"]
                .get("publish")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        })
        .collect()
}

/// Resolve a field that may be inherited via `field.workspace = true`.
fn resolve_field<'a>(
    pkg: &'a toml::Value,
    field: &str,
    ws_pkg: &'a toml::Value,
) -> Option<&'a toml::Value> {
    match pkg.get(field) {
        Some(v) if v.is_table() => {
            if v.get("workspace").and_then(|w| w.as_bool()).unwrap_or(false) {
                ws_pkg.get(field)
            } else {
                Some(v)
            }
        }
        other => other,
    }
}

// ─── Metadata completeness ────────────────────────────────────────────────

#[test]
fn publishable_crates_have_complete_metadata() {
    let ws = workspace_toml();
    let ws_pkg = &ws["workspace"]["package"];
    let required = [
        "version",
        "edition",
        "license",
        "rust-version",
        "repository",
        "authors",
        "description",
    ];

    let mut missing = Vec::new();

    for m in publishable_members() {
        let pkg = &m.manifest["package"];
        for &field in &required {
            if resolve_field(pkg, field, ws_pkg).is_none() {
                missing.push(format!("{}: missing `{field}`", m.name));
            }
        }
        if let Some(desc) = resolve_field(pkg, "description", ws_pkg) {
            let s = desc.as_str().unwrap_or("");
            if s.is_empty() || s.len() < 10 {
                missing.push(format!("{}: description too short", m.name));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "metadata gaps in publishable crates:\n  {}",
        missing.join("\n  "),
    );
}

#[test]
fn publishable_crates_have_keywords_and_categories() {
    let ws = workspace_toml();
    let ws_pkg = &ws["workspace"]["package"];
    let mut issues = Vec::new();

    for m in publishable_members() {
        let pkg = &m.manifest["package"];

        let kw = resolve_field(pkg, "keywords", ws_pkg);
        match kw {
            Some(v) => {
                let arr = v.as_array().expect("keywords should be array");
                if arr.is_empty() {
                    issues.push(format!("{}: keywords is empty", m.name));
                }
                if arr.len() > 5 {
                    issues.push(format!(
                        "{}: keywords exceeds crates.io limit of 5",
                        m.name,
                    ));
                }
            }
            None => issues.push(format!("{}: missing keywords", m.name)),
        }

        let cat = resolve_field(pkg, "categories", ws_pkg);
        match cat {
            Some(v) => {
                let arr = v.as_array().expect("categories should be array");
                if arr.is_empty() {
                    issues.push(format!("{}: categories is empty", m.name));
                }
                if arr.len() > 5 {
                    issues.push(format!(
                        "{}: categories exceeds crates.io limit of 5",
                        m.name,
                    ));
                }
            }
            None => issues.push(format!("{}: missing categories", m.name)),
        }
    }

    assert!(
        issues.is_empty(),
        "keyword/category issues:\n  {}",
        issues.join("\n  "),
    );
}

#[test]
fn publishable_crate_descriptions_are_unique() {
    let ws = workspace_toml();
    let ws_pkg = &ws["workspace"]["package"];
    let mut seen: HashMap<String, Vec<String>> = HashMap::new();

    for m in publishable_members() {
        let pkg = &m.manifest["package"];
        if let Some(desc) = resolve_field(pkg, "description", ws_pkg) {
            let s = desc.as_str().unwrap_or("").to_string();
            seen.entry(s).or_default().push(m.name.clone());
        }
    }

    let dupes: Vec<_> = seen
        .iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(desc, names)| format!("{names:?} share description: {desc:?}"))
        .collect();

    assert!(
        dupes.is_empty(),
        "duplicate descriptions among publishable crates:\n  {}",
        dupes.join("\n  "),
    );
}

// ─── Release workflow tier ordering ────────────────────────────────────────

#[test]
fn release_workflow_tier_ordering_matches_deps() {
    let workflow_path = workspace_root().join(".github/workflows/release.yml");
    let content = fs::read_to_string(&workflow_path).expect("read release.yml");

    // Extract crate names in publication order from `cargo publish -p` steps.
    let mut publish_order: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("run: cargo publish -p ")
            && !trimmed.contains("--dry-run")
        {
            let crate_name = trimmed
                .strip_prefix("run: cargo publish -p ")
                .unwrap()
                .trim();
            publish_order.push(crate_name.to_string());
        }
    }

    assert!(
        !publish_order.is_empty(),
        "could not parse any publish steps from release.yml",
    );

    // Build dep graph for publishable crates.
    let members = all_members();
    let all_names: HashSet<String> = members.iter().map(|m| m.name.clone()).collect();
    let deps_of: HashMap<String, BTreeSet<String>> = members
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

    // When a crate is published, all its deps must already have been published.
    let mut published: HashSet<String> = HashSet::new();
    let mut violations = Vec::new();

    for crate_name in &publish_order {
        if let Some(deps) = deps_of.get(crate_name) {
            for dep in deps {
                if !published.contains(dep) {
                    violations.push(format!(
                        "`{crate_name}` published before dependency `{dep}`",
                    ));
                }
            }
        }
        published.insert(crate_name.clone());
    }

    assert!(
        violations.is_empty(),
        "release tier ordering violations:\n  {}",
        violations.join("\n  "),
    );
}

#[test]
fn release_workflow_publishes_all_publishable_crates() {
    let workflow_path = workspace_root().join(".github/workflows/release.yml");
    let content = fs::read_to_string(&workflow_path).expect("read release.yml");

    let mut published_crates: BTreeSet<String> = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("run: cargo publish -p ")
            && !trimmed.contains("--dry-run")
        {
            let name = trimmed
                .strip_prefix("run: cargo publish -p ")
                .unwrap()
                .trim()
                .to_string();
            published_crates.insert(name);
        }
    }

    let publishable: BTreeSet<String> = publishable_members()
        .iter()
        .map(|m| m.name.clone())
        .collect();

    let missing: Vec<_> = publishable.difference(&published_crates).collect();
    let extra: Vec<_> = published_crates.difference(&publishable).collect();

    assert!(
        missing.is_empty(),
        "publishable crates not in release.yml: {missing:?}",
    );
    assert!(
        extra.is_empty(),
        "crates in release.yml but not publishable: {extra:?}",
    );
}

// ─── Package hygiene ──────────────────────────────────────────────────────

#[test]
fn publishable_crates_have_readme() {
    let ws = workspace_toml();
    let ws_pkg = &ws["workspace"]["package"];
    let mut missing = Vec::new();

    for m in publishable_members() {
        let pkg = &m.manifest["package"];
        if resolve_field(pkg, "readme", ws_pkg).is_none() {
            missing.push(m.name.clone());
        }
    }

    assert!(
        missing.is_empty(),
        "publishable crates missing readme field: {missing:?}",
    );
}