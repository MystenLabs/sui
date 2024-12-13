// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow;
use cli_sandbox::Project;
use insta::{assert_snapshot, assert_yaml_snapshot};
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::{fs, fs::File, path::PathBuf, process::Command};

/// Note: tests use [insta] and [insta-cmd]; you can review and update expected test results using
/// cargo insta test --review

/// # Test infrastructure //////////////////////////////////////////////////////////////////////////

fn sui_move() -> (Project, Command) {
    let project = Project::new().unwrap();
    let mut cmd = Command::new(get_cargo_bin("sui"));
    cmd.arg("move");
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
fn recursive_paths(proj: &Project) -> Vec<PathBuf> {
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
    add_files(&mut result, proj.path(), proj.path());
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

    assert_yaml_snapshot!(recursive_paths(&proj));
}

/// # Tests for `sui move new` /////////////////////////////////////////////////////////////////////

#[test]
fn test_new_basic() {
    let (mut proj, mut cmd) = sui_move();

    // sui move new
    assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // check list of files
    assert_yaml_snapshot!(recursive_paths(&proj));

    // check .gitignore contents
    assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").unwrap(), @r###"
    build/*
    "###)
}

#[test]
fn test_new_gitignore_exists() {
    let (mut proj, mut cmd) = sui_move();

    // create .gitignore file
    new_file(&mut proj, "example/.gitignore", "existing_ignore\n").unwrap();

    // sui move new
    assert_cmd_snapshot!(cmd.arg("new").arg("example"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // check .gitignore contents
    assert_snapshot!(slurp_file(&mut proj, "example/.gitignore").unwrap(), @r###"
    existing_ignore
    build/*
    "###)
}

#[test]
fn test_new_sources_exists() {
    // TODO
}

#[test]
fn test_new_tests_exists() {
    // TODO
}

#[test]
fn test_new_move_toml_exists() {
    // TODO
}
