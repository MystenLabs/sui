// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compilation::compiled_package::CompiledPackage,
    resolution::resolution_graph::Package,
    resolution::resolution_graph::ResolvedGraph,
    source_package::{
        manifest_parser::{resolve_move_manifest_path, EDITION_NAME, PACKAGE_NAME},
        parsed_manifest::PackageName,
    },
};
use anyhow::Result;
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::{
        report_diagnostics_to_color_buffer, report_warnings, FilesSourceText, Migration,
    },
    editions::Edition,
    Compiler,
};
use std::{
    collections::BTreeSet,
    io::Write,
    path::{Path, PathBuf},
};

use toml_edit::{value, Document};

use super::{
    compiled_package::{DependencyInfo, ModuleFormat},
    package_layout::CompiledPackageLayout,
};

#[derive(Debug, Clone)]
pub struct BuildPlan {
    root: PackageName,
    sorted_deps: Vec<PackageName>,
    resolution_graph: ResolvedGraph,
}

struct CompilationDependencies<'a> {
    root_package: Package,
    project_root: PathBuf,
    transitive_dependencies: Vec<DependencyInfo<'a>>,
}

impl BuildPlan {
    pub fn create(resolution_graph: ResolvedGraph) -> Result<Self> {
        let mut sorted_deps = resolution_graph.topological_order();
        sorted_deps.reverse();

        Ok(Self {
            root: resolution_graph.root_package(),
            sorted_deps,
            resolution_graph,
        })
    }

    pub fn root_crate_edition_defined(&self) -> bool {
        self.resolution_graph.package_table[&self.root]
            .source_package
            .package
            .edition
            .is_some()
    }

    /// Compilation results in the process exit upon warning/failure
    pub fn compile<W: Write>(&self, writer: &mut W) -> Result<CompiledPackage> {
        self.compile_with_driver(writer, |compiler| compiler.build_and_report())
    }

    /// Compilation results in the process exit upon warning/failure
    pub fn migrate<W: Write>(&self, writer: &mut W) -> Result<Option<Migration>> {
        let CompilationDependencies {
            root_package,
            project_root,
            transitive_dependencies,
        } = self.compute_dependencies();

        let (_, migration) = CompiledPackage::build_for_result(
            writer,
            root_package,
            transitive_dependencies,
            &self.resolution_graph,
            |compiler| compiler.generate_migration_patch(&self.root),
        )?;

        Self::clean(
            &project_root.join(CompiledPackageLayout::Root.path()),
            self.sorted_deps.iter().copied().collect(),
        )?;
        Ok(migration)
    }

    /// Compilation process does not exit even if warnings/failures are encountered
    pub fn compile_no_exit<W: Write>(&self, writer: &mut W) -> Result<CompiledPackage> {
        self.compile_with_driver(writer, |compiler| {
            let (files, units_res) = compiler.build()?;
            match units_res {
                Ok((units, warning_diags)) => {
                    report_warnings(&files, warning_diags);
                    Ok((files, units))
                }
                Err(error_diags) => {
                    assert!(!error_diags.is_empty());
                    let diags_buf = report_diagnostics_to_color_buffer(&files, error_diags);
                    if let Err(err) = std::io::stdout().write_all(&diags_buf) {
                        anyhow::bail!("Cannot output compiler diagnostics: {}", err);
                    }
                    anyhow::bail!("Compilation error");
                }
            }
        })
    }

    fn compute_dependencies(&self) -> CompilationDependencies {
        let root_package = &self.resolution_graph.package_table[&self.root];
        let project_root = match &self.resolution_graph.build_options.install_dir {
            Some(under_path) => under_path.clone(),
            None => self.resolution_graph.graph.root_path.clone(),
        };
        let immediate_dependencies_names =
            root_package.immediate_dependencies(&self.resolution_graph);
        let transitive_dependencies = self
            .resolution_graph
            .topological_order()
            .into_iter()
            .filter(|package_name| *package_name != self.root)
            .map(|package_name| {
                let dep_package = self
                    .resolution_graph
                    .package_table
                    .get(&package_name)
                    .unwrap();
                let mut dep_source_paths = dep_package
                    .get_sources(&self.resolution_graph.build_options)
                    .unwrap();
                let mut source_available = true;
                // If source is empty, search bytecode(mv) files
                if dep_source_paths.is_empty() {
                    dep_source_paths = dep_package.get_bytecodes().unwrap();
                    source_available = false;
                }
                DependencyInfo {
                    name: package_name,
                    is_immediate: immediate_dependencies_names.contains(&package_name),
                    source_paths: dep_source_paths,
                    address_mapping: &dep_package.resolved_table,
                    compiler_config: dep_package.compiler_config(
                        /* is_dependency */ true,
                        &self.resolution_graph.build_options,
                    ),
                    module_format: if source_available {
                        ModuleFormat::Source
                    } else {
                        ModuleFormat::Bytecode
                    },
                }
            })
            .collect();

        CompilationDependencies {
            root_package: root_package.clone(),
            project_root,
            transitive_dependencies,
        }
    }

    pub fn compile_with_driver<W: Write>(
        &self,
        writer: &mut W,
        mut compiler_driver: impl FnMut(
            Compiler,
        )
            -> anyhow::Result<(FilesSourceText, Vec<AnnotatedCompiledUnit>)>,
    ) -> Result<CompiledPackage> {
        let CompilationDependencies {
            root_package,
            project_root,
            transitive_dependencies,
        } = self.compute_dependencies();

        let compiled = CompiledPackage::build_all(
            writer,
            &project_root,
            root_package,
            transitive_dependencies,
            &self.resolution_graph,
            &mut compiler_driver,
        )?;

        Self::clean(
            &project_root.join(CompiledPackageLayout::Root.path()),
            self.sorted_deps.iter().copied().collect(),
        )?;
        Ok(compiled)
    }

    // Clean out old packages that are no longer used, or no longer used under the current
    // compilation flags
    fn clean(build_root: &Path, keep_paths: BTreeSet<PackageName>) -> Result<()> {
        for dir in std::fs::read_dir(build_root)? {
            let path = dir?.path();
            if !keep_paths.iter().any(|name| path.ends_with(name.as_str())) {
                if path.is_file() {
                    std::fs::remove_file(&path)?;
                } else {
                    std::fs::remove_dir_all(&path)?;
                }
            }
        }
        Ok(())
    }

    pub fn root_package_path(&self) -> PathBuf {
        self.resolution_graph.package_table[&self.root]
            .package_path
            .clone()
    }

    pub fn record_package_edition(&self, edition: Edition) -> anyhow::Result<()> {
        let move_toml_path = resolve_move_manifest_path(&self.root_package_path());
        let mut toml = std::fs::read_to_string(move_toml_path.clone())?
            .parse::<Document>()
            .expect("Failed to read TOML file to update edition");
        toml[PACKAGE_NAME][EDITION_NAME] = value(edition.to_string());
        std::fs::write(move_toml_path, toml.to_string())?;
        Ok(())
    }
}
