// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::PathBuf,
};
use tempfile::TempDir;

use move_package::resolution::lock_file::LockFile;

#[test]
fn commit() {
    let pkg = create_test_package().unwrap();
    let lock_path = pkg.path().join("Move.lock");

    {
        let mut lock = LockFile::new(pkg.path().to_path_buf()).unwrap();
        writeln!(lock, "# Write and commit").unwrap();
        lock.commit(&lock_path).unwrap();
    }

    assert!(lock_path.is_file());

    let lock_contents = {
        let mut lock_file = File::open(lock_path).unwrap();
        let mut buf = String::new();
        lock_file.read_to_string(&mut buf).unwrap();
        buf
    };

    // Check that the content written into the `LockFile` instance above can be found at the path
    // that that lock file was committed to (indicating that the commit actually happened).
    assert!(
        lock_contents.ends_with("# Write and commit\n"),
        "Lock file doesn't have expected content:\n{}",
        lock_contents,
    );
}

#[test]
fn discard() {
    let pkg = create_test_package().unwrap();

    {
        let mut lock = LockFile::new(pkg.path().to_path_buf()).unwrap();
        writeln!(lock, "# Write but don't commit").unwrap();
    }

    assert!(!pkg.path().join("Move.lock").is_file());
}

/// Create a simple Move package with no sources (just a manifest and an output directory) in a
/// temporary directory, and return it.
fn create_test_package() -> io::Result<TempDir> {
    let dir = tempfile::tempdir()?;

    let toml_path: PathBuf = [".", "tests", "test_sources", "basic_no_deps", "Move.toml"]
        .into_iter()
        .collect();

    fs::copy(toml_path, dir.path().join("Move.toml"))?;
    Ok(dir)
}
