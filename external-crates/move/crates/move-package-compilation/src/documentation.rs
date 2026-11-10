// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::layout::CompiledPackageLayout;
use anyhow::{Context, Result};
use move_command_line_common::files::{extension_equals, find_filenames, try_exists};
use move_docgen::{Docgen, DocgenFlags, DocgenOptions};
use move_model_2::source_model;
use move_package::SourcePackageLayout;
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
    let root_doc_templates = find_root_doc_templates(package_root)?;
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

fn find_root_doc_templates(package_root: &Path) -> Result<Vec<String>> {
    let path = package_root.join(SourcePackageLayout::DocTemplates.path());
    if !try_exists(&path)
        .with_context(|| format!("Failed to inspect doc templates at '{}'", path.display()))?
    {
        return Ok(vec![]);
    }
    find_filenames(&[&path], |path| extension_equals(path, "md"))
        .with_context(|| format!("Failed to discover doc templates at '{}'", path.display()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::find_root_doc_templates;
    use std::{fs::create_dir, os::unix::fs::symlink};

    #[test]
    fn missing_doc_templates_are_optional() {
        let temp_dir = tempfile::tempdir().unwrap();

        assert!(find_root_doc_templates(temp_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn doc_template_traversal_errors_are_propagated() {
        let temp_dir = tempfile::tempdir().unwrap();
        let templates = temp_dir.path().join("doc_templates");
        create_dir(&templates).unwrap();
        symlink(&templates, templates.join("loop")).unwrap();

        let error = find_root_doc_templates(temp_dir.path()).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Failed to discover doc templates")
        );
    }
}
