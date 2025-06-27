// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dependency::PinnedDependencyInfo,
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{EnvironmentName, Package, lockfile::Lockfiles, paths::PackagePath},
    schema::PackageName,
};

use std::{
    collections::{BTreeMap, btree_map::Entry},
    fs::read_to_string,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use petgraph::graph::{DiGraph, NodeIndex};
use tokio::sync::OnceCell;

use super::{PackageGraph, PackageNode};

struct PackageCache<F: MoveFlavor> {
    // TODO: better errors; I'm using Option for now because PackageResult doesn't have clone, but
    // it's too much effort to add clone everywhere; we should do this when we update the error
    // infra
    // TODO: would dashmap simplify this?
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

    /// Loads the package graph for `env`. It checks whether the
    /// resolution graph in the lockfile is up-to-date (i.e., whether any of the
    /// manifests digests are out of date). If the resolution graph is up-to-date, it is returned.
    /// Otherwise a new resolution graph is constructed by traversing (only) the manifest files.
    pub async fn load(
        &self,
        path: &PackagePath,
        env: &EnvironmentName,
    ) -> PackageResult<PackageGraph<F>> {
        let lockfile = self.load_from_lockfile(path, env).await?;
        match lockfile {
            Some(result) => Ok(result),
            None => self.load_from_manifests_by_env(path, env).await,
        }
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if the contents of the
    /// lockfile are out of date (i.e. if the lockfile doesn't exist or the manifest digests don't
    /// match).
    pub async fn load_from_lockfile(
        &self,
        path: &PackagePath,
        env: &EnvironmentName,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        self.load_from_lockfile_impl(path, env, true).await
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if there is no lockfile
    pub async fn load_from_lockfile_ignore_digests(
        &self,
        path: &PackagePath,
        env: &EnvironmentName,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        self.load_from_lockfile_impl(path, env, false).await
    }

    /// Load a [PackageGraph] from the lockfile at `path`. Returns [None] if there is no lockfile.
    /// Also returns [None] if `check_digests` is true and any of the digests don't match.
    pub async fn load_from_lockfile_impl(
        &self,
        path: &PackagePath,
        env: &EnvironmentName,
        check_digests: bool,
    ) -> PackageResult<Option<PackageGraph<F>>> {
        let Some(lockfile) = Lockfiles::<F>::read_from_dir(path)? else {
            return Ok(None);
        };

        let mut graph = PackageGraph {
            inner: DiGraph::new(),
        };

        let pins = lockfile.pins_for_env(env);

        let mut package_nodes = BTreeMap::new();
        if let Some(pins) = pins {
            // First pass: create nodes for all packages
            for (pkg_id, pin) in pins.iter() {
                let dep = PinnedDependencyInfo::from_pin(lockfile.file(), env, pin);
                let package = self.cache.fetch(&dep).await?;
                let package_manifest_digest = package.manifest().digest();
                if check_digests && package_manifest_digest != &pin.manifest_digest {
                    return Ok(None);
                }
                let index = graph.inner.add_node(PackageNode {
                    package,
                    use_env: todo!(),
                });
                package_nodes.insert(pkg_id.clone(), index);
            }

            // Second pass: add edges based on dependencies
            for (pkg_id, dep_info) in pins.iter() {
                let from_index = package_nodes.get(pkg_id).unwrap();
                for (dep_name, dep_id) in dep_info.deps.iter() {
                    if let Some(to_index) = package_nodes.get(dep_id) {
                        graph
                            .inner
                            .add_edge(*from_index, *to_index, dep_name.clone());
                    }
                }
            }
        }

        Ok(Some(graph))
    }

    /// Construct a new package graph for `env` by recursively fetching and reading manifest files
    /// starting from the package at `path`.
    /// Lockfiles are ignored. See [PackageGraph::load]
    pub async fn load_from_manifests_by_env(
        &self,
        path: &PackagePath,
        env: &EnvironmentName,
    ) -> PackageResult<PackageGraph<F>> {
        // TODO: this is wrong - it is ignoring `path`
        let graph = Arc::new(Mutex::new(DiGraph::new()));
        let visited = Arc::new(Mutex::new(BTreeMap::new()));
        let root = Arc::new(Package::<F>::load_root(path).await?);

        self.add_transitive_manifest_deps(root, env, graph.clone(), visited)
            .await?;

        let graph = graph.lock().expect("unpoisoned").map(
            |_, node| {
                node.clone()
                    .expect("add_transitive_packages removes all `None`s before returning")
            },
            |_, e| e.clone(),
        );

        Ok(PackageGraph { inner: graph })
    }

    /// Adds nodes and edges for the graph rooted at `package` to `graph` and returns the node ID for
    /// `package`. Nodes are constructed by fetching the dependencies. If this function returns successfully,
    /// all nodes that it adds to `graph` will be set to `Some`.
    ///
    /// `visited` is used to short-circuit refetching - if a node is in `visited` then neither it nor its
    /// dependencies will be readded.
    pub async fn add_transitive_manifest_deps(
        &self,
        package: Arc<Package<F>>,
        env: &EnvironmentName,
        graph: Arc<Mutex<DiGraph<Option<PackageNode<F>>, PackageName>>>,
        visited: Arc<Mutex<BTreeMap<(EnvironmentName, PathBuf), NodeIndex>>>,
    ) -> PackageResult<NodeIndex> {
        // return early if node is cached; add empty node to graph and visited list otherwise
        let index = match visited
            .lock()
            .expect("unpoisoned")
            .entry((env.clone(), package.path().as_ref().to_path_buf()))
        {
            Entry::Occupied(entry) => return Ok(*entry.get()),
            Entry::Vacant(entry) => *entry.insert(graph.lock().expect("unpoisoned").add_node(None)),
        };

        // add outgoing edges for dependencies
        // Note: this loop could be parallel if we want parallel fetching:
        for (name, dep) in package.direct_deps(env).await?.iter() {
            let fetched = self.cache.fetch(dep).await?;
            let future = self.add_transitive_manifest_deps(
                fetched,
                dep.use_environment(),
                graph.clone(),
                visited.clone(),
            );
            let dep_index = Box::pin(future).await?;

            graph
                .lock()
                .expect("unpoisoned")
                .add_edge(index, dep_index, name.clone());
        }

        graph
            .lock()
            .expect("unpoisoned")
            .node_weight_mut(index)
            .expect("node was added above")
            .replace(PackageNode {
                package,
                use_env: env.clone(),
            });

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
    pub async fn fetch(&self, dep: &PinnedDependencyInfo) -> PackageResult<Arc<Package<F>>> {
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
        match Package::load(dep.clone()).await {
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
