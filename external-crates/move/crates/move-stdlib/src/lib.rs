// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::files::{MOVE_EXTENSION, extension_equals, find_filenames};
use move_core_types::parsing::address::NumericalAddress;
use move_docgen::DocgenOptions;
use move_package_alt::flavor::Vanilla;
use move_package_alt_compilation::build_config::BuildConfig;
use std::{
    collections::BTreeMap,
    fs,
    io::Stdout,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;
pub mod utils;

const MODULES_DIR: &str = "sources";
const DOCS_DIR: &str = "docs";
const SUMMARIES_DIR: &str = "summaries";

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

pub fn modules_full_path() -> String {
    format!("{}/{}", env!("CARGO_MANIFEST_DIR"), MODULES_DIR)
}

pub fn docs_full_path() -> String {
    format!("{}/{}", env!("CARGO_MANIFEST_DIR"), DOCS_DIR)
}

pub fn summaries_full_path() -> String {
    format!("{}/{}", env!("CARGO_MANIFEST_DIR"), SUMMARIES_DIR)
}

pub fn source_files() -> Vec<String> {
    let path = path_in_crate(MODULES_DIR);
    find_filenames(&[path], |p| extension_equals(p, MOVE_EXTENSION)).unwrap()
}

pub fn named_addresses() -> BTreeMap<String, NumericalAddress> {
    let mapping = [("std", "0x1")];
    mapping
        .iter()
        .map(|(name, addr)| (name.to_string(), NumericalAddress::parse_str(addr).unwrap()))
        .collect()
}

pub async fn build_doc(output_directory: String) -> anyhow::Result<()> {
    let config = build_config();

    let env = move_package_alt::flavor::vanilla::default_environment();

    let model = config
        .move_model_from_path::<Vanilla, Stdout>(
            Path::new(&modules_full_path()).parent().unwrap(),
            env,
            &mut std::io::stdout(),
        )
        .await?;
    let options = DocgenOptions {
        output_directory,
        doc_path: vec![String::new()],
        root_doc_templates: vec![
            path_in_crate(OVERVIEW_TEMPLATE)
                .to_string_lossy()
                .to_string(),
        ],
        references_file: Some(
            path_in_crate(REFERENCES_TEMPLATE)
                .to_string_lossy()
                .to_string(),
        ),
        ..DocgenOptions::default()
    };
    let docgen = move_docgen::Docgen::new(&model, &options);
    for (file, content) in docgen.generate(&model)? {
        let path = PathBuf::from(&file);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path.as_path(), content)?;
    }
    Ok(())
}

pub(crate) fn build_config() -> BuildConfig {
    BuildConfig::default()
}
