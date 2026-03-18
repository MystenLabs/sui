// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::testing::insta_assert;
use move_compiler::parser::ast::TargetKind;
use move_docgen::{Docgen, DocgenFlags, DocgenOptions};
use move_package_alt::{RootPackage, Vanilla};
use move_package_alt_compilation::build_config::BuildConfig;
use move_package_alt_compilation::model_builder;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;

const ROOT_DOC_TEMPLATE_NAME: &str = "root_template.md";

fn options(root_doc_template: Option<&Path>, flags: DocgenFlags) -> DocgenOptions {
    DocgenOptions {
        output_directory: "output".to_string(),
        root_doc_templates: root_doc_template
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        compile_relative_to_output_dir: true,
        flags,
        ..DocgenOptions::default()
    }
}

fn test_default(toml_path: &Path) -> datatest_stable::Result<()> {
    let flags = DocgenFlags::default();
    assert!(!flags.exclude_impl);
    assert!(!flags.no_collapsed_sections);
    test_impl(toml_path, flags, "default")
}

fn test_collapsed_sections(toml_path: &Path) -> datatest_stable::Result<()> {
    let mut flags = DocgenFlags::default();
    assert!(!flags.exclude_impl);
    flags.no_collapsed_sections = true;
    test_impl(toml_path, flags, "collapsed_sections")
}

fn test_impl(toml_path: &Path, flags: DocgenFlags, test_case: &str) -> datatest_stable::Result<()> {
    let test_dir = toml_path.parent().unwrap();
    let output_dir = TempDir::new()?;
    let config = BuildConfig {
        test_mode: false,
        install_dir: Some(output_dir.path().to_path_buf()),
        force_recompilation: false,
        ..Default::default()
    };
    let mut w = Vec::new();

    // Block on the async function
    let env = Vanilla::default_environment();
    let root_pkg: RootPackage<Vanilla> = config
        .package_loader(toml_path.parent().unwrap(), &env)
        .load_sync()?;
    let model = model_builder::build(&mut w, &root_pkg, &config)?;
    let root_doc_template: PathBuf = test_dir.join(ROOT_DOC_TEMPLATE_NAME);
    let root_doc_template = if root_doc_template.is_file() {
        Some(root_doc_template.as_path())
    } else {
        None
    };
    let options = options(root_doc_template, flags);
    let docgen = Docgen::new(&model, &options);
    let file_contents = docgen.generate(&model)?;
    // Determine root package directory prefix to filter out dependency docs.
    // Template output files sit at top level (no `/` prefix) and should also be included.
    let root_pkg_dir = model
        .modules()
        .find(|m| {
            matches!(
                m.info().target_kind,
                TargetKind::Source {
                    is_root_package: true
                }
            )
        })
        .map(|m| {
            m.package()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|| m.id().address.to_string())
        })
        .expect("must have at least one root package module");
    let root_prefix = format!("{}/", root_pkg_dir);
    let [(path, contents)] = file_contents
        .iter()
        .filter(|(path, _contents)| {
            path.starts_with(&root_prefix) || !path.contains('/')
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("Test infra supports only one output file currently");
    insta_assert! {
        input_path: toml_path,
        contents: contents,
        name: path,
        info: &options.flags,
        suffix: test_case,
    };
    Ok(())
}

datatest_stable::harness!(
    test_default,
    "tests/move/",
    r".*\.toml",
    test_collapsed_sections,
    "tests/move/",
    r".*\.toml"
);
