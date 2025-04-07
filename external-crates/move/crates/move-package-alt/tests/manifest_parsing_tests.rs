// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::insta_assert;
use move_package_alt::{flavor::Vanilla, package::manifest::Manifest};
use std::path::Path;

fn run_manifest_parsing_tests(input_path: &Path) -> datatest_stable::Result<()> {
    let manifest = Manifest::<Vanilla>::read_from(input_path);

    let contents = if let Ok(manifest) = manifest {
        format!("{:?}", manifest)
    } else {
        format!("{}", manifest.unwrap_err())
    };

    insta_assert! {
        input_path: input_path,
        contents: contents,
    }

    Ok(())
}

datatest_stable::harness!(
    run_manifest_parsing_tests,
    "tests/data",
    r"manifest_parsing.*\.toml$",
);
