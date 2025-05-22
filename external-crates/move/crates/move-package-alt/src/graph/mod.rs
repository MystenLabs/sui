// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;

use crate::{
    errors::PackageResult,
    flavor::MoveFlavor,
    package::EnvironmentName,
    package::{Package, PackageName, PackagePath, lockfile::Lockfile},
};
use petgraph::graph::DiGraph;

#[derive(Debug)]
pub struct PackageGraph<F: MoveFlavor + fmt::Debug> {
    pub inner: DiGraph<Package<F>, PackageName>,
}

impl<F: MoveFlavor> PackageGraph<F> {
    /// Try to load a package graph from the lockfile in [path]; check if it is up-to-date (i.e. if
    /// the manifest digests are correct), and if not, rebuild the graph from the manifest
    pub fn load(path: PackagePath) -> PackageResult<Self> {
        todo!()
    }

    pub async fn load_from_manifests(path: PackagePath) -> PackageResult<Self> {
        todo!()
    }

    pub async fn load_from_lockfile(
        path: PackagePath,
        env: &EnvironmentName,
    ) -> PackageResult<Self> {
        let lockfile = Lockfile::<F>::read_from_dir(path.as_path())?;
        let mut graph = Self {
            inner: DiGraph::new(),
        };

        let deps = lockfile.pinned_deps_for_env(env);

        let mut package_indices = BTreeMap::new();
        if let Some(deps) = deps {
            // First pass: create nodes for all packages
            for (pkg_id, dep_info) in deps.data.iter() {
                let package = Package::load(dep_info.source.clone()).await?;
                let index = graph.inner.add_node(package);
                package_indices.insert(pkg_id.clone(), index);
            }

            // Second pass: add edges based on dependencies
            for (pkg_id, dep_info) in deps.data.iter() {
                let from_index = package_indices.get(pkg_id).unwrap();
                for (dep_name, dep_id) in dep_info.deps.iter() {
                    if let Some(to_index) = package_indices.get(dep_id) {
                        graph
                            .inner
                            .add_edge(*from_index, *to_index, dep_name.clone());
                    }
                }
            }
        }

        println!("Package graph: {:?}", graph.inner);

        Ok(graph)
    }
}
