// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use move_command_line_common::testing::read_insta_snapshot;
use move_package::{
    lock_file::LockFile,
    resolution::dependency_graph::{
        DependencyGraph, DependencyGraphBuilder, DependencyGraphInfo, DependencyMode,
    },
    source_package::{
        layout::SourcePackageLayout,
        parsed_manifest::{Dependencies, Dependency, DependencyKind, InternalDependency},
    },
};
use move_symbol_pool::Symbol;

macro_rules! assert_error_contains {
    ($err:expr, $sub:expr) => {
        let err = $err.to_string();
        let sub = $sub;
        assert!(err.contains(sub), "{}", err);
    };
}

fn snapshot_path(pkg: &Path, kind: &str) -> PathBuf {
    pkg.join(format!("Move@{kind}.snap"))
}

#[test]
fn no_dep_graph() {
    let pkg = no_dep_test_package();

    let manifest_string = std::fs::read_to_string(pkg.join(SourcePackageLayout::Manifest.path()))
        .expect("Loading manifest");
    let mut dep_graph_builder = DependencyGraphBuilder::new(
        /* skip_fetch_latest_git_deps */ true,
        std::io::sink(),
        tempfile::tempdir().unwrap().path().to_path_buf(),
        Dependencies::default(), /* implicit deps */
        /* force_lock_file */ false,
    );
    let (graph, _) = dep_graph_builder
        .get_graph(
            &DependencyKind::default(),
            pkg,
            manifest_string,
            /* lock_string_opt */ None,
        )
        .expect("Creating DependencyGraph");

    assert!(
        graph.package_graph.contains_node(graph.root_package_id),
        "A graph for a package with no dependencies should still contain the root package",
    );

    assert_eq!(graph.topological_order(), vec![graph.root_package_id]);
}

#[test]
fn no_dep_graph_from_lock() {
    let pkg = no_dep_test_package();

    let snapshot = snapshot_path(&pkg, "locked");
    let contents = read_insta_snapshot(snapshot).unwrap();
    let graph = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut contents.as_bytes(),
        None,
    )
    .expect("Reading DependencyGraph");

    assert!(
        graph.package_graph.contains_node(graph.root_package_id),
        "A graph for a package with no dependencies should still contain the root package",
    );

    assert_eq!(graph.topological_order(), vec![graph.root_package_id]);
}

#[test]
fn lock_file_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = one_dep_test_package();

    let snapshot = snapshot_path(&pkg, "locked");
    let contents = read_insta_snapshot(snapshot).unwrap();
    let commit = tmp.path().join("Move.lock");

    let graph = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut contents.as_bytes(),
        None,
    )
    .expect("Reading DependencyGraph");

    let lock = graph
        .write_to_lock(tmp.path().to_path_buf(), None)
        .expect("Writing DependencyGraph");

    lock.commit(&commit).expect("Committing lock file");

    let actual = fs::read_to_string(commit).expect("Reading committed lock");

    assert_eq!(
        contents,
        actual.trim(),
        "LockFile -> DependencyGraph -> LockFile roundtrip"
    );
}

#[test]
fn lock_file_missing_dependency() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = one_dep_test_package();

    let commit = tmp.path().join("Move.lock");
    let lock = LockFile::new(
        pkg.clone(),
        /* manifest_digest */ "42".to_string(),
        /* deps_digest */ "7".to_string(),
    )
    .expect("Creating new lock file");

    // Write a reference to a dependency that there isn't package information for.
    writeln!(
        &*lock,
        r#"dependencies = [{{ id = "OtherDep", name = "OtherDep" }}]"#
    )
    .unwrap();
    lock.commit(&commit).expect("Writing partial lock file");

    let Err(err) = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut File::open(&commit).expect("Opening empty lock file"),
        None,
    ) else {
        panic!("Expected reading dependencies to fail.");
    };

    let message = err.to_string();
    assert!(
        message.contains("No source found for package OtherDep, depended on by: Root"),
        "{message}",
    );
}

