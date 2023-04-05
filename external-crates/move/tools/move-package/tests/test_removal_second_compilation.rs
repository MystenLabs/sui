// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_package::{compilation::package_layout::CompiledPackageLayout, BuildConfig};
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_that_second_build_artifacts_removed() {
    let path = Path::new("tests/test_sources/basic_no_deps_test_mode");
    let dir = tempdir().unwrap().path().to_path_buf();

    BuildConfig {
        dev_mode: true,
        test_mode: true,
        install_dir: Some(dir.clone()),
        ..Default::default()
    }
    .compile_package(path, &mut Vec::new())
    .unwrap();

    let expected_stdlib_path = dir
        .join(CompiledPackageLayout::Root.path())
        .join("test")
        .join(CompiledPackageLayout::CompiledModules.path())
        .join(CompiledPackageLayout::Dependencies.path())
        .join("MoveStdlib");
    assert!(expected_stdlib_path.is_dir());

    assert!(dir
        .join(CompiledPackageLayout::Root.path())
        .join("test")
        .join(CompiledPackageLayout::CompiledModules.path())
        .join("MTest.mv")
        .exists());

    // Now make sure the MoveStdlib still exists, but that the test-only code is removed
    BuildConfig {
        dev_mode: true,
        test_mode: false,
        install_dir: Some(dir.clone()),
        ..Default::default()
    }
    .compile_package(path, &mut Vec::new())
    .unwrap();

    // The MoveStdlib dep should still exist, but the MTest module should go away
    assert!(expected_stdlib_path.is_dir());
    assert!(!dir
        .join(CompiledPackageLayout::Root.path())
        .join("test")
        .join(CompiledPackageLayout::CompiledModules.path())
        .join("MTest.mv")
        .exists());

    BuildConfig {
        dev_mode: false,
        test_mode: false,
        install_dir: Some(dir.clone()),
        ..Default::default()
    }
    .compile_package(path, &mut Vec::new())
    .unwrap();

    // The MoveStdlib dep should no longer exist, and the MTest module shouldn't exist either
    assert!(!expected_stdlib_path.is_dir());
    assert!(!dir
        .join(CompiledPackageLayout::Root.path())
        .join("test")
        .join(CompiledPackageLayout::CompiledModules.path())
        .join("MTest.mv")
        .exists());
}
