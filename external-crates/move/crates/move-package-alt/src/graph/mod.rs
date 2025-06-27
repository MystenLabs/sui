// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod builder;
mod to_lockfile;

use crate::{
    dependency::PinnedDependencyInfo,
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{
        EnvironmentName, Package, PackageName, lockfile::Lockfiles, manifest::Manifest,
        paths::PackagePath,
    },
    schema::{LockfileDependencyInfo, PackageID, Pin},
};
use builder::PackageGraphBuilder;
use derive_where::derive_where;
use move_core_types::identifier::Identifier;
use path_clean::PathClean;
use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, btree_map::Entry},
    fs::read_to_string,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::OnceCell;
use tracing::{debug, info};

#[derive(Debug)]
#[derive_where(Clone)]
pub struct PackageGraph<F: MoveFlavor> {
    inner: DiGraph<PackageNode<F>, PackageName>,
}

/// A node in the package graph, containing a [Package] in a particular environment
#[derive(Debug)]
#[derive_where(Clone)]
struct PackageNode<F: MoveFlavor> {
    package: Arc<Package<F>>,
    use_env: EnvironmentName,
}

impl<F: MoveFlavor> PackageNode<F> {
    fn name(&self) -> &PackageName {
        self.package.manifest().package_name()
    }
}

impl<F: MoveFlavor> PackageGraph<F> {
    /// Loads the package graph for each environment defined in the manifest. It checks whether the
    /// resolution graph in the lockfile inside `path` is up-to-date (i.e., whether any of the
    /// manifests digests are out of date). If the resolution graph is up-to-date, it is returned.
    /// Otherwise a new resolution graph is constructed by traversing (only) the manifest files.
    pub async fn load(path: &PackagePath) -> PackageResult<BTreeMap<EnvironmentName, Self>> {
        let manifest = Manifest::<F>::read_from_file(path.manifest_path())?;
        let envs = manifest.environments();
        let builder = PackageGraphBuilder::<F>::new();
        let mut output = BTreeMap::new();

        for env in envs.keys() {
            if let Some(graph) = builder.load_from_lockfile(path, env).await? {
                output.insert(env.clone(), graph);
            } else {
                output.insert(
                    env.clone(),
                    PackageGraphBuilder::<F>::new()
                        .load_from_manifests_by_env(path, env)
                        .await?,
                );
            }
        }
        Ok(output)
    }

    /// Constructs a [PackageGraph] for each environment in the manifest, by pinning and fetching
    /// all transitive dependencies from the manifests rooted at `path` (no lockfiles are read).
    pub async fn load_from_manifests(
        path: &PackagePath,
    ) -> PackageResult<BTreeMap<EnvironmentName, Self>> {
        let manifest = Manifest::<F>::read_from_file(path.manifest_path())?;
        let envs = manifest.environments();
        let mut output = BTreeMap::new();

        for env in envs.keys() {
            debug!("Creating a PackageGraph for env {env}");
            output.insert(
                env.clone(),
                PackageGraphBuilder::<F>::new()
                    .load_from_manifests_by_env(path, env)
                    .await?,
            );
        }
        Ok(output)
    }

    /// Construct a [PackageGraph] by pinning and fetching all transitive dependencies from the
    /// manifests rooted at `path` (no lockfiles are read) for the passed environment.
    pub async fn load_from_manifest_by_env(
        path: &PackagePath,
        env: &EnvironmentName,
    ) -> PackageResult<Self> {
        PackageGraphBuilder::new()
            .load_from_manifests_by_env(path, env)
            .await
    }

    /// Read a [PackageGraph] from a lockfile, ignoring manifest digests. Primarily useful for
    /// testing - you will usually want [Self::load].
    /// TODO: probably want to take a path to the lockfile
    pub async fn load_from_lockfile_ignore_digests(
        path: &PackagePath,
        env: &EnvironmentName,
    ) -> PackageResult<Option<Self>> {
        PackageGraphBuilder::new()
            .load_from_lockfile_ignore_digests(path, env)
            .await
    }
}