#[test]
fn always_deps() {
    let pkg = dev_dep_test_package();

    let manifest_string = std::fs::read_to_string(pkg.join(SourcePackageLayout::Manifest.path()))
        .expect("Loading manifest");
    let mut dep_graph_builder = DependencyGraphBuilder::new(
        /* skip_fetch_latest_git_deps */ true,
        std::io::sink(),
        tempfile::tempdir().unwrap().path().to_path_buf(),
        /* implicit_deps */ Dependencies::default(),
        /* force_lock_file */ false,
    );
    let (graph, _) = dep_graph_builder
        .get_graph(
            &DependencyKind::default(),
            pkg,
            manifest_string,
            /* lock_string_opt */ None,
        )
        .expect("Creating DependencyGraph");

    assert_eq!(
        graph.always_deps,
        BTreeSet::from([
            Symbol::from("Root"),
            Symbol::from("A"),
            Symbol::from("B"),
            Symbol::from("C"),
        ]),
    );
}

#[test]
fn always_deps_from_lock() {
    let pkg = dev_dep_test_package();
    let snapshot = snapshot_path(&pkg, "locked");
    let contents = read_insta_snapshot(snapshot).unwrap();

    let graph = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut contents.as_bytes(),
        None,
    )
    .expect("Creating DependencyGraph");

    assert_eq!(
        graph.always_deps,
        BTreeSet::from([
            Symbol::from("Root"),
            Symbol::from("A"),
            Symbol::from("B"),
            Symbol::from("C"),
        ]),
    );
}

#[test]
fn merge_simple() {
    let tmp = tempfile::tempdir().unwrap();
    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut A_LOCK.as_bytes(),
        None,
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("A"),
        Symbol::from("A"),
        &mut EMPTY_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner");

    let dep_graphs = BTreeMap::from([(
        Symbol::from("A"),
        DependencyGraphInfo::new(inner, DependencyMode::Always, false, false, None),
    )]);
    let dependencies = &BTreeMap::from([(
        Symbol::from("A"),
        Dependency::Internal(InternalDependency {
            kind: DependencyKind::default(),
            subst: None,
            digest: None,
            dep_override: false,
        }),
    )]);
    let orig_names: BTreeMap<Symbol, Symbol> = dependencies.keys().map(|k| (*k, *k)).collect();
    assert!(
        outer
            .merge(
                dep_graphs,
                &DependencyKind::default(),
                dependencies,
                &BTreeMap::new(),
                &orig_names,
                Symbol::from("Root")
            )
            .is_ok(),
    );
    assert_eq!(
        outer.topological_order(),
        vec![Symbol::from("Root"), Symbol::from("A")],
    );
}
#[test]
fn merge_into_root() {
    let tmp = tempfile::tempdir().unwrap();
    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut EMPTY_LOCK.as_bytes(),
        None,
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    // The `inner` graph describes more dependencies for `outer`'s root package.
    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut A_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner");

    let dep_graphs = BTreeMap::from([(
        Symbol::from("A"),
        DependencyGraphInfo::new(inner, DependencyMode::Always, false, false, None),
    )]);
    let dependencies = &BTreeMap::from([(
        Symbol::from("A"),
        Dependency::Internal(InternalDependency {
            kind: DependencyKind::Local("A".into()),
            subst: None,
            digest: None,
            dep_override: false,
        }),
    )]);
    let orig_names: BTreeMap<Symbol, Symbol> = dependencies.keys().map(|k| (*k, *k)).collect();
    assert!(
        outer
            .merge(
                dep_graphs,
                &DependencyKind::default(),
                dependencies,
                &BTreeMap::new(),
                &orig_names,
                Symbol::from("Root")
            )
            .is_ok()
    );

    assert_eq!(
        outer.topological_order(),
        vec![Symbol::from("Root"), Symbol::from("A")],
    );
}

