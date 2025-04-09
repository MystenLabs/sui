// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::insta_assert;
use move_package_alt::{
    flavor::Vanilla,
    package::{lockfile::Lockfile, manifest::Manifest},
};
use std::path::Path;

fn run_manifest_parsing_tests(input_path: &Path) -> datatest_stable::Result<()> {
    let manifest = Manifest::<Vanilla>::read_from(input_path);

    let contents = match manifest {
        Ok(m) => format!("{:?}", m),
        Err(e) => e.to_string(),
    };

    insta_assert! {
        input_path: input_path,
        contents: contents,
    }

    Ok(())
}

fn run_lockfile_parsing_tests(input_path: &Path) -> datatest_stable::Result<()> {
    let lockfile = Lockfile::<Vanilla>::read_from(input_path.parent().unwrap());

    let contents = match lockfile {
        Ok(l) => format!("{:?}", l),
        Err(e) => e.to_string(),
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
    run_lockfile_parsing_tests,
    "tests/data",
    r"lockfile_parsing.*\.lock$",
);
