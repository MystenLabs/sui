// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dependency::PinnedDependencyInfo,
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{EnvironmentName, Package, lockfile::Lockfiles, paths::PackagePath},
    schema::Environment,
};

use std::{
    collections::{BTreeMap, btree_map::Entry},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use petgraph::graph::{DiGraph, NodeIndex};
use tokio::sync::OnceCell;

use super::{PackageGraph, PackageGraphEdge};

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
    ) -> PackageResult<Option<PackageGraph<F>>> {
        self.load_from_lockfile_impl(path, env, true).await
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if there is no lockfile
    pub async fn load_from_lockfile_ignore_digests(
        &self,
        path: &PackagePath,
        env: &Environment,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        self.load_from_lockfile_impl(path, env, false).await
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if there is no lockfile.
    /// Also returns [None] if `check_digests` is true and any of the digests don't match.
    pub async fn load_from_lockfile_impl(
        &self,
        path: &PackagePath,
        env: &Environment,
        check_digests: bool,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        let Some(lockfile) = Lockfiles::<F>::read_from_dir(path)? else {
            return Ok(None);
        };

        let mut inner = DiGraph::new();
        let mut package_nodes = BTreeMap::new();
        let mut root_index = None;

        let Some(pins) = lockfile.pins_for_env(env.name()) else {
            return Ok(None);
        };

        // First pass: create nodes for all packages
        for (pkg_id, pin) in pins.iter() {
            let dep = PinnedDependencyInfo::from_lockfile(lockfile.file(), env.name(), pin)?;
            let package = self.cache.fetch(&dep, env).await?;
            let package_manifest_digest = package.digest();
            if check_digests && package_manifest_digest != &pin.manifest_digest {
                return Ok(None);
            }
            let index = inner.add_node(package.clone());
            package_nodes.insert(pkg_id.clone(), index);
            if package.is_root() {
                let old_root = root_index.replace(index);
                if old_root.is_some() {
                    return Err(PackageError::Generic(format!(
                        "Invalid lockfile: there are multiple root nodes in environment {}",
                        env.name()
                    )));
                }
            }
        }

        let root_index = root_index.ok_or(PackageError::Generic(
            "Invalid lockfile: there is no root node".into(),
        ))?;

        // Second pass: add edges based on dependencies
        for (source_id, source_pin) in pins.iter() {
            let source_index = package_nodes.get(source_id).unwrap();

            for (name, target_id) in source_pin.deps.iter() {
                let target_index = package_nodes
                    .get(target_id)
                    .ok_or(PackageError::Generic(format!(
                        "Invalid lockfile: package depends on a package with undefined ID `{target_id}`"
                    )))?;

                let target = &inner[*target_index];

                // we assume that the override and rename-from checks have already been performed,
                // so we go ahead an update the override and rename-from fields to values that we
                // know will pass the rename-from checks
                let dep = target
                    .dep_for_self()
                    .clone()
                    .with_rename_from(target.name().clone())
                    .with_override(true);

                inner.add_edge(
                    *source_index,
                    *target_index,
                    PackageGraphEdge {
                        name: name.clone(),
                        dep,
                    },
                );
            }
        }

        Ok(Some(PackageGraph { inner, root_index }))
    }

    /// Construct a new package graph for `env` by recursively fetching and reading manifest files
    /// starting from the package at `path`.
    /// Lockfiles are ignored. See [PackageGraph::load]
    pub async fn load_from_manifests(
        &self,
        path: &PackagePath,
        env: &Environment,
    ) -> PackageResult<PackageGraph<F>> {
        let graph = Arc::new(Mutex::new(DiGraph::new()));
        let root = Arc::new(Package::<F>::load_root(path, env).await?);

        // TODO: should we add `root` to `visited`? we may have a problem if there is a cyclic
        // dependency involving the root

        let visited = Arc::new(Mutex::new(BTreeMap::new()));

        let root_idx = self
            .add_transitive_manifest_deps(root, env, graph.clone(), visited)
            .await?;

        let graph: DiGraph<Arc<Package<F>>, PackageGraphEdge> =
            graph.lock().expect("unpoisoned").map(
                |_, node| {
                    node.clone()
                        .expect("add_transitive_packages removes all `None`s before returning")
                },
                |_, e| e.clone(),
            );

        Ok(PackageGraph {
            inner: graph,
            root_index: root_idx,
        })
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
        graph: Arc<Mutex<DiGraph<Option<Arc<Package<F>>>, PackageGraphEdge>>>,
        visited: Arc<Mutex<BTreeMap<(EnvironmentName, PathBuf), NodeIndex>>>,
    ) -> PackageResult<NodeIndex> {
        // return early if node is cached; add empty node to graph and visited list otherwise
        let index = match visited
            .lock()
            .expect("unpoisoned")
            .entry((env.name().clone(), package.path().as_ref().to_path_buf()))
        {
            Entry::Occupied(entry) => return Ok(*entry.get()),
            Entry::Vacant(entry) => *entry.insert(graph.lock().expect("unpoisoned").add_node(None)),
        };

        // add outgoing edges for dependencies
        // Note: this loop could be parallel if we want parallel fetching:
        for (name, dep) in package.direct_deps().iter() {
            let fetched = self.cache.fetch(dep, env).await?;

            // We retain the defined environment name, but we assign a consistent chain id (environmentID).
            let new_env = Environment::new(dep.use_environment().clone(), env.id().clone());

            let future = self.add_transitive_manifest_deps(
                fetched.clone(),
                &new_env,
                graph.clone(),
                visited.clone(),
            );
            let dep_index = Box::pin(future).await?;

            // TODO(manos): re-check the implementation here --  to make sure nothing was missed.
            // TODO(manos)(2): Do we wanna error for missmatches on legacy packages? Will come on a follow-up.
            // TODO(manos)(3): Do we wanna rename only for legacy parents, and error out for modern parents?
            // If we're dealing with legacy packages, we are free to fix the naming in the outgoing edge, to match
            // our modern system names.
            let edge_name = if fetched.is_legacy() {
                fetched.name()
            } else {
                name
            };

            graph.lock().expect("unpoisoned").add_edge(
                index,
                dep_index,
                PackageGraphEdge {
                    name: edge_name.clone(),
                    dep: dep.clone(),
                },
            );
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
        dep: &PinnedDependencyInfo,
        env: &Environment,
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
        match Package::load(dep.clone(), env).await {
            Ok(package) => {
                let node = Arc::new(package);
                cell.get_or_init(async || Some(node.clone())).await;
                Ok(node)
            }
            Err(e) => Err(PackageError::Generic(format!(
                "Failed to load package from {}: {}",
                dep.unfetched_path().display(),
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO: add a tests with a cyclic dependency involving the root
}