#[test]
fn merge_detached() {
    let tmp = tempfile::tempdir().unwrap();
    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut EMPTY_LOCK.as_bytes(),
        None,
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("OtherDep"),
        Symbol::from("OtherDep"),
        &mut EMPTY_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner");

    let dep_graphs = BTreeMap::from([(
        Symbol::from("OtherDep"),
        DependencyGraphInfo::new(inner, DependencyMode::Always, false, false, None),
    )]);
    let orig_names: BTreeMap<Symbol, Symbol> = dep_graphs.keys().map(|k| (*k, *k)).collect();
    let Err(err) = outer.merge(
        dep_graphs,
        &DependencyKind::default(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &orig_names,
        Symbol::from("Root"),
    ) else {
        panic!("Inner's root is not part of outer's graph, so this should fail");
    };

    assert_error_contains!(err, "Can't merge dependencies for 'OtherDep'");
}

#[test]
fn merge_after_calculating_always_deps() {
    let tmp = tempfile::tempdir().unwrap();
    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut A_LOCK.as_bytes(),
        None,
    )
    .expect("Reading outer");

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("A"),
        Symbol::from("A"),
        &mut EMPTY_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner");

    let dep_graphs = BTreeMap::from([(
        Symbol::from("A"),
        DependencyGraphInfo::new(inner, DependencyMode::Always, false, false, None),
    )]);
    let orig_names: BTreeMap<Symbol, Symbol> = dep_graphs.keys().map(|k| (*k, *k)).collect();
    let Err(err) = outer.merge(
        dep_graphs,
        &DependencyKind::default(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &orig_names,
        Symbol::from("Root"),
    ) else {
        panic!("Outer's always deps have already been calculated so this should fail");
    };

    assert_error_contains!(err, "after calculating its 'always' dependencies");
}

#[test]
fn merge_overlapping() {
    let tmp = tempfile::tempdir().unwrap();

    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut EMPTY_LOCK.as_bytes(),
        None,
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner1 = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("C"),
        Symbol::from("C"),
        &mut AB_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner1");

    let inner2 = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("C"),
        Symbol::from("C"),
        &mut A_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner2");

    let dep_graphs = BTreeMap::from([
        (
            Symbol::from("B"),
            DependencyGraphInfo::new(inner1, DependencyMode::Always, false, false, None),
        ),
        (
            Symbol::from("C"),
            DependencyGraphInfo::new(inner2, DependencyMode::Always, false, false, None),
        ),
    ]);
    let dependencies = &BTreeMap::from([
        (
            Symbol::from("B"),
            Dependency::Internal(InternalDependency {
                kind: DependencyKind::Local("B".into()),
                subst: None,
                digest: None,
                dep_override: false,
            }),
        ),
        (
            Symbol::from("C"),
            Dependency::Internal(InternalDependency {
                kind: DependencyKind::default(),
                subst: None,
                digest: None,
                dep_override: false,
            }),
        ),
    ]);
    let orig_names: BTreeMap<Symbol, Symbol> = dependencies.keys().map(|k| (*k, *k)).collect();
    assert!(
        outer
            .merge(
                dep_graphs,
                &DependencyKind::default(),
                dependencies,
                &BTreeMap::new(),
                &orig_names,
                Symbol::from("Root")
            )
            .is_ok()
    );
}

#[test]
fn merge_overlapping_different_deps() {
    let tmp = tempfile::tempdir().unwrap();

    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        Symbol::from("Root"),
        &mut EMPTY_LOCK.as_bytes(),
        None,
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner1 = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("C"),
        Symbol::from("C"),
        &mut A_DEP_B_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner1");

    let inner2 = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("C"),
        Symbol::from("C"),
        &mut A_LOCK.as_bytes(),
        None,
    )
    .expect("Reading inner2");

    let dep_graphs = BTreeMap::from([
        (
            Symbol::from("B"),
            DependencyGraphInfo::new(inner1, DependencyMode::Always, false, false, None),
        ),
        (
            Symbol::from("C"),
            DependencyGraphInfo::new(inner2, DependencyMode::Always, false, false, None),
        ),
    ]);
    let dependencies = &BTreeMap::from([
        (
            Symbol::from("B"),
            Dependency::Internal(InternalDependency {
                kind: DependencyKind::default(),
                subst: None,
                digest: None,
                dep_override: false,
            }),
        ),
        (
            Symbol::from("C"),
            Dependency::Internal(InternalDependency {
                kind: DependencyKind::default(),
                subst: None,
                digest: None,
                dep_override: false,
            }),
        ),
    ]);
    let orig_names: BTreeMap<Symbol, Symbol> = dependencies.keys().map(|k| (*k, *k)).collect();
    let Err(err) = outer.merge(
        dep_graphs,
        &DependencyKind::default(),
        dependencies,
        &BTreeMap::new(),
        &orig_names,
        Symbol::from("Root"),
    ) else {
        panic!("Outer and inner mention package A which has different dependencies in both.");
    };

    assert_error_contains!(err, "conflicting dependencies found");
}

