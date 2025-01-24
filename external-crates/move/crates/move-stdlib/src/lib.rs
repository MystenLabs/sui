// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::files::{extension_equals, find_filenames, MOVE_EXTENSION};
use move_core_types::parsing::address::NumericalAddress;
use move_docgen::DocgenOptions;
use move_package::BuildConfig;
use std::{collections::BTreeMap, fs, path::Path, path::PathBuf};

#[cfg(test)]
mod tests;
pub mod utils;

const MODULES_DIR: &str = "sources";
const DOCS_DIR: &str = "docs";

const REFERENCES_TEMPLATE: &str = "doc_templates/references.md";
const OVERVIEW_TEMPLATE: &str = "doc_templates/overview.md";

pub fn unit_testing_files() -> Vec<String> {
    vec![path_in_crate("sources/UnitTest.move")]
        .into_iter()
        .map(|p| p.into_os_string().into_string().unwrap())
        .collect()
}

pub fn path_in_crate<S>(relative: S) -> PathBuf
where
    S: Into<String>,
{
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(relative.into());
    path
}

pub fn move_stdlib_modules_full_path() -> String {
    format!("{}/{}", env!("CARGO_MANIFEST_DIR"), MODULES_DIR)
}

pub fn move_stdlib_docs_full_path() -> String {
    format!("{}/{}", env!("CARGO_MANIFEST_DIR"), DOCS_DIR)
}

pub fn move_stdlib_files() -> Vec<String> {
    let path = path_in_crate(MODULES_DIR);
    find_filenames(&[path], |p| extension_equals(p, MOVE_EXTENSION)).unwrap()
}

pub fn move_stdlib_named_addresses() -> BTreeMap<String, NumericalAddress> {
    let mapping = [("std", "0x1")];
    mapping
        .iter()
        .map(|(name, addr)| (name.to_string(), NumericalAddress::parse_str(addr).unwrap()))
        .collect()
}

pub fn build_stdlib_doc(output_directory: String) -> anyhow::Result<()> {
    let config = BuildConfig {
        additional_named_addresses: move_stdlib_named_addresses()
            .into_iter()
            .map(|(k, v)| (k, v.into_inner()))
            .collect(),
        ..BuildConfig::default()
    };
    let model = config.move_model_for_package(
        Path::new(&move_stdlib_modules_full_path()),
        &mut std::io::stdout(),
    )?;
    let options = DocgenOptions {
        output_directory,
        doc_path: vec![String::new()],
        root_doc_templates: vec![path_in_crate(OVERVIEW_TEMPLATE)
            .to_string_lossy()
            .to_string()],
        references_file: Some(
            path_in_crate(REFERENCES_TEMPLATE)
                .to_string_lossy()
                .to_string(),
        ),
        ..DocgenOptions::default()
    };
    let docgen = move_docgen::Docgen::new(&model, &options);
    for (file, content) in docgen.gen(&model)? {
        let path = PathBuf::from(&file);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path.as_path(), content)?;
    }
    Ok(())
}
