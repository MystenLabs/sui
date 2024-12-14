// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow;
use cli_sandbox::Project;
use insta::{assert_debug_snapshot, assert_snapshot};
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::io::Read;
use std::path::Path;
use std::{fs, fs::File, path::PathBuf, process::Command};

/// Note: tests use [insta] and [insta-cmd]; you can review and update expected test results using
/// cargo insta test --review

/// # Test infrastructure //////////////////////////////////////////////////////////////////////////

/// create a `sui-move` Command that runs in a new temporary directory
fn sui_move() -> (Project, Command) {
    let project = Project::new().unwrap();
    let mut cmd = Command::new(get_cargo_bin("sui-move"));
    cmd.current_dir(project.path());
    (project, cmd)
}

/// create a file in `[proj]/[path]` containing `[contents]`
fn new_file<P: AsRef<Path>>(proj: &mut Project, path: P, contents: &str) -> anyhow::Result<()> {
    let fullpath = proj.path().join(&path);
    let parent = fullpath
        .parent()
        .ok_or_else(|| anyhow::anyhow!("joined path is expected to have a parent"))?;
    fs::create_dir_all(parent)?;
    proj.new_file(path, contents)
}

/// return the contents of the file `[proj]/[path]`
fn slurp_file<P: AsRef<Path>>(proj: &Project, path: P) -> anyhow::Result<String> {
    let mut buf: String = String::new();
    File::open(proj.path().join(path))?.read_to_string(&mut buf)?;
    Ok(buf)
}

/// return the set of files (and directories) recursively contained in the directory at [root].
/// Results are normalized relative to [root] and returned in sorted order.
/// [root] is included (even if it doesn't exist)
fn recursive_paths<P: AsRef<Path>>(root: P) -> Vec<PathBuf> {
    // dump [files rooted at [path] into [accum]
    fn add_files(accum: &mut Vec<PathBuf>, path: &Path, root: &Path) {
        for child in path.read_dir().into_iter().flatten() {
            add_files(
                accum,
                &child.expect("read_dir returns a valid dir").path(),
                root,
            )
        }
        accum.push(path.strip_prefix(root).unwrap().to_path_buf());
    }

    let mut result = vec![];
    add_files(&mut result, root.as_ref(), root.as_ref());
    result.sort();
    result
}

/// # Infrastructure tests ////////////////////////////////////////////////////////////////////////

#[test]
fn test_new_file_slurp() {
    let mut proj = Project::new().unwrap();
    new_file(&mut proj, "foo/bar/baz.txt", "test text").unwrap();
    assert_snapshot!(slurp_file(&proj, "foo/bar/baz.txt").unwrap(), @"test text");
}

