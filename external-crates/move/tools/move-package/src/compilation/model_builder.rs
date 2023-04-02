// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compilation::compiled_package::make_source_and_deps_for_compiler,
    resolution::resolution_graph::ResolvedGraph, ModelConfig,
};
use anyhow::Result;
use move_compiler::shared::PackagePaths;
use move_model::{model::GlobalEnv, options::ModelBuilderOptions, run_model_builder_with_options};

#[derive(Debug, Clone)]
pub struct ModelBuilder {
    resolution_graph: ResolvedGraph,
    model_config: ModelConfig,
}

impl ModelBuilder {
    pub fn create(resolution_graph: ResolvedGraph, model_config: ModelConfig) -> Self {
        Self {
            resolution_graph,
            model_config,
        }
    }

    // NOTE: If there are now renamings, then the root package has the global resolution of all named
    // addresses in the package graph in scope. So we can simply grab all of the source files
    // across all packages and build the Move model from that.
    // TODO: In the future we will need a better way to do this to support renaming in packages
    // where we want to support building a Move model.
    pub fn build_model(&self) -> Result<GlobalEnv> {
        // Make sure no renamings have been performed
        for (pkg_name, pkg) in self.resolution_graph.package_table.iter() {
            if !pkg.renaming.is_empty() {
                anyhow::bail!(
                    "Found address renaming in package '{}' when \
                    building Move model -- this is currently not supported",
                    pkg_name
                )
            }
        }

        // Targets are all files in the root package
        let root_name = self.resolution_graph.root_package();
        let root_package = self.resolution_graph.get_package(root_name).clone();
        let deps_source_info = self
            .resolution_graph
            .package_table
            .iter()
            .filter_map(|(nm, pkg)| {
                if *nm == root_name {
                    return None;
                }
                let dep_source_paths = pkg
                    .get_sources(&self.resolution_graph.build_options)
                    .unwrap();
                Some(Ok((*nm, dep_source_paths, &pkg.resolved_table)))
            })
            .collect::<Result<Vec<_>>>()?;

        let (target, deps) = make_source_and_deps_for_compiler(
            &self.resolution_graph,
            &root_package,
            deps_source_info,
        )?;
        let (all_targets, all_deps) = if self.model_config.all_files_as_targets {
            let mut targets = vec![target];
            targets.extend(deps.into_iter());
            (targets, vec![])
        } else {
            (vec![target], deps)
        };
        let (all_targets, all_deps) = match &self.model_config.target_filter {
            Some(filter) => {
                let mut new_targets = vec![];
                let mut new_deps = all_deps;
                for PackagePaths {
                    name,
                    paths,
                    named_address_map,
                } in all_targets
                {
                    let (true_targets, false_targets): (Vec<_>, Vec<_>) =
                        paths.into_iter().partition(|t| t.contains(filter));
                    if !true_targets.is_empty() {
                        new_targets.push(PackagePaths {
                            name,
                            paths: true_targets,
                            named_address_map: named_address_map.clone(),
                        })
                    }
                    if !false_targets.is_empty() {
                        new_deps.push(PackagePaths {
                            name,
                            paths: false_targets,
                            named_address_map,
                        })
                    }
                }
                (new_targets, new_deps)
            }
            None => (all_targets, all_deps),
        };

        run_model_builder_with_options(all_targets, all_deps, ModelBuilderOptions::default())
    }
}
