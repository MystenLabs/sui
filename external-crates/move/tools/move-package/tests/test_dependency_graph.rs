// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use move_package::{
    resolution::{
        dependency_cache::DependencyCache,
        dependency_graph::{DependencyGraph, DependencyMode},
        lock_file::LockFile,
    },
    source_package::manifest_parser::parse_move_manifest_from_file,
};
use move_symbol_pool::Symbol;

macro_rules! assert_error_contains {
    ($err:expr, $sub:expr) => {
        let err = $err.to_string();
        let sub = $sub;
        assert!(err.contains(sub), "{}", err);
    };
}

#[test]
fn no_dep_graph() {
    let pkg = no_dep_test_package();

    let manifest = parse_move_manifest_from_file(&pkg).expect("Loading manifest");
    let mut dependency_cache = DependencyCache::new(/* skip_fetch_latest_git_deps */ true);
    let graph = DependencyGraph::new(&manifest, pkg, &mut dependency_cache, &mut std::io::sink())
        .expect("Creating DependencyGraph");

    assert!(
        graph.package_graph.contains_node(graph.root_package),
        "A graph for a package with no dependencies should still contain the root package",
    );

    assert_eq!(graph.topological_order(), vec![graph.root_package]);
}

#[test]
fn no_dep_graph_from_lock() {
    let pkg = no_dep_test_package();

    let snapshot = pkg.join("Move.locked");
    let graph = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        &mut File::open(&snapshot).expect("Opening snapshot"),
    )
    .expect("Reading DependencyGraph");

    assert!(
        graph.package_graph.contains_node(graph.root_package),
        "A graph for a package with no dependencies should still contain the root package",
    );

    assert_eq!(graph.topological_order(), vec![graph.root_package]);
}

#[test]
fn lock_file_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = one_dep_test_package();

    let snapshot = pkg.join("Move.locked");
    let commit = tmp.path().join("Move.lock");

    let graph = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        &mut File::open(&snapshot).expect("Opening snapshot"),
    )
    .expect("Reading DependencyGraph");

    let lock = graph
        .write_to_lock(tmp.path().to_path_buf())
        .expect("Writing DependencyGraph");

    lock.commit(&commit).expect("Committing lock file");

    let expect = fs::read_to_string(&snapshot).expect("Reading snapshot");
    let actual = fs::read_to_string(commit).expect("Reading committed lock");

    assert_eq!(
        expect, actual,
        "LockFile -> DependencyGraph -> LockFile roundtrip"
    );
}

#[test]
fn lock_file_missing_dependency() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = one_dep_test_package();

    let commit = tmp.path().join("Move.lock");
    let lock = LockFile::new(pkg.clone()).expect("Creating new lock file");

    // Write a reference to a dependency that there isn't package information for.
    writeln!(&*lock, r#"dependencies = [{{ name = "OtherDep" }}]"#).unwrap();
    lock.commit(&commit).expect("Writing partial lock file");

    let Err(err) = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        &mut File::open(&commit).expect("Opening empty lock file"),
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

    let manifest = parse_move_manifest_from_file(&pkg).expect("Loading manifest");
    let mut dependency_cache = DependencyCache::new(/* skip_fetch_latest_git_deps */ true);
    let graph = DependencyGraph::new(&manifest, pkg, &mut dependency_cache, &mut std::io::sink())
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
    let snapshot = pkg.join("Move.locked");

    let graph = DependencyGraph::read_from_lock(
        pkg,
        Symbol::from("Root"),
        &mut File::open(&snapshot).expect("Opening snapshot"),
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
        &mut A_LOCK.as_bytes(),
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("A"),
        &mut EMPTY_LOCK.as_bytes(),
    )
    .expect("Reading inner");

    assert!(outer
        .merge(
            Symbol::from("A"),
            Symbol::from("A"),
            inner,
            Symbol::from(""),
            &BTreeMap::new(),
        )
        .is_ok());

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
        &mut EMPTY_LOCK.as_bytes(),
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    // The `inner` graph describes more dependencies for `outer`'s root package.
    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        &mut A_LOCK.as_bytes(),
    )
    .expect("Reading inner");

    assert!(outer
        .merge(
            Symbol::from("Root"),
            Symbol::from("A"),
            inner,
            Symbol::from(""),
            &BTreeMap::new(),
        )
        .is_ok());

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
        &mut EMPTY_LOCK.as_bytes(),
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("OtherDep"),
        &mut EMPTY_LOCK.as_bytes(),
    )
    .expect("Reading inner");

    let Err(err) = outer.merge(Symbol::from("OtherDep"), Symbol::from("A"), inner, Symbol::from(""), &BTreeMap::new()) else {
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
        &mut A_LOCK.as_bytes(),
    )
    .expect("Reading outer");

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("A"),
        &mut EMPTY_LOCK.as_bytes(),
    )
    .expect("Reading inner");

    let Err(err) = outer.merge(Symbol::from("A"),Symbol::from("A"), inner, Symbol::from(""), &BTreeMap::new()) else {
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
        &mut AB_LOCK.as_bytes(),
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("B"),
        &mut A_LOCK.as_bytes(),
    )
    .expect("Reading inner");

    assert!(outer
        .merge(
            Symbol::from("B"),
            Symbol::from("A"),
            inner,
            Symbol::from(""),
            &BTreeMap::new(),
        )
        .is_ok());
}

