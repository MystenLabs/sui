// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::layout::CompiledPackageLayout;
use anyhow::Result;
use move_command_line_common::files::{extension_equals, find_filenames};
use move_docgen::{Docgen, DocgenFlags, DocgenOptions};
use move_model_2::source_model;
use move_package_alt::package::layout::SourcePackageLayout;
use move_symbol_pool::Symbol;
use std::path::{Path, PathBuf};

/// References file for documentation generation
pub const REFERENCE_TEMPLATE_FILENAME: &str = "references.md";

pub fn build_docs(
    docgen_flags: DocgenFlags,
    package_name: Symbol,
    model: &source_model::Model,
    package_root: &Path,
    deps: &[Symbol],
    install_dir: &Option<PathBuf>,
) -> Result<Vec<(String, String)>> {
    let root_doc_templates = find_filenames(
        &[package_root
            .join(SourcePackageLayout::DocTemplates.path())
            .to_string_lossy()
            .to_string()],
        |path| extension_equals(path, "md"),
    )
    .unwrap_or_else(|_| vec![]);
    let root_for_docs = if let Some(install_dir) = install_dir {
        install_dir.join(CompiledPackageLayout::Root.path())
    } else {
        CompiledPackageLayout::Root.path().to_path_buf()
    };
    let dep_paths = deps
        .iter()
        .map(|dep_name| {
            root_for_docs
                .join(CompiledPackageLayout::CompiledDocs.path())
                .join(dep_name.as_str())
                .to_string_lossy()
                .to_string()
        })
        .collect();
    let in_pkg_doc_path = root_for_docs
        .join(CompiledPackageLayout::CompiledDocs.path())
        .join(package_name.as_str());
    let references_path = package_root
        .join(SourcePackageLayout::DocTemplates.path())
        .join(REFERENCE_TEMPLATE_FILENAME);
    let references_file = if references_path.exists() {
        Some(references_path.to_string_lossy().to_string())
    } else {
        None
    };
    let doc_options = DocgenOptions {
        doc_path: dep_paths,
        output_directory: in_pkg_doc_path.to_string_lossy().to_string(),
        root_doc_templates,
        compile_relative_to_output_dir: true,
        references_file,
        flags: docgen_flags,
    };
    let docgen = Docgen::new(model, &doc_options);
    docgen.generate(model)
}
