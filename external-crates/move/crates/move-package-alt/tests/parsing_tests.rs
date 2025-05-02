// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use codespan_reporting::{
    files::SimpleFiles,
    term::{self, Config, termcolor::Buffer},
};

use move_command_line_common::insta_assert;
use move_package_alt::{
    flavor::Vanilla,
    package::{lockfile::Lockfile, manifest::Manifest},
};

fn run_manifest_parsing_tests(input_path: &Path) -> datatest_stable::Result<()> {
    let manifest = Manifest::<Vanilla>::read_from(input_path);

    let contents = match manifest.as_ref() {
        Ok(m) => format!("{:?}", m),
        Err(_) => {
            let mut mapped_files = SimpleFiles::new();
            mapped_files.add(
                input_path.to_str().unwrap(),
                std::fs::read_to_string(input_path).unwrap(),
            );

            if let Some(e) = manifest.as_ref().err() {
                let diagnostic = e.to_diagnostic();
                let mut writer = Buffer::no_color();
                term::emit(&mut writer, &Config::default(), &mapped_files, &diagnostic).unwrap();
                let inner = writer.into_inner();
                String::from_utf8(inner).unwrap_or_default()
            } else {
                format!("{}", manifest.unwrap_err())
            }
        }
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
    "tests/data/lockfile_parsing_valid",
    r".*",
);