#[test]
fn immediate_dependencies() {
    let pkg = dev_dep_test_package();

    let manifest_string = std::fs::read_to_string(pkg.join(SourcePackageLayout::Manifest.path()))
        .expect("Loading manifest");
    let mut dep_graph_builder = DependencyGraphBuilder::new(
        /* skip_fetch_latest_git_deps */ true,
        std::io::sink(),
        tempfile::tempdir().unwrap().path().to_path_buf(),
        /* implicit_deps */ Dependencies::default(),
        /* force_lock_file */ false,
    );
    let (graph, _) = dep_graph_builder
        .get_graph(
            &DependencyKind::default(),
            pkg,
            manifest_string,
            /* lock_string_opt */ None,
        )
        .expect("Creating DependencyGraph");

    let r = Symbol::from("Root");
    let a = Symbol::from("A");
    let b = Symbol::from("B");
    let c = Symbol::from("C");
    let d = Symbol::from("D");

    let deps = |pkg, mode| {
        graph
            .immediate_dependencies(pkg, mode)
            .map(|(pkg, _, _)| pkg)
            .collect::<BTreeSet<_>>()
    };

    assert_eq!(deps(r, DependencyMode::Always), BTreeSet::from([a, c]));
    assert_eq!(deps(a, DependencyMode::Always), BTreeSet::from([b]));
    assert_eq!(deps(b, DependencyMode::Always), BTreeSet::from([]));
    assert_eq!(deps(c, DependencyMode::Always), BTreeSet::from([]));
    assert_eq!(deps(d, DependencyMode::Always), BTreeSet::from([]));

    assert_eq!(deps(r, DependencyMode::DevOnly), BTreeSet::from([a, b, c]));
    assert_eq!(deps(a, DependencyMode::DevOnly), BTreeSet::from([b, d]));
    assert_eq!(deps(b, DependencyMode::DevOnly), BTreeSet::from([c]));
    assert_eq!(deps(c, DependencyMode::DevOnly), BTreeSet::from([]));
    assert_eq!(deps(d, DependencyMode::DevOnly), BTreeSet::from([]));
}

fn no_dep_test_package() -> PathBuf {
    [".", "tests", "test_sources", "basic_no_deps"]
        .into_iter()
        .collect()
}

fn one_dep_test_package() -> PathBuf {
    [".", "tests", "test_sources", "one_dep"]
        .into_iter()
        .collect()
}

fn dev_dep_test_package() -> PathBuf {
    [".", "tests", "test_sources", "dep_dev_dep_diamond"]
        .into_iter()
        .collect()
}

const EMPTY_LOCK: &str = r#"
[move]
version = 3
manifest_digest = "42"
deps_digest = ""
"#;

const A_LOCK: &str = r#"
[move]
version = 3
manifest_digest = "42"
deps_digest = "7"
dependencies = [
    { id = "A", name = "A" },
]

[[move.package]]
id = "A"
source = { local = "./A" }
"#;

const AB_LOCK: &str = r#"
[move]
version = 3
manifest_digest = "42"
deps_digest = "7"
dependencies = [
    { id = "A", name = "A" },
    { id = "B", name = "A" },
]

[[move.package]]
id = "A"
source = { local = "./A" }

[[move.package]]
id = "B"
source = { local = "./B" }
"#;

const A_DEP_B_LOCK: &str = r#"
[move]
version = 3
manifest_digest = "42"
deps_digest = "7"
dependencies = [
    { id = "A", name = "A" },
]

[[move.package]]
id = "A"
source = { local = "./A" }
dependencies = [
    { id = "B", name = "A" },
]

[[move.package]]
id = "B"
source = { local = "./B" }
"#;