#[test]
fn merge_overlapping_different_deps() {
    let tmp = tempfile::tempdir().unwrap();
    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        &mut A_DEP_B_LOCK.as_bytes(),
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("B"),
        &mut A_LOCK.as_bytes(),
    )
    .expect("Reading inner");

    let Err(err) = outer.merge(Symbol::from("B"),Symbol::from("A"), inner, Symbol::from(""), &BTreeMap::new()) else {
        panic!("Outer and inner mention package A which has different dependencies in both.");
    };

    assert_error_contains!(err, "Conflicting dependencies found");
}

#[test]
fn merge_cyclic() {
    let tmp = tempfile::tempdir().unwrap();
    let mut outer = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("Root"),
        &mut AB_LOCK.as_bytes(),
    )
    .expect("Reading outer");

    // Test only -- clear always deps because usually `merge` is used while the graph is being
    // built, not after it has been entirely read.
    outer.always_deps.clear();

    let inner = DependencyGraph::read_from_lock(
        tmp.path().to_path_buf(),
        Symbol::from("B"),
        &mut ROOT_LOCK.as_bytes(),
    )
    .expect("Reading inner");

    let Err(err) = outer.merge(Symbol::from("B"), Symbol::from("Root"), inner, Symbol::from(""), &BTreeMap::new()) else {
        panic!("Inner refers back to outer's root");
    };

    assert_error_contains!(err, "Conflicting dependencies found");
}

#[test]
fn immediate_dependencies() {
    let pkg = dev_dep_test_package();

    let manifest = parse_move_manifest_from_file(&pkg).expect("Loading manifest");
    let mut dependency_cache = DependencyCache::new(/* skip_fetch_latest_git_deps */ true);
    let graph = DependencyGraph::new(&manifest, pkg, &mut dependency_cache, &mut std::io::sink())
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
version = 0
"#;

const ROOT_LOCK: &str = r#"
[move]
version = 0
dependencies = [
    { name = "Root" },
]

[[move.package]]
name = "Root"
source = { local = "." }
"#;

const A_LOCK: &str = r#"
[move]
version = 0
dependencies = [
    { name = "A" },
]

[[move.package]]
name = "A"
source = { local = "./A" }
"#;

const AB_LOCK: &str = r#"
[move]
version = 0
dependencies = [
    { name = "A" },
    { name = "B" },
]

[[move.package]]
name = "A"
source = { local = "./A" }

[[move.package]]
name = "B"
source = { local = "./B" }
"#;

const A_DEP_B_LOCK: &str = r#"
[move]
version = 0
dependencies = [
    { name = "A" },
]

[[move.package]]
name = "A"
source = { local = "./A" }
dependencies = [
    { name = "B" },
]

[[move.package]]
name = "B"
source = { local = "./B" }
"#;
