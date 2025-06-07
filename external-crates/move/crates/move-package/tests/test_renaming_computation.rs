// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
use move_command_line_common::testing::insta;
use move_package::BuildConfig;
use std::{fmt::Write as _, path::Path};
use tempfile::tempdir;

#[test]
fn simple_root_renaming() {
    let path = Path::new("tests/test_sources/multiple_deps_rename");

    // resolution graph diagnostics are only needed for CLI commands so ignore them in both cases by
    // passing a vector as the writer

    let pkg1 = BuildConfig {
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        ..Default::default()
    }
    .resolution_graph_for_package(path, None, &mut Vec::new())
    .unwrap();

    let mut writer = String::new();
    for (pkg, remapping) in pkg1.root_renaming() {
        writeln!(&mut writer, "Package: {pkg}").unwrap();
        for (local_name, root_name) in remapping {
            writeln!(&mut writer, "\tlocal({local_name}) -> root({root_name})").unwrap();
        }
    }

    insta::assert_snapshot!(writer);
}

#[test]
fn transitive_root_renaming() {
    let path = Path::new("tests/test_sources/transitive_renames");

    // resolution graph diagnostics are only needed for CLI commands so ignore them in both cases by
    // passing a vector as the writer

    let pkg1 = BuildConfig {
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        ..Default::default()
    }
    .resolution_graph_for_package(path, None, &mut Vec::new())
    .unwrap();

    let mut writer = String::new();
    for (pkg, remapping) in pkg1.root_renaming() {
        writeln!(&mut writer, "Package: {pkg}").unwrap();
        for (local_name, root_name) in remapping {
            writeln!(&mut writer, "\tlocal({local_name}) -> root({root_name})").unwrap();
        }
    }

    insta::assert_snapshot!(writer);
}