#[test]
fn warn_explicit_sui_dependencies() {
    use std::io::Cursor;
    
    // Create a temporary directory for the test
    let tmp_dir = tempfile::tempdir().unwrap();
    let pkg_path = tmp_dir.path().join("test_pkg");
    std::fs::create_dir_all(&pkg_path).unwrap();
    
    // Create a Move.toml with explicit Sui dependencies of different types
    let manifest_content = r#"
[package]
name = "TestPackage"
edition = "2024.beta"

[dependencies]
Sui = { local = "../sui-framework" }

[dev-dependencies]
SuiSystem = { local = "../sui-system" }
"#;
    
    std::fs::write(
        pkg_path.join("Move.toml"),
        manifest_content,
    ).unwrap();
    
    // Capture progress output (warnings are written to progress_output, not stderr)
    let mut progress_output = Vec::new();
    {
        let mut dep_graph_builder = DependencyGraphBuilder::new(
            /* skip_fetch_latest_git_deps */ true,
            Cursor::new(&mut progress_output),
            tmp_dir.path().to_path_buf(),
            Dependencies::default(), /* implicit deps */
            /* force_lock_file */ false,
        );
        
        // This should trigger warnings for explicit Sui dependencies
        let result = dep_graph_builder.get_graph(
            &DependencyKind::default(),
            pkg_path,
            manifest_content.to_string(),
            /* lock_string_opt */ None,
        );
        
        // The graph creation might fail due to missing dependencies, but that's OK
        // We just want to test that the warning is emitted
        let _ = result;
    }
    
    let progress_str = String::from_utf8(progress_output).unwrap();
    println!("Progress output: {}", progress_str);
    
    // Check that warnings were emitted for explicit Sui dependencies
    assert!(progress_str.contains("Found explicit Sui dependencies"));
    assert!(progress_str.contains("Sui"));
    assert!(progress_str.contains("SuiSystem"));
    assert!(progress_str.contains("Consider using implicit dependencies"));
}

#[test]
fn warn_explicit_sui_dependencies_in_nested_packages() {
    use std::io::Cursor;
    
    // Create a temporary directory structure for testing nested packages
    let tmp_dir = tempfile::tempdir().unwrap();
    let root_pkg_path = tmp_dir.path().join("root_pkg");
    let nested_pkg_path = tmp_dir.path().join("nested_pkg");
    std::fs::create_dir_all(&root_pkg_path).unwrap();
    std::fs::create_dir_all(&nested_pkg_path).unwrap();
    
    // Create a nested package with Sui dependencies
    let nested_manifest_content = r#"
[package]
name = "NestedPackage"
edition = "2024.beta"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework/packages/sui-framework", rev = "main" }
"#;
    
    std::fs::write(
        nested_pkg_path.join("Move.toml"),
        nested_manifest_content,
    ).unwrap();
    
    // Create root package that depends on the nested package
    let root_manifest_content = format!(
        r#"
[package]
name = "RootPackage"
edition = "2024.beta"

[dependencies]
NestedPackage = {{ local = "{}" }}
"#,
        nested_pkg_path.to_string_lossy()
    );
    
    std::fs::write(
        root_pkg_path.join("Move.toml"),
        &root_manifest_content,
    ).unwrap();
    
    // Capture progress output to check for warnings from nested packages
    let mut progress_output = Vec::new();
    {
        let mut dep_graph_builder = DependencyGraphBuilder::new(
            /* skip_fetch_latest_git_deps */ true,
            Cursor::new(&mut progress_output),
            tmp_dir.path().to_path_buf(),
            Dependencies::default(), /* implicit deps */
            /* force_lock_file */ false,
        );
        
        // This should trigger warnings for Sui dependencies in both root and nested packages
        let result = dep_graph_builder.get_graph(
            &DependencyKind::default(),
            root_pkg_path,
            root_manifest_content,
            /* lock_string_opt */ None,
        );
        
        // The graph creation might fail due to missing dependencies, but that's OK
        // We just want to test that warnings are emitted for nested packages too
        let _ = result;
    }
    
    let progress_str = String::from_utf8(progress_output).unwrap();
    println!("Progress output for nested test: {}", progress_str);
    
    // Check that warnings were emitted for Sui dependencies in nested packages
    assert!(progress_str.contains("Found explicit Sui dependencies"));
    assert!(progress_str.contains("NestedPackage"));
    assert!(progress_str.contains("Sui"));
    assert!(progress_str.contains("internal package"));
}
