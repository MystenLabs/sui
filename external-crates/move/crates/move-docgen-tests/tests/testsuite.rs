// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_docgen::{Docgen, DocgenOptions};
use move_model_2::source_model;
use move_package::compilation::model_builder;
use move_package::BuildConfig;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;

const ROOT_DOC_TEMPLATE_NAME: &str = "root_template.md";

fn options(root_doc_template: Option<&Path>) -> DocgenOptions {
    DocgenOptions {
        output_directory: "output".to_string(),
        root_doc_templates: root_doc_template
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
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
    let model = model_builder::build(resolved_package, &mut w)?;
    let root_doc_template: PathBuf = test_dir.join(ROOT_DOC_TEMPLATE_NAME);
    let root_doc_template = if root_doc_template.is_file() {
        Some(root_doc_template.as_path())
    } else {
        None
    };
    let mut options = options(root_doc_template);

    assert!(!options.flags.exclude_impl);
    assert!(!options.flags.exclude_impl);
    test_move_one(&test_dir.join("default"), &model, &options)?;

    assert!(!options.flags.no_collapsed_sections);
    options.flags.no_collapsed_sections = true;
    test_move_one(&test_dir.join("collapsed_sections"), &model, &options)?;

    Ok(())
}

fn test_move_one(
    out_dir: &Path,
    model: &source_model::Model,
    doc_options: &DocgenOptions,
) -> anyhow::Result<()> {
    let docgen = Docgen::new(model, doc_options);
    let file_contents = docgen.gen(model)?;
    for (path, contents) in file_contents {
        if path.contains("dependencies") {
            continue;
        }
        let out_path = out_dir.join(&path).to_string_lossy().to_string();
        insta::assert_snapshot!(out_path, contents);
    }
    Ok(())
}

datatest_stable::harness!(test_move, "tests/move/", r".*\.toml",);