#[test]
fn test_new_file_recursive_paths() {
    let mut proj = Project::new().unwrap();
    new_file(&mut proj, "foo/bar.txt", "foo/bar").unwrap();
    new_file(&mut proj, "foo/baz/baz.txt", "foo/baz/baz").unwrap();
    new_file(&mut proj, "qux.txt", "qux").unwrap();
    new_file(&mut proj, "quux/.foo", "qux").unwrap();

    assert_debug_snapshot!(recursive_paths(proj.path()), @r###"
    [
        "",
        "foo",
        "foo/bar.txt",
        "foo/baz",
        "foo/baz/baz.txt",
        "quux",
        "quux/.foo",
        "qux.txt",
    ]
    "###);
}

/// # Tests for `sui move new` /////////////////////////////////////////////////////////////////////

#[test]
/// Check files created by `sui-move new example`, where `example/` doesn't exist
fn test_new_basic() {
    let (mut proj, mut cmd) = sui_move();

    // sui move new example
    assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // check list of files
    assert_debug_snapshot!(recursive_paths(proj.path().join("example")), @r###"
    [
        "",
        ".gitignore",
        "Move.toml",
        "sources",
        "sources/example.move",
        "tests",
        "tests/example_tests.move",
    ]
    "###);

    // check .gitignore contents
    assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").expect("sui move new creates .gitignore"), @r###"
    build/*
    "###);
}

#[test]
/// `sui-move new example` when `example/.gitignore` exists: it should be modified rather than
/// replaced
fn test_new_gitignore_exists() {
    let (mut proj, mut cmd) = sui_move();

    // create .gitignore file
    new_file(&mut proj, "example/.gitignore", "existing_ignore\n").unwrap();

    // sui move new example
    assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // check list of files
    assert_debug_snapshot!(recursive_paths(proj.path().join("example")), @r###"
    [
        "",
        ".gitignore",
        "Move.toml",
        "sources",
        "sources/example.move",
        "tests",
        "tests/example_tests.move",
    ]
    "###);

    // check .gitignore contents
    assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").expect("sui move new updates .gitignore"), @r###"
    existing_ignore
    build/*
    "###);
}

#[test]
/// `sui-move new example` when `example/.gitignore` already contains `build/*`: it should be
/// unchanged
fn test_new_gitignore_has_build() {
    let (mut proj, mut cmd) = sui_move();

    // create .gitignore file containing `build/*`
    new_file(
        &mut proj,
        "example/.gitignore",
        r###"
        first_ignore
        build/*
        another_ignore
        "###,
    )
    .unwrap();

    // sui move new example
    assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###,
    );

    // check list of files
    assert_debug_snapshot!(recursive_paths(proj.path().join("example")), @r###"
    [
        "",
        ".gitignore",
        "Move.toml",
        "sources",
        "sources/example.move",
        "tests",
        "tests/example_tests.move",
    ]
    "###);

    // check .gitignore contents
    assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").expect("sui move new updates .gitignore"), @r###"
    first_ignore
    build/*
    another_ignore
    "###);
}

// TODO: implement this functionality (linear: https://linear.app/mysten-labs/issue/DVX-486/sui-move-new-will-clobber-existing-files)
//
// #[test]
// /// `sui-move new example` when `example/sources` exists should not generate any new example source
// /// but should otherwise operate normally
// fn test_new_sources_exists() {
//     let (mut proj, mut cmd) = sui_move();
//
//     // create .gitignore and sources/ files
//     new_file(&mut proj, "example/.gitignore", "existing_ignore\n").unwrap();
//     new_file(&mut proj, "example/sources/dummy.txt", "").unwrap();
//
//     // sui move new example
//     assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
//     success: true
//     exit_code: 0
//     ----- stdout -----
//
//     ----- stderr -----
//     "###);
//
//     // check list of files - no new source files
//     assert_debug_snapshot!(recursive_paths(proj.path().join("example")), @r###"
//     [
//         "",
//         ".gitignore",
//         "Move.toml",
//         "sources",
//         "sources/dummy.txt",
//         "tests",
//     ]
//     "###);
//
//     // check .gitignore contents
//     assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").expect("sui move new creates .gitignore"), @r###"
//     existing_ignore
//     build/*
//     "###);
// }
//
// #[test]
// /// `sui-move new example` when `example/tests` exists should not generate any new example source
// /// but should otherwise operate normally
// fn test_new_tests_exists() {
//     let (mut proj, mut cmd) = sui_move();
//
//     // create .gitignore and tests/ files
//     new_file(&mut proj, "example/.gitignore", "existing_ignore\n").unwrap();
//     new_file(&mut proj, "example/tests/dummy.txt", "").unwrap();
//
//     // sui move new example
//     assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
//     success: true
//     exit_code: 0
//     ----- stdout -----
//
//     ----- stderr -----
//     "###);
//
//     // check list of files - no new source files
//     assert_debug_snapshot!(recursive_paths(proj.path().join("example")), @r###"
//     [
//         "",
//         ".gitignore",
//         "Move.toml",
//         "sources",
//         "tests",
//         "tests/dummy.txt",
//     ]
//     "###);
//
//     // check .gitignore contents
//     assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").expect("sui move new creates .gitignore"), @r###"
//     existing_ignore
//     build/*
//     "###);
// }
//
// #[test]
// /// `sui-move new example` when `example/Move.toml` exists should fail and modify nothing
// fn test_new_move_toml_exists() {
//     let (mut proj, mut cmd) = sui_move();
//
//     // create files
//     new_file(&mut proj, "example/Move.toml", "dummy").unwrap();
//     new_file(&mut proj, "example/.gitignore", "existing_ignore\n").unwrap();
//
//     // sui move new example
//     assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
//     success: false
//     "###);
//
//     // check list of files - should be unchanged
//     assert_debug_snapshot!(recursive_paths(proj.path().join("example")), @r###"
//     [
//          "",
//          "Move.toml",
//          ".gitignore",
//     ]
//     "###);
//
//     // check that files are unchanged
//     assert_snapshot!(slurp_file(&mut proj, "example/Move.toml").expect("Move.toml should exist"), @"dummy");
//     assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").expect(".gitignore should exist"), @"existing_ignore\n");
// }
