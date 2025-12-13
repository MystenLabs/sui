// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dependency::{Pinned, PinnedDependencyInfo},
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    logging::user_note,
    package::{
        EnvironmentName, Package, lockfile::Lockfiles, package_lock::PackageSystemLock,
        paths::PackagePath,
    },
    schema::{Environment, PackageID, PackageName},
};

use std::{
    collections::{BTreeMap, btree_map::Entry},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use bimap::BiBTreeMap;
use petgraph::graph::{DiGraph, NodeIndex};
use thiserror::Error;
use tokio::sync::OnceCell;
use tracing::debug;

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

struct PackageCache<F: MoveFlavor> {
    // TODO: better errors; I'm using Option for now because PackageResult doesn't have clone, but
    // it's too much effort to add clone everywhere; we should do this when we update the error
    // infra
    // TODO: would dashmap simplify this?
    #[allow(clippy::type_complexity)]
    cache: Mutex<BTreeMap<PathBuf, Arc<OnceCell<Option<Arc<Package<F>>>>>>>,
}

pub struct PackageGraphBuilder<F: MoveFlavor> {
    cache: PackageCache<F>,
}

impl<F: MoveFlavor> PackageGraphBuilder<F> {
    pub fn new() -> Self {
        Self {
            cache: PackageCache::new(),
        }
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if the contents of the
    /// lockfile are out of date (i.e. if the lockfile doesn't exist or the manifest digests don't
    /// match).
    pub async fn load_from_lockfile(
        &self,
        path: &PackagePath,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        self.load_from_lockfile_impl(path, env, true, mtx).await
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if there is no lockfile
    pub async fn load_from_lockfile_ignore_digests(
        &self,
        path: &PackagePath,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        self.load_from_lockfile_impl(path, env, false, mtx).await
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if there is no lockfile.
    /// Also returns [None] if `check_digests` is true and any of the digests don't match.
    pub async fn load_from_lockfile_impl(
        &self,
        path: &PackagePath,
        env: &Environment,
        check_digests: bool,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        // TODO: this function is too long
        let Some(lockfile) = Lockfiles::read_from_dir::<F>(path, mtx)? else {
            return Ok(None);
        };

        let Some(pins) = lockfile.pins_for_env(env.name()) else {
            return Ok(None);
        };

        let mut inner = DiGraph::new();
        let mut package_ids: BiBTreeMap<PackageID, NodeIndex> = BiBTreeMap::new();
        let mut root_index = None;

        // First pass: create nodes for all packages
        for (pkg_id, pin) in pins.iter() {
            let dep = Pinned::from_lockfile(lockfile.file(), &pin.source)?;
            let package = self.cache.fetch(&dep, env, mtx).await?;
            let package_manifest_digest = package.digest();
            if check_digests && package_manifest_digest != &pin.manifest_digest {
                user_note!(
                    "Updating dependencies for `{}` environment because {:?} has been changed since the last update.",
                    env.name(),
                    package.path().path().join("Move.toml")
                );
                return Ok(None);
            }
            let index = inner.add_node(package.clone());
            package_ids.insert(pkg_id.clone(), index);
            if dep.is_root() {
                let old_root = root_index.replace(index);
                if old_root.is_some() {
                    return Err(LockfileError::MultipleRootNodes {
                        env: env.name().clone(),
                    }
                    .into());
                }
            }
        }

        let root_index = root_index.ok_or(LockfileError::NoRootNode)?;

        debug!("loaded packages from lockfile: {package_ids:?}");

        // Second pass: add edges based on dependencies
        for (source_id, source_pin) in pins.iter() {
            let source_index = package_ids.get_by_left(source_id).unwrap();
            let source_package = inner[*source_index].clone();

            for dep in source_package.direct_deps() {
                let dep_name = dep.name();
                let target_id = source_pin
                    .deps
                    .get(dep_name)
                    .ok_or(LockfileError::MissingDep {
                        source_id: source_id.clone(),
                        dep_name: dep_name.clone(),
                    })?;

                let target_index =
                    package_ids
                        .get_by_left(target_id)
                        .ok_or(LockfileError::UndefinedDep {
                            target_id: target_id.clone(),
                        })?;

                let pin = inner
                    .node_weight(*target_index)
                    .expect("node exists")
                    .dep_for_self()
                    .clone();

                let dep = PinnedDependencyInfo::from_combined(dep.clone(), pin);

                inner.add_edge(*source_index, *target_index, dep.clone());
            }
        }

        Ok(Some(PackageGraph {
            inner,
            root_index,
            package_ids,
        }))
    }

    /// Construct a new package graph for `env` by recursively fetching and reading manifest files
    /// starting from the package at `path`.
    /// Lockfiles are ignored. See [PackageGraph::load]
    pub async fn load_from_manifests(
        &self,
        path: &PackagePath,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<PackageGraph<F>> {
        let graph = Arc::new(Mutex::new(DiGraph::new()));

        let root = self
            .cache
            .fetch(&Pinned::Root(path.clone()), env, mtx)
            .await?;

        // TODO: should we add `root` to `visited`? we may have a problem if there is a cyclic
        // dependency involving the root

        let visited = Arc::new(Mutex::new(BTreeMap::new()));

        let root_index = self
            .add_transitive_manifest_deps(root, env, graph.clone(), visited, mtx)
            .await?;

        let inner: DiGraph<Arc<Package<F>>, PinnedDependencyInfo> =
            graph.lock().expect("unpoisoned").map(
                |_, node| {
                    node.clone()
                        .expect("add_transitive_packages removes all `None`s before returning")
                },
                |_, e| e.clone(),
            );

        let package_ids = Self::create_ids(&inner);
        Ok(PackageGraph {
            inner,
            package_ids,
            root_index,
        })
    }

    /// Assign unique identifiers to each node. In the case that there is no overlap, the
    /// identifier should be the same as the package's name.
    fn create_ids(
        graph: &DiGraph<Arc<Package<F>>, PinnedDependencyInfo>,
    ) -> BiBTreeMap<PackageID, NodeIndex> {
        let mut name_to_suffix: BTreeMap<PackageName, u8> = BTreeMap::new();
        let mut node_to_id: BiBTreeMap<PackageID, NodeIndex> = BiBTreeMap::new();

        // TODO: maybe we need to be more deterministic about disambiguation? In particular, the ID
        // we generate depends on the iteration order, which may be nondeterministic. If we're
        // exposing this in any way (e.g. using the IDs to index ephemeral addresses) then a switch
        // could lead to confusion.
        //
        // Of course repinning will change these names too, so something more stable might be to
        // use the inclusion paths as indices (but this may still depend on order?)

        // build index to id map
        for node in graph.node_indices() {
            let pkg_node = graph.node_weight(node).expect("node exists");
            let name = if let Some(legacy_data) = &pkg_node.legacy_data {
                &legacy_data.normalized_legacy_name
            } else {
                pkg_node.name()
            };
            let suffix = name_to_suffix.entry(name.clone()).or_default();
            let id = if *suffix == 0 {
                name.to_string()
            } else {
                format!("{}_{suffix}", name)
            };
            node_to_id.insert(id, node);
            *suffix += 1;
        }

        node_to_id
    }

    /// Adds nodes and edges for the graph rooted at `package` to `graph` and returns the node ID for
    /// `package`. Nodes are constructed by fetching the dependencies. If this function returns successfully,
    /// all nodes that it adds to `graph` will be set to `Some`.
    ///
    /// `visited` is used to short-circuit refetching - if a node is in `visited` then neither it nor its
    /// dependencies will be readded.
    #[allow(clippy::type_complexity)] // TODO
    pub async fn add_transitive_manifest_deps(
        &self,
        package: Arc<Package<F>>,
        env: &Environment,
        graph: Arc<Mutex<DiGraph<Option<Arc<Package<F>>>, PinnedDependencyInfo>>>,
        visited: Arc<Mutex<BTreeMap<(EnvironmentName, PackagePath), NodeIndex>>>,
        mtx: &PackageSystemLock,
    ) -> PackageResult<NodeIndex> {
        // return early if node is cached; add empty node to graph and visited list otherwise
        let index = match visited
            .lock()
            .expect("unpoisoned")
            .entry((env.name().clone(), package.path().clone()))
        {
            Entry::Occupied(entry) => return Ok(*entry.get()),
            Entry::Vacant(entry) => *entry.insert(graph.lock().expect("unpoisoned").add_node(None)),
        };

        // pin dependencies
        let pinned = PinnedDependencyInfo::pin::<F>(
            package.dep_for_self(),
            package.direct_deps().clone(),
            env.id(),
        )
        .await
        .map_err(|err| PackageError::DepError {
            dep: package
                .dep_for_self()
                .unfetched_path()
                .to_string_lossy()
                .to_string(),
            err: Box::new(err),
        })?;

        // add outgoing edges for dependencies
        // Note: this loop could be parallel if we want parallel fetching:
        for dep in pinned {
            let fetched = self.cache.fetch(dep.as_ref(), env, mtx).await?;

            // We retain the defined environment name, but we assign a consistent chain id (environmentID).
            let new_env = Environment::new(dep.use_environment().clone(), env.id().clone());

            let future = self.add_transitive_manifest_deps(
                fetched.clone(),
                &new_env,
                graph.clone(),
                visited.clone(),
                mtx,
            );
            let dep_index = Box::pin(future).await?;

            graph
                .lock()
                .expect("unpoisoned")
                .add_edge(index, dep_index, dep.clone());
        }

        graph
            .lock()
            .expect("unpoisoned")
            .node_weight_mut(index)
            .expect("node was added above")
            .replace(package);

        Ok(index)
    }
}

impl<F: MoveFlavor> PackageCache<F> {
    /// Construct a new empty cache
    pub fn new() -> Self {
        Self {
            cache: Mutex::default(),
        }
    }

    /// Return a reference to a cached [Package], loading it if necessary
    pub async fn fetch(
        &self,
        dep: &Pinned,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Arc<Package<F>>> {
        let cell = self
            .cache
            .lock()
            .expect("unpoisoned")
            .entry(dep.unfetched_path())
            .or_default()
            .clone();

        // TODO: this refetches if there was a previous error, it should save the error instead

        // First try to get cached result
        if let Some(Some(cached)) = cell.get() {
            return Ok(cached.clone());
        }

        // If not cached, load and cache
        match Package::load(dep.clone(), env, mtx).await {
            Ok(package) => {
                let node = Arc::new(package);
                cell.get_or_init(async || Some(node.clone())).await;
                Ok(node)
            }
            Err(e) => Err(PackageError::DepError {
                dep: dep.unfetched_path().display().to_string(),
                err: Box::new(e),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO: add a tests with a cyclic dependency involving the root
}
