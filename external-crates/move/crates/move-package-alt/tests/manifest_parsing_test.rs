// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_package_alt::{flavor::Vanilla, package::manifest::Manifest};
use std::path::Path;

fn run_manifest_valid_vanilla_tests(path: &Path) -> datatest_stable::Result<()> {
    let manifest = Manifest::<Vanilla>::read_from(path);
    let file_name = path.parent().unwrap().display().to_string();

    insta::assert_debug_snapshot!(file_name, manifest);

    Ok(())
}

datatest_stable::harness!(run_manifest_valid_vanilla_tests, "tests", r".*\.toml$",);
