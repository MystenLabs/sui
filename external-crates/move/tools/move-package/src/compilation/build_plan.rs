// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compilation::compiled_package::CompiledPackage, resolution::resolution_graph::ResolvedGraph,
    source_package::parsed_manifest::PackageName,
};
use anyhow::Result;
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::{report_diagnostics_to_color_buffer, report_warnings, FilesSourceText},
    Compiler,
};
use std::{collections::BTreeSet, io::Write, path::Path};

use super::package_layout::CompiledPackageLayout;

#[derive(Debug, Clone)]
pub struct BuildPlan {
    root: PackageName,
    sorted_deps: Vec<PackageName>,
    resolution_graph: ResolvedGraph,
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

    /// Compilation results in the process exit upon warning/failure
    pub fn compile<W: Write>(&self, writer: &mut W) -> Result<CompiledPackage> {
        self.compile_with_driver(writer, |compiler| compiler.build_and_report())
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

    pub fn compile_with_driver<W: Write>(
        &self,
        writer: &mut W,
        mut compiler_driver: impl FnMut(
            Compiler,
        )
            -> anyhow::Result<(FilesSourceText, Vec<AnnotatedCompiledUnit>)>,
    ) -> Result<CompiledPackage> {
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
                let dep_source_paths = dep_package
                    .get_sources(&self.resolution_graph.build_options)
                    .unwrap();
                (
                    package_name,
                    immediate_dependencies_names.contains(&package_name),
                    dep_source_paths,
                    &dep_package.resolved_table,
                    dep_package.compiler_config(&self.resolution_graph.build_options),
                )
            })
            .collect();

        let compiled = CompiledPackage::build_all(
            writer,
            &project_root,
            root_package.clone(),
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
                std::fs::remove_dir_all(&path)?;
            }
        }
        Ok(())
    }
}
