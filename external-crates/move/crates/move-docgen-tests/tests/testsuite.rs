// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::term::termcolor::Buffer;
use itertools::Itertools;
use log::debug;
use move_command_line_common::env::read_bool_env_var;
use move_docgen::{Docgen, DocgenOptions};
use move_model_2::source_model;
use move_package::compilation::model_builder;
use move_package::BuildConfig;
use move_symbol_pool::Symbol;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::{fmt::Write, fs::File, io::Read};
use tempfile::TempDir;

const SAVE_FILES_ENV_VAR: &str = "KEEP";

fn options(root_doc_templates: Vec<String>) -> DocgenOptions {
    DocgenOptions {
        output_directory: "output".to_string(),
        root_doc_templates,
        compile_relative_to_output_dir: true,
        ..DocgenOptions::default()
    }
}

fn test_move(toml_path: &Path) -> datatest_stable::Result<()> {
    let test_dir = toml_path.parent().unwrap();
    let output_dir = TempDir::new()?;
    let config = BuildConfig {
        dev_mode: true,
        test_mode: false,
        install_dir: Some(output_dir.path().to_path_buf()),
        force_recompilation: false,
        ..Default::default()
    };
    let mut w = Vec::new();
    let resolved_package = config.resolution_graph_for_package(toml_path, None, &mut w)?;
    let package_name = resolved_package.root_package();
    let model = model_builder::build(resolved_package, &mut w)?;
    let mut out = String::new();
    let mut options = options(vec![]);

    assert!(options.flags.include_impl);
    assert!(options.flags.include_private_fun);
    test_move_one(&mut out, test_dir, &model, package_name, &options)?;

    assert!(options.flags.collapsed_sections);
    options.flags.collapsed_sections = false;
    test_move_one(&mut out, test_dir, &model, package_name, &options)?;

    insta::assert_snapshot!(out);
    Ok(())
}

fn test_move_one(
    out: &mut String,
    test_dir: &Path,
    model: &source_model::Model,
    package_name: Symbol,
    doc_options: &DocgenOptions,
) -> anyhow::Result<()> {
    let docgen = Docgen::new(model, package_name, &doc_options);
    let file_contents = docgen.gen(model)?;
    for (path, contents) in file_contents {
        if read_bool_env_var(SAVE_FILES_ENV_VAR) {
            fs::write(test_dir.join(&path), &contents).unwrap();
        }
        write!(
            out,
            "
<!---
BEGIN FILE '{path}' with settings
{doc_options:#?}
-->
{contents}
<!--- END FILE -->
            "
        )?;
    }
    Ok(())
}

datatest_stable::harness!(test_move, "tests/move/annotation", r".*\.toml",);
