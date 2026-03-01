// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

use crate::{
    MoveFlavor,
    dependency::Pinned,
    errors::PackageResult,
    graph2::PackageInfo,
    package::{package_loader::PackageConfig, package_lock::PackageSystemLock},
    schema::{Environment, EnvironmentName, PackageID, PackageName},
};

use super::PackageGraph;

#[derive(Error, Debug)]
pub enum LockfileError {
    #[error("Invalid lockfile: there are multiple root nodes in environment {env}")]
    MultipleRootNodes { env: EnvironmentName },

    #[error("Invalid lockfile: there is no root node")]
    NoRootNode,

    #[error(
        "Invalid lockfile: package `{source_id}` has a dependency named `{dep_name}` in its manifest, but that dependency is not pinned in the lockfile"
    )]
    MissingDep {
        source_id: PackageID,
        dep_name: PackageName,
    },

    #[error("Invalid lockfile: package depends on a package with undefined ID `{target_id}`")]
    UndefinedDep { target_id: PackageID },
}

impl<'graph, F: MoveFlavor> PackageGraph<'graph, F> {
    pub async fn load(config: &PackageConfig, env: &Environment, mtx: &PackageSystemLock) -> Self {
        todo!()
    }

    /// Try to load the entire subgraph rooted at `package` from its lockfile into the graph.
    /// If the lockfile is missing or out of date, return false and don't add any edges to
    /// `package` (although disconnected nodes may be added to the graph)
    async fn add_subgraph_from_lockfile(
        &mut self,
        package: &PackageInfo<'graph, F>,
    ) -> PackageResult<bool> {
        // for dep in package.transitive_deps_from_lockfile
        //   load dep
        //   if dep.digest doesn't match lockfile digest
        //      return None
        //
        // for dep in package.direct_deps
        //   add edge from package to dep
        //
        // return dep
        todo!()
    }

    /// Load the entire subgraph rooted at `package` into `graph` and `visited` by first pinning
    /// the dependencies of `package` and then recursively loading the dependencies (from their
    /// lockfiles or manifests)
    async fn add_subgraph_from_manifest(
        &mut self,
        package: &PackageInfo<'graph, F>,
    ) -> PackageResult<()> {
        // pin direct deps of package
        // for dep in direct deps
        //   load dep
        //   direct_dep = add_subgraph(dep)
        //   add edge from package to direct_dep
        todo!()
    }

    /// Load the subgraph rooted at `package` into `graph` and `visited` by first trying to load
    /// from the lockfile of `package` and then repinning if that fails
    ///
    /// precondition: package is loaded but its deps might not be
    async fn add_subgraph(&mut self, package: &PackageInfo<'graph, F>) -> PackageResult<()> {
        // if `package` is in `visited`, return early
        // add package to graph
        // add_subgraph_from_lockfile(package).or_else(add_subgraph_from_manifest(package))
        // return `package`

        todo!()
    }

    /// Fetch `dep` and add it (but not its dependencies) to the graph
    async fn add_node(
        &mut self,
        dep: &Pinned,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<&PackageInfo<F>> {
        todo!()
    }
}
