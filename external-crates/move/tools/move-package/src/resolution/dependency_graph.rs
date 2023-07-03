// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use colored::Colorize;
use move_symbol_pool::Symbol;
use petgraph::{algo, prelude::DiGraphMap, Direction};
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, VecDeque},
    fmt,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    package_hooks,
    source_package::{
        layout::SourcePackageLayout,
        manifest_parser::{
            parse_dependency, parse_move_manifest_string, parse_source_manifest, parse_substitution,
        },
        parsed_manifest as PM,
    },
};

use super::{
    dependency_cache::DependencyCache,
    digest::{digest_str, hashed_files_digest},
    local_path,
    lock_file::{schema, LockFile},
};

/// A representation of the transitive dependency graph of a Move package.  If successfully created,
/// the resulting graph:
///
/// - is directed, acyclic and `BuildConfig` agnostic,
/// - mentions each package at most once (i.e. no duplicate packages), and
/// - contains information about the source of every package (excluding the root package).
///
/// It can be built by recursively exploring a package's dependencies, fetching their sources if
/// necessary, or by reading its serialized contents from a lock file (provided either alongside its
/// manifest file in storage or by the external resolver).  Both these processes will fail if any of
/// the criteria above cannot be met (e.g. if the graph contains a cycle, the same package is
/// fetched multiple times from different sources, or information about a package's source is not
/// available).
///
/// In order to be `BuildConfig` agnostic, it contains `dev-dependencies` as well as `dependencies`
/// and labels edges in the graph accordingly, as `DevOnly`, or `Always` dependencies.
///
/// When building a dependency graph, different versions of the same (transitively) dependent
/// package can be encountered. If this is indeed the case, a single version must be chosen by the
/// developer to be the override, and this override must be specified in a manifest file whose
/// package dominates all the conflicting "uses" of the dependent package. These overrides are taken
/// into consideration during the dependency graph construction - sub-graphs being combined into the
/// main graph are pruned based on the information about overrides (a package in a graph is pruned
/// if its dominated by another overridden package).
///
/// If an up-to-date lock file for the dependency graph being constructed is not available, the
/// graph construction proceeds bottom-up, by either reading sub-graphs from their respective lock
/// files (if they are up-to-date) or by constructing sub-graphs by exploring all their (direct and
/// indirect) dependencies specified in manifest files. These sub-graphs are then successively
/// merged into larger graphs until the main combined graph is computed.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Path to the root package and its name (according to its manifest)
    pub root_path: PathBuf,
    pub root_package: PM::PackageName,

    /// Transitive dependency graph, with dependency edges `P -> Q` labelled according to whether Q
    /// is always a dependency of P or only in dev-mode.
    pub package_graph: DiGraphMap<PM::PackageName, Dependency>,

    /// The dependency that each package (keyed by name) originates from.  The root package is the
    /// only node in `package_graph` that does not have an entry in `package_table`.
    pub package_table: BTreeMap<PM::PackageName, Package>,

    /// Packages that are transitive dependencies regardless of mode (the transitive closure of
    /// `DependencyMode::Always` edges in `package_graph`).
    pub always_deps: BTreeSet<PM::PackageName>,

    /// A hash of the manifest file content this lock file was generated from, if any.
    pub manifest_digest: Option<String>,
    /// A hash of all the dependencies (their lock file content) this lock file depends on, if any.
    pub deps_digest: Option<String>,
}

#[derive(Debug, Clone, Eq, Ord, PartialOrd)]
pub struct Package {
    pub kind: PM::DependencyKind,
    pub version: Option<PM::Version>,
    /// Optional field set if the package was externally resolved.
    resolver: Option<Symbol>,
}

impl PartialEq for Package {
    fn eq(&self, other: &Self) -> bool {
        // comparison omit the type of resolver (as it would actually lead to incorrect result when
        // comparing packages during insertion of externally resolved ones - an internally resolved
        // existing package in the graph would not be recognized as a potential different version of
        // the externally resolved one)
        self.kind == other.kind && self.version == other.version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dependency {
    pub mode: DependencyMode,
    pub subst: Option<PM::Substitution>,
    pub digest: Option<PM::PackageDigest>,
    pub dep_override: PM::DepOverride,
}

/// Indicates whether one package always depends on another, or only in dev-mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyMode {
    Always,
    DevOnly,
}

/// Wrapper struct to display a package as an inline table in the lock file (matching the
/// convention in the source manifest).  This is necessary becase the `toml` crate does not
/// currently support serializing types as inline tables.
struct PackageTOML<'a>(&'a Package);
struct PackageWithResolverTOML<'a>(&'a Package);
struct DependencyTOML<'a>(PM::PackageName, &'a Dependency);
struct SubstTOML<'a>(&'a PM::Substitution);

/// A builder for `DependencyGraph`
pub struct DependencyGraphBuilder<Progress: Write> {
    /// Used to avoid re-fetching dependencies that are already present locally
    pub dependency_cache: DependencyCache,
    /// Logger
    pub progress_output: Progress,
    /// A chain of visited dependencies used for cycle detection
    visited_dependencies: VecDeque<(PM::PackageName, PM::InternalDependency)>,
}

impl<Progress: Write> DependencyGraphBuilder<Progress> {
    pub fn new(skip_fetch_latest_git_deps: bool, progress_output: Progress) -> Self {
        DependencyGraphBuilder {
            dependency_cache: DependencyCache::new(skip_fetch_latest_git_deps),
            progress_output,
            visited_dependencies: VecDeque::new(),
        }
    }

    /// Get a graph from the Move.lock file, if Move.lock file is present and up-to-date
    /// (additionally returning false), otherwise compute a new graph based on the content of the
    /// Move.toml (manifest) file (additionally returning true).
    pub fn get_graph(
        &mut self,
        parent: &PM::DependencyKind,
        root_path: PathBuf,
        manifest_string: String,
        lock_string: Option<String>,
    ) -> Result<(DependencyGraph, bool)> {
        let toml_manifest = parse_move_manifest_string(manifest_string.clone())?;
        let manifest = parse_source_manifest(toml_manifest)?;

        // compute digests eagerly as even if we can't reuse existing lock file, they need to become
        // part of the newly computed dependency graph
        let new_manifest_digest = digest_str(manifest_string.into_bytes().as_slice());
        let new_deps_digest_opt = self.dependency_digest(root_path.clone(), &manifest)?;
        if let Some(lock_contents) = lock_string {
            let schema::Header {
                version: _,
                manifest_digest: manifest_digest_opt,
                deps_digest: deps_digest_opt,
            } = schema::read_header(&lock_contents)?;

            // check if manifest file and dependencies haven't changed and we can use existing lock
            // file to create the dependency graph
            if Some(new_manifest_digest.clone()) == manifest_digest_opt {
                // manifest file hasn't changed
                if let Some(deps_digest) = deps_digest_opt {
                    // dependencies digest exists in the lock file
                    if Some(deps_digest) == new_deps_digest_opt {
                        // dependencies have not changed
                        return Ok((
                            DependencyGraph::read_from_lock(
                                root_path,
                                manifest.package.name,
                                &mut lock_contents.as_bytes(),
                                None,
                            )?,
                            false,
                        ));
                    }
                }
            }
        }

        Ok((
            self.new_graph(
                parent,
                &manifest,
                root_path.to_path_buf(),
                Some(new_manifest_digest),
                new_deps_digest_opt,
            )?,
            true,
        ))
    }

    /// Build a graph from the transitive dependencies and dev-dependencies of `root_package`.
    ///
    /// `progress_output` is an output stream that is written to while generating the graph, to
    /// provide human-readable progress updates.
    pub fn new_graph(
        &mut self,
        parent: &PM::DependencyKind,
        root_manifest: &PM::SourceManifest,
        root_path: PathBuf,
        manifest_digest: Option<String>,
        deps_digest: Option<String>,
    ) -> Result<DependencyGraph> {
        // collect sub-graphs for "regular" and "dev" dependencies
        let mut dep_graphs = self.collect_graphs(
            parent,
            root_manifest.package.name,
            root_path.clone(),
            DependencyMode::Always,
            &root_manifest.dependencies,
        )?;
        let mut dev_dep_graphs = self.collect_graphs(
            parent,
            root_manifest.package.name,
            root_path.clone(),
            DependencyMode::DevOnly,
            &root_manifest.dev_dependencies,
        )?;

        let mut combined_graph = DependencyGraph {
            root_path: root_path.clone(),
            root_package: root_manifest.package.name,
            package_graph: DiGraphMap::new(),
            package_table: BTreeMap::new(),
            always_deps: BTreeSet::new(),
            manifest_digest,
            deps_digest,
        };
        // ensure there's always a root node, even if it has no edges
        combined_graph
            .package_graph
            .add_node(combined_graph.root_package);

        // get overrides
        let overrides = DependencyGraphBuilder::<Progress>::collect_overrides(
            parent,
            &root_manifest.dependencies,
        )?;
        let dev_overrides = DependencyGraphBuilder::<Progress>::collect_overrides(
            parent,
            &root_manifest.dev_dependencies,
        )?;

        for (_, (g, is_override)) in dep_graphs.iter_mut() {
            if !*is_override {
                g.prune_overriden_pkgs(
                    root_manifest.package.name,
                    g.root_package,
                    DependencyMode::Always,
                    &overrides,
                    &dev_overrides,
                )?;
            }
        }
        for (_, (g, is_override)) in dev_dep_graphs.iter_mut() {
            if !*is_override {
                g.prune_overriden_pkgs(
                    root_manifest.package.name,
                    g.root_package,
                    DependencyMode::DevOnly,
                    &overrides,
                    &dev_overrides,
                )?;
            }
        }

        combined_graph.merge(
            dep_graphs,
            DependencyMode::Always,
            root_manifest.package.name,
            parent,
            &root_manifest.dependencies,
        )?;
        combined_graph.merge(
            dev_dep_graphs,
            DependencyMode::DevOnly,
            root_manifest.package.name,
            parent,
            &root_manifest.dev_dependencies,
        )?;

        combined_graph.check_acyclic()?;
        combined_graph.discover_always_deps();

        Ok(combined_graph)
    }

    /// Collects overridden dependencies.
    fn collect_overrides(
        parent: &PM::DependencyKind,
        dependencies: &PM::Dependencies,
    ) -> Result<BTreeMap<Symbol, Package>> {
        let mut overrides = BTreeMap::new();
        for (dep_pkg_name, dep) in dependencies {
            if let PM::Dependency::Internal(internal) = dep {
                if internal.dep_override {
                    let mut dep_pkg = Package {
                        kind: internal.kind.clone(),
                        version: internal.version,
                        resolver: None,
                    };
                    dep_pkg.kind.reroot(parent)?;
                    overrides.insert(*dep_pkg_name, dep_pkg);
                }
            }
        }
        Ok(overrides)
    }

    /// Given all dependencies from the parent manifest file, collects all the sub-graphs
    /// representing these dependencies - returns internally resolved sub-graphs (first return
    /// value) and externally resolved sub-graphs (second retrun value) separately.
    pub fn collect_graphs(
        &mut self,
        parent: &PM::DependencyKind,
        parent_pkg: PM::PackageName,
        root_path: PathBuf,
        mode: DependencyMode,
        dependencies: &PM::Dependencies,
    ) -> Result<BTreeMap<PM::PackageName, (DependencyGraph, bool)>> {
        let mut dep_graphs = BTreeMap::new();
        for (dep_pkg_name, dep) in dependencies {
            let (pkg_graph, is_override) = self
                .new_for_dep(
                    parent,
                    dep,
                    mode,
                    parent_pkg,
                    *dep_pkg_name,
                    root_path.clone(),
                )
                .with_context(|| {
                    format!(
                        "Failed to resolve dependencies for package '{}'",
                        parent_pkg
                    )
                })?;
            dep_graphs.insert(*dep_pkg_name, (pkg_graph, is_override));
        }
        Ok(dep_graphs)
    }

    /// Given a dependency in the parent's manifest file, creates a sub-graph for this dependency.
    fn new_for_dep(
        &mut self,
        parent: &PM::DependencyKind,
        dep: &PM::Dependency,
        mode: DependencyMode,
        parent_pkg: PM::PackageName,
        dep_pkg_name: PM::PackageName,
        dep_pkg_path: PathBuf,
    ) -> Result<(DependencyGraph, bool)> {
        let (pkg_graph, is_override) = match dep {
            PM::Dependency::Internal(d) => {
                DependencyGraph::check_for_dep_cycles(
                    d.clone(),
                    dep_pkg_name,
                    &mut self.visited_dependencies,
                )?;
                self.dependency_cache
                    .download_and_update_if_remote(dep_pkg_name, &d.kind, &mut self.progress_output)
                    .with_context(|| format!("Fetching '{}'", dep_pkg_name))?;
                let pkg_path = dep_pkg_path.join(local_path(&d.kind));
                let manifest_string =
                    std::fs::read_to_string(pkg_path.join(SourcePackageLayout::Manifest.path()))
                        .with_context(|| format!("Parsing manifest for '{}'", dep_pkg_name))?;
                let lock_string =
                    std::fs::read_to_string(pkg_path.join(SourcePackageLayout::Lock.path())).ok();
                // save dependency for cycle detection
                self.visited_dependencies
                    .push_front((dep_pkg_name, d.clone()));
                let (mut pkg_graph, _) =
                    self.get_graph(&d.kind, pkg_path.clone(), manifest_string, lock_string)?;
                self.visited_dependencies.pop_front();
                // reroot all packages to normalize local paths across all graphs
                for (_, p) in pkg_graph.package_table.iter_mut() {
                    p.kind.reroot(parent)?;
                }
                (pkg_graph, d.dep_override)
            }
            PM::Dependency::External(resolver) => {
                let pkg_graph = DependencyGraph::get_external(
                    mode,
                    parent_pkg,
                    dep_pkg_name,
                    *resolver,
                    &dep_pkg_path,
                    &mut self.progress_output,
                )?;
                (pkg_graph, false)
            }
        };
        Ok((pkg_graph, is_override))
    }

    /// Computes dependency hashes but may return None if information about some dependencies is not
    /// available.
    fn dependency_hashes(
        &mut self,
        root_path: PathBuf,
        dependencies: &PM::Dependencies,
    ) -> Result<Option<Vec<String>>> {
        let mut hashed_lock_files = Vec::new();

        for (pkg_name, dep) in dependencies {
            let internal_dep = match dep {
                // bail if encountering external dependency that would require running the external
                // resolver
                // TODO: should we consider handling this here?
                PM::Dependency::External(_) => return Ok(None),
                PM::Dependency::Internal(d) => d,
            };

            self.dependency_cache
                .download_and_update_if_remote(
                    *pkg_name,
                    &internal_dep.kind,
                    &mut self.progress_output,
                )
                .with_context(|| format!("Fetching '{}'", *pkg_name))?;
            let pkg_path = root_path.join(local_path(&internal_dep.kind));

            let Ok(lock_contents) = std::fs::read_to_string(pkg_path.join(SourcePackageLayout::Lock.path())) else {
            return Ok(None);
        };
            hashed_lock_files.push(digest_str(lock_contents.as_bytes()));
        }

        Ok(Some(hashed_lock_files))
    }

    /// Computes a digest of all dependencies in a manifest file but may return None if information
    /// about some dependencies is not available.
    fn dependency_digest(
        &mut self,
        root_path: PathBuf,
        manifest: &PM::SourceManifest,
    ) -> Result<Option<String>> {
        let Some(mut dep_hashes) = self.dependency_hashes(
            root_path.clone(),
            &manifest.dependencies,
        )? else {
            return Ok(None);
        };

        let Some(dev_dep_hashes) = self.dependency_hashes(
            root_path,
            &manifest.dev_dependencies,
        )? else {
            return Ok(None);
        };

        dep_hashes.extend(dev_dep_hashes);
        Ok(Some(hashed_files_digest(dep_hashes)))
    }
}

impl DependencyGraph {
    /// Finds packages in a sub-graph that should be pruned as a result of applying an override from
    /// the outer graph. A package should be pruned if it's dominated by an overridden package.
    fn find_pruned_pkgs(
        &self,
        pruned_pkgs: &mut BTreeMap<PM::PackageName, BTreeSet<PM::PackageName>>,
        reachable_pkgs: &mut BTreeSet<PM::PackageName>,
        root_pkg_name: PM::PackageName,
        from_pkg_name: PM::PackageName,
        mode: DependencyMode,
        overrides: &BTreeMap<PM::PackageName, Package>,
        dev_overrides: &BTreeMap<PM::PackageName, Package>,
        overridden_path: bool,
    ) -> Result<()> {
        if overridden_path {
            // we are on a path originating at the overridden package
            if !reachable_pkgs.contains(&from_pkg_name) {
                // not (yet) reached via regular (non-overridden) path - include in the list of
                // packages to be pruned but need to include a list of neighbors to correctly remove
                // edges going to the pruned package
                let neighbors = BTreeSet::from_iter(
                    self.package_graph
                        .neighbors_directed(from_pkg_name, Direction::Incoming),
                );
                pruned_pkgs.insert(from_pkg_name, neighbors);
            }
        } else {
            // we are on a regular path, not involving an override - if there was a package
            // candidate for pruning, it should be removed from the list as it can be reached via a
            // regular path
            pruned_pkgs.remove(&from_pkg_name);
        }

        let mut override_found = overridden_path;

        if !override_found {
            override_found = DependencyGraph::get_dep_override(
                root_pkg_name,
                from_pkg_name,
                overrides,
                dev_overrides,
                mode == DependencyMode::DevOnly,
            )?
            .is_some();

            if override_found {
                // we also prune overridden package but only its outgoing edges and not the incoming
                // ones (hence empty neighbors set)
                pruned_pkgs.insert(from_pkg_name, /* neighbors */ BTreeSet::new());
            }
        }

        for to_pkg_name in self
            .package_graph
            .neighbors_directed(from_pkg_name, Direction::Outgoing)
        {
            self.find_pruned_pkgs(
                pruned_pkgs,
                reachable_pkgs,
                root_pkg_name,
                to_pkg_name,
                mode,
                overrides,
                dev_overrides,
                override_found,
            )?;
        }
        Ok(())
    }

    /// Prunes packages in a sub-graph based on the overrides information from the outer graph.
    fn prune_overriden_pkgs(
        &mut self,
        root_pkg_name: PM::PackageName,
        from_pkg_name: PM::PackageName,
        mode: DependencyMode,
        overrides: &BTreeMap<PM::PackageName, Package>,
        dev_overrides: &BTreeMap<PM::PackageName, Package>,
    ) -> Result<()> {
        let mut pruned_pkgs = BTreeMap::new();
        let mut reachable_pkgs = BTreeSet::new();
        self.find_pruned_pkgs(
            &mut pruned_pkgs,
            &mut reachable_pkgs,
            root_pkg_name,
            from_pkg_name,
            mode,
            overrides,
            dev_overrides,
            false,
        )?;

        for (pkg, neighbors) in pruned_pkgs {
            // simply removing a node does not remove edges coming from its neighbors to this node
            for n in neighbors {
                self.package_graph.remove_edge(n, pkg);
            }
            self.package_graph.remove_node(pkg);
            self.package_table.remove(&pkg);
        }

        Ok(())
    }

    /// Given all sub-graphs representing dependencies of the parent manifest file, combines all
    /// subgraphs to form the parent dependency graph.
    pub fn merge(
        &mut self,
        dep_graphs: BTreeMap<PM::PackageName, (DependencyGraph, bool)>,
        mode: DependencyMode,
        root_package: PM::PackageName,
        parent: &PM::DependencyKind,
        dependencies: &PM::Dependencies,
    ) -> Result<()> {
        if !self.always_deps.is_empty() {
            bail!("Merging dependencies into a graph after calculating its 'always' dependencies");
        }

        // collect all package names in all graphs in package table
        let mut all_packges: BTreeSet<PM::PackageName> = BTreeSet::new();
        for (g, _) in (&dep_graphs).values() {
            all_packges.extend(g.package_table.keys());
        }

        // analyze all packages in all package tables of all sub-graphs to determine if any of these
        // packages represent a conflicting dependency; insert the packages and their respective
        // edges into the combined graph along the way
        for pkg_name in all_packges {
            let mut existing_pkg_info: Option<(Symbol, &DependencyGraph, &Package)> = None;
            for (dep_name, (g, _)) in &dep_graphs {
                if let Some(pkg) = g.package_table.get(&pkg_name) {
                    // graph g has a package with name pkg_name
                    if let Some((existing_dep_name, existing_graph, existing_pkg)) =
                        existing_pkg_info
                    {
                        // it's the subsequent time package with pkg_name has been encountered
                        if pkg == existing_pkg {
                            // both packages are the same but we need to check if their dependencies
                            // are the same as well
                            let (exisiting_pkg_deps, pkg_deps) =
                                deps_equal(pkg_name, &existing_graph, &g);
                            if exisiting_pkg_deps != pkg_deps {
                                bail!(
                                    "When resolving dependencies for package {}, \
                                     conflicting dependencies found for '{}':\n{}{}",
                                    root_package,
                                    pkg_name,
                                    format_deps(existing_dep_name, exisiting_pkg_deps),
                                    format_deps(*dep_name, pkg_deps),
                                )
                            }

                            continue;
                        } else {
                            bail!(
                                "When resolving dependencies for package {0}, \
                                 conflicting dependencies found:\nin {4}: {1} = {2}\nin {5}: {1} = {3}",
                                root_package,
                                pkg_name,
                                PackageWithResolverTOML(existing_pkg),
                                PackageWithResolverTOML(&pkg),
                                existing_dep_name,
                                dep_name,
                            );
                        }
                    } else {
                        // first time this package was encountered
                        existing_pkg_info = Some((*dep_name, g, pkg));
                    }
                }
            }
            if let Some((_, g, existing_pkg)) = existing_pkg_info {
                // update combined graph with the new package and its dependencies
                self.package_table.insert(pkg_name, existing_pkg.clone());
                for to_pkg_name in g
                    .package_graph
                    .neighbors_directed(pkg_name, Direction::Outgoing)
                {
                    // unwrap is safe as all edges have a Dependency weight
                    let sub_dep = g.package_graph.edge_weight(pkg_name, to_pkg_name).unwrap();
                    self.package_graph
                        .add_edge(pkg_name, to_pkg_name, sub_dep.clone());
                }
            }
        }

        // insert direct dependency edges and (if necessary) packages for the remaining graph nodes
        // (not present in package table)
        for (dep_name, (g, _)) in &dep_graphs {
            if let Some(dep) = dependencies.get(dep_name) {
                let internally_resolved =
                    self.insert_direct_dep(dep, root_package, *dep_name, g, mode, parent)?;
                // make sure that dependencies of the directly dependent package do not differ from
                // the dependencies of the same package in other sub-graphs (if any)
                for (other_dep_name, (other_g, _)) in &dep_graphs {
                    if dep_name != other_dep_name && other_g.package_graph.contains_node(*dep_name)
                    {
                        let (other_pkg_deps, pkg_deps) = deps_equal(*dep_name, &other_g, &g);
                        if other_pkg_deps != pkg_deps {
                            bail!(
                                "When resolving dependencies for package {}, \
                                         conflicting dependencies found for '{}':\n{}{}",
                                root_package,
                                dep_name,
                                format_deps(*other_dep_name, other_pkg_deps),
                                format_deps(*dep_name, pkg_deps),
                            )
                        }
                    }
                }

                if internally_resolved {
                    // insert edges from the directly dependent package to its neighbors for
                    // internally resolved sub-graphs - due to how external graphs are constructed,
                    // edges between directly dependent packages and their neighbors are already in
                    // the sub-graph and would have been inserted in the first loop in this function
                    for to_pkg_name in g
                        .package_graph
                        .neighbors_directed(*dep_name, Direction::Outgoing)
                    {
                        // unwrap is safe as all edges have a Dependency weight
                        let sub_dep = g.package_graph.edge_weight(*dep_name, to_pkg_name).unwrap();
                        self.package_graph
                            .add_edge(*dep_name, to_pkg_name, sub_dep.clone());
                    }
                }
            } else {
                bail!(
                    "Can't merge dependencies for '{}' because nothing depends on it",
                    dep_name
                );
            }
        }
        Ok(())
    }

    /// Inserts a single direct dependency with given (package) name representing a sub-graph into
    /// the combined graph. Returns true if the sub-graph has to be further merged into the combined
    /// graph and false if it does not (i.e., if the dependency is already represented in the
    /// combined graph).
    fn insert_direct_dep(
        &mut self,
        dep: &PM::Dependency,
        root_package: PM::PackageName,
        dep_pkg_name: PM::PackageName,
        sub_graph: &DependencyGraph,
        mode: DependencyMode,
        parent: &PM::DependencyKind,
    ) -> Result<bool> {
        match dep {
            PM::Dependency::Internal(internal) => {
                let PM::InternalDependency {
                    kind,
                    version,
                    subst,
                    digest,
                    dep_override,
                } = internal;

                match self.package_table.get(&dep_pkg_name) {
                    Some(_) => (),
                    None => {
                        let mut pkg = Package {
                            kind: kind.clone(),
                            version: version.clone(),
                            resolver: None,
                        };
                        pkg.kind.reroot(parent)?;
                        self.package_table.insert(dep_pkg_name, pkg);
                    }
                };
                self.package_graph.add_edge(
                    root_package,
                    dep_pkg_name,
                    Dependency {
                        mode,
                        subst: subst.clone(),
                        digest: digest.clone(),
                        dep_override: *dep_override,
                    },
                );
                Ok(true)
            }
            PM::Dependency::External(_) => {
                // the way that external graphs are constructed, edges between the (root) package of
                // the outer graph and dependencies in the sub-graph are already present in the
                // sub-graph
                let d = sub_graph
                    .package_graph
                    .edge_weight(root_package, dep_pkg_name)
                    .unwrap();
                self.package_graph
                    .add_edge(root_package, dep_pkg_name, d.clone());
                Ok(false)
            }
        }
    }

    /// Helper function to get overrides for "regular" dependencies (`dev_only` is false) or "dev"
    /// dependencies (`dev_only` is true).
    fn get_dep_override<'a>(
        root_pkg_name: PM::PackageName,
        pkg_name: PM::PackageName,
        overrides: &'a BTreeMap<Symbol, Package>,
        dev_overrides: &'a BTreeMap<Symbol, Package>,
        dev_only: bool,
    ) -> Result<Option<&'a Package>> {
        // for "regular" dependencies override can come only from "regular" dependencies section,
        // but for "dev" dependencies override can come from "regular" or "dev" dependencies section
        if let Some(pkg) = overrides.get(&pkg_name) {
            // "regular" dependencies section case
            if dev_overrides.get(&pkg_name).is_some() {
                bail!(
                    "Conflicting \"regular\" and \"dev\" overrides of {} in {}",
                    pkg_name,
                    root_pkg_name
                );
            }
            return Ok(Some(pkg));
        } else if let Some(dev_pkg) = dev_overrides.get(&pkg_name) {
            // "dev" dependencies section case
            return Ok(dev_only.then_some(dev_pkg));
        }
        Ok(None)
    }

    /// Cycle detection to avoid infinite recursion due to the way we construct internally resolved
    /// sub-graphs, expecting to end recursion at leaf packages that have no dependencies.
    fn check_for_dep_cycles(
        dep: PM::InternalDependency,
        dep_pkg_name: PM::PackageName,
        internal_dependencies: &mut VecDeque<(PM::PackageName, PM::InternalDependency)>,
    ) -> Result<()> {
        if internal_dependencies.contains(&(dep_pkg_name, dep.clone())) {
            let (mut processed_name, mut processed_dep) = internal_dependencies.pop_back().unwrap();
            while processed_name != dep_pkg_name || processed_dep != dep {
                (processed_name, processed_dep) = internal_dependencies.pop_back().unwrap();
            }
            // now the queue contains all intermediate dependencies
            let mut msg = "Found cycle between packages: ".to_string();
            msg.push_str(format!("{} -> ", dep_pkg_name).as_str());
            while !internal_dependencies.is_empty() {
                let (p, _) = internal_dependencies.pop_back().unwrap();
                msg.push_str(format!("{} -> ", p).as_str());
            }
            msg.push_str(format!("{}", dep_pkg_name).as_str());
            bail!(msg);
        }
        Ok(())
    }

    /// Creates a dependency graph by reading a lock file.
    ///
    /// The lock file is expected to contain a complete picture of the package's transitive
    /// dependency graph, which means it is not required to discover it through a recursive
    /// traversal.
    ///
    /// Expects the lock file to conform to the schema expected by this version of the compiler (in
    /// the `lock_file::schema` module).
    pub fn read_from_lock(
        root_path: PathBuf,
        root_package: PM::PackageName,
        lock: &mut impl Read,
        resolver: Option<Symbol>,
    ) -> Result<DependencyGraph> {
        let mut package_graph = DiGraphMap::new();
        let mut package_table = BTreeMap::new();

        let (
            packages,
            schema::Header {
                version: _,
                manifest_digest,
                deps_digest,
            },
        ) = schema::Packages::read(lock)?;

        // Ensure there's always a root node, even if it has no edges.
        package_graph.add_node(root_package);

        for schema::Dependency {
            name,
            subst,
            digest,
        } in packages.root_dependencies.into_iter().flatten()
        {
            package_graph.add_edge(
                root_package,
                Symbol::from(name),
                Dependency {
                    mode: DependencyMode::Always,
                    subst: subst.map(parse_substitution).transpose()?,
                    digest: digest.map(Symbol::from),
                    dep_override: false,
                },
            );
        }

        for schema::Dependency {
            name,
            subst,
            digest,
        } in packages.root_dev_dependencies.into_iter().flatten()
        {
            package_graph.add_edge(
                root_package,
                Symbol::from(name),
                Dependency {
                    mode: DependencyMode::DevOnly,
                    subst: subst.map(parse_substitution).transpose()?,
                    digest: digest.map(Symbol::from),
                    dep_override: false,
                },
            );
        }

        // Fill in the remaining dependencies, and the package source information from the lock
        // file.
        for schema::Package {
            name: pkg_name,
            source,
            dependencies,
            dev_dependencies,
        } in packages.packages.into_iter().flatten()
        {
            let pkg_name = PM::PackageName::from(pkg_name.as_str());
            let source = parse_dependency(pkg_name.as_str(), source)
                .with_context(|| format!("Deserializing dependency '{pkg_name}'"))?;

            let source = match source {
                PM::Dependency::Internal(source) => source,
                PM::Dependency::External(resolver) => {
                    bail!("Unexpected dependency '{pkg_name}' resolved externally by '{resolver}'");
                }
            };

            if source.subst.is_some() {
                bail!("Unexpected 'addr_subst' in source for '{pkg_name}'")
            }

            if source.digest.is_some() {
                bail!("Unexpected 'digest' in source for '{pkg_name}'")
            }

            let pkg = Package {
                kind: source.kind,
                version: source.version,
                resolver,
            };

            match package_table.entry(pkg_name) {
                Entry::Vacant(entry) => {
                    entry.insert(pkg);
                }

                // Seeing the same package twice in the same lock file: Not OK even if all their
                // properties match as a properly created lock file should de-duplicate packages.
                Entry::Occupied(entry) => {
                    bail!(
                        "Conflicting dependencies found:\n{0} = {1}\n{0} = {2}",
                        pkg_name,
                        PackageWithResolverTOML(entry.get()),
                        PackageWithResolverTOML(&pkg),
                    );
                }
            };

            for schema::Dependency {
                name: dep_name,
                subst,
                digest,
            } in dependencies.into_iter().flatten()
            {
                package_graph.add_edge(
                    pkg_name,
                    PM::PackageName::from(dep_name.as_str()),
                    Dependency {
                        mode: DependencyMode::Always,
                        subst: subst.map(parse_substitution).transpose()?,
                        digest: digest.map(Symbol::from),
                        dep_override: false,
                    },
                );
            }

            for schema::Dependency {
                name: dep_name,
                subst,
                digest,
            } in dev_dependencies.into_iter().flatten()
            {
                package_graph.add_edge(
                    pkg_name,
                    PM::PackageName::from(dep_name.as_str()),
                    Dependency {
                        mode: DependencyMode::DevOnly,
                        subst: subst.map(parse_substitution).transpose()?,
                        digest: digest.map(Symbol::from),
                        dep_override: false,
                    },
                );
            }
        }

        let mut graph = DependencyGraph {
            root_path,
            root_package,
            package_graph,
            package_table,
            always_deps: BTreeSet::new(),
            manifest_digest,
            deps_digest,
        };

        graph.check_consistency()?;
        graph.check_acyclic()?;
        graph.discover_always_deps();
        Ok(graph)
    }

    /// Serializes this dependency graph into a lock file and return it.
    ///
    /// This operation fails, writing nothing, if the graph contains a cycle, and can fail with an
    /// undefined output if it cannot be represented in a TOML file.
    pub fn write_to_lock(&self, install_dir: PathBuf) -> Result<LockFile> {
        let lock = LockFile::new(
            install_dir,
            self.manifest_digest.clone(),
            self.deps_digest.clone(),
        )?;
        let mut writer = BufWriter::new(&*lock);

        self.write_dependencies_to_lock(self.root_package, &mut writer)?;

        for (name, pkg) in &self.package_table {
            writeln!(writer, "\n[[move.package]]")?;

            writeln!(writer, "name = {}", str_escape(name.as_str())?)?;
            writeln!(writer, "source = {}", PackageTOML(pkg))?;

            self.write_dependencies_to_lock(*name, &mut writer)?;
        }

        writer.flush()?;
        std::mem::drop(writer);

        Ok(lock)
    }

    /// Helper function to output the dependencies and dev-dependencies of `name` from this
    /// dependency graph, to the lock file under `writer`.
    fn write_dependencies_to_lock<W: Write>(
        &self,
        name: PM::PackageName,
        writer: &mut W,
    ) -> Result<()> {
        let mut deps: Vec<_> = self
            .package_graph
            .edges(name)
            .map(|(_, pkg, dep)| (dep, pkg))
            .collect();

        // Sort by kind ("always" dependencies go first), and by name, to keep the output
        // stable.
        deps.sort_by_key(|(dep, pkg)| (dep.mode, *pkg));
        let mut deps = deps.into_iter().peekable();

        macro_rules! write_deps {
            ($mode: pat, $label: literal) => {
                if let Some((Dependency { mode: $mode, .. }, _)) = deps.peek() {
                    writeln!(writer, "\n{} = [", $label)?;
                    while let Some((dep @ Dependency { mode: $mode, .. }, pkg)) = deps.peek() {
                        writeln!(writer, "  {},", DependencyTOML(*pkg, dep))?;
                        deps.next();
                    }
                    writeln!(writer, "]")?;
                }
            };
        }

        write_deps!(DependencyMode::Always, "dependencies");
        write_deps!(DependencyMode::DevOnly, "dev-dependencies");

        Ok(())
    }

    /// Returns packages in the graph in topological order (a package is ordered before its
    /// dependencies).
    ///
    /// The ordering is agnostic to dependency mode (dev-mode or not) and contains all packagesd
    /// (including packages that are exclusively dev-mode-only).
    ///
    /// Guaranteed to succeed because `DependencyGraph` instances cannot contain cycles.
    pub fn topological_order(&self) -> Vec<PM::PackageName> {
        algo::toposort(&self.package_graph, None)
            .expect("Graph is determined to be acyclic when created")
    }

    /// Returns an iterator over `pkg`'s immediate dependencies in the graph.  If `mode` is
    /// `DependencyMode::Always`, only always dependencies are included, whereas if `mode` is
    /// `DependencyMode::DevOnly`, both always and dev-only dependecies are included.
    pub fn immediate_dependencies(
        &'_ self,
        pkg: PM::PackageName,
        mode: DependencyMode,
    ) -> impl Iterator<Item = (PM::PackageName, &'_ Dependency, &'_ Package)> {
        self.package_graph
            .edges(pkg)
            .filter(move |(_, _, dep)| dep.mode <= mode)
            .map(|(_, dep_name, dep)| (dep_name, dep, &self.package_table[&dep_name]))
    }

    /// Resolves the packages described at dependency `to` of package `from` with manifest at path
    /// `package_path` by running the binary `resolver.  `mode` decides whether the resulting
    /// packages are added to `self` as dependencies of `package_name` or dev-dependencies.
    ///
    /// Sends progress updates to `progress_output`, including stderr from the resolver, and
    /// captures stdout, which is assumed to be a lock file containing the result of package
    /// resolution.
    fn get_external<Progress: Write>(
        mode: DependencyMode,
        from: PM::PackageName,
        to: PM::PackageName,
        resolver: Symbol,
        package_path: &Path,
        progress_output: &mut Progress,
    ) -> Result<DependencyGraph> {
        let mode_label = if mode == DependencyMode::DevOnly {
            "dev-dependencies"
        } else {
            "dependencies"
        };

        let progress_label = format!("RESOLVING {} IN", mode_label.to_uppercase())
            .bold()
            .green();

        writeln!(
            progress_output,
            "{progress_label} {to} {} {from} {} {resolver}",
            "FROM".bold().green(),
            "WITH".bold().green(),
        )?;

        // Call out to the external resolver
        let output = Command::new(resolver.as_str())
            .arg(format!("--resolve-move-{mode_label}"))
            .arg(to.as_str())
            .current_dir(package_path)
            .output()
            .with_context(|| format!("Running resolver: {resolver}"))?;

        // Present the stderr from the resolver, whether the process succeeded or not.
        if !output.stderr.is_empty() {
            let stderr_label = format!("{resolver} stderr:").red();
            writeln!(progress_output, "{stderr_label}")?;
            progress_output.write_all(&output.stderr)?;
        }

        if !output.status.success() {
            let err_msg = format!(
                "'{resolver}' failed to resolve {mode_label} for dependency '{to}' of package \
                 '{from}'"
            );

            if let Some(code) = output.status.code() {
                bail!("{err_msg}. Exited with code: {code}");
            } else {
                bail!("{err_msg}. Terminated by signal");
            }
        }

        let sub_graph = DependencyGraph::read_from_lock(
            package_path.to_path_buf(),
            from,
            &mut output.stdout.as_slice(),
            Some(resolver),
        )
        .with_context(|| {
            format!("Parsing response from '{resolver}' for dependency '{to}' of package '{from}'")
        })?;

        Ok(sub_graph)
    }

    /// Checks that every dependency in the graph, excluding the root package, is present in the
    /// package table.
    fn check_consistency(&self) -> Result<()> {
        for package in self.package_graph.nodes() {
            if package == self.root_package {
                continue;
            }

            if self.package_table.contains_key(&package) {
                continue;
            }

            let dependees: Vec<_> = self
                .package_graph
                .neighbors_directed(package, Direction::Incoming)
                .map(|pkg| String::from(pkg.as_str()))
                .collect();

            bail!(
                "No source found for package {}, depended on by: {}",
                package,
                dependees.join(", "),
            );
        }

        Ok(())
    }

    /// Checks that there isn't a cycle between packages in the dependency graph.  Returns `Ok(())`
    /// if there is not, or an error describing the cycle if there is.
    fn check_acyclic(&self) -> Result<()> {
        let mut cyclic_components = algo::kosaraju_scc(&self.package_graph)
            .into_iter()
            .filter(|scc| scc.len() != 1 || self.package_graph.contains_edge(scc[0], scc[0]));

        let Some(scc) = cyclic_components.next() else {
            return Ok(())
        };

        // Duplicate start of the node at end for display
        // SAFETY: Strongly connected components can't be empty
        let mut cycle: Vec<_> = scc.iter().map(Symbol::as_str).collect();
        cycle.push(cycle[0]);

        bail!("Found cycle between packages: {}", cycle.join(" -> "));
    }

    /// Adds the transitive closure of `DependencyMode::Always` edges reachable from the root package
    /// to the `always_deps` set.  Assumes that if a package is already in the graph's `always_deps`
    /// set, then the sub-graph reachable from it has already been explored.
    fn discover_always_deps(&mut self) {
        let mut frontier = vec![self.root_package];
        while let Some(package) = frontier.pop() {
            let new_frontier = self.always_deps.insert(package);
            if !new_frontier {
                continue;
            }

            frontier.extend(
                self.package_graph
                    .edges(package)
                    .filter(|(_, _, dep)| dep.mode == DependencyMode::Always)
                    .map(|(_, pkg, _)| pkg),
            );
        }
    }
}

impl<'a> fmt::Display for PackageTOML<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Package {
            kind,
            version,
            resolver: _,
        } = self.0;

        f.write_str("{ ")?;

        match kind {
            PM::DependencyKind::Local(local) => {
                write!(f, "local = ")?;
                f.write_str(&path_escape(local)?)?;
            }

            PM::DependencyKind::Git(PM::GitInfo {
                git_url,
                git_rev,
                subdir,
            }) => {
                write!(f, "git = ")?;
                f.write_str(&str_escape(git_url.as_str())?)?;

                write!(f, ", rev = ")?;
                f.write_str(&str_escape(git_rev.as_str())?)?;

                write!(f, ", subdir = ")?;
                f.write_str(&path_escape(subdir)?)?;
            }

            PM::DependencyKind::Custom(PM::CustomDepInfo {
                node_url,
                package_address,
                subdir,
                package_name: _,
            }) => {
                let custom_key = package_hooks::custom_dependency_key().ok_or(fmt::Error)?;

                f.write_str(&custom_key)?;
                write!(f, " = ")?;
                f.write_str(&str_escape(node_url.as_str())?)?;

                write!(f, ", address = ")?;
                f.write_str(&str_escape(package_address.as_str())?)?;

                write!(f, ", subdir = ")?;
                f.write_str(&path_escape(subdir)?)?;
            }
        }

        if let Some((major, minor, bugfix)) = version {
            write!(f, ", version = \"{}.{}.{}\"", major, minor, bugfix)?;
        }

        f.write_str(" }")?;
        Ok(())
    }
}

impl<'a> fmt::Display for PackageWithResolverTOML<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        PackageTOML(self.0).fmt(f)?;

        if let Some(resolver) = self.0.resolver {
            write!(f, " # Resolved by {resolver}")?;
        }

        Ok(())
    }
}

impl<'a> fmt::Display for DependencyTOML<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let DependencyTOML(
            name,
            Dependency {
                mode: _,
                subst,
                digest,
                dep_override: _,
            },
        ) = self;

        f.write_str("{ ")?;

        write!(f, "name = ")?;
        f.write_str(&str_escape(name.as_str())?)?;

        if let Some(digest) = digest {
            write!(f, ", digest = ")?;
            f.write_str(&str_escape(digest.as_str())?)?;
        }

        if let Some(subst) = subst {
            write!(f, ", addr_subst = {}", SubstTOML(subst))?;
        }

        f.write_str(" }")?;
        Ok(())
    }
}

impl<'a> fmt::Display for SubstTOML<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Write an individual key value pair in the substitution.
        fn write_subst(
            f: &mut fmt::Formatter<'_>,
            addr: &PM::NamedAddress,
            subst: &PM::SubstOrRename,
        ) -> fmt::Result {
            f.write_str(&str_escape(addr.as_str())?)?;
            write!(f, " = ")?;

            match subst {
                PM::SubstOrRename::RenameFrom(named) => {
                    f.write_str(&str_escape(named.as_str())?)?;
                }

                PM::SubstOrRename::Assign(account) => {
                    f.write_str(&str_escape(&account.to_canonical_string())?)?;
                }
            }

            Ok(())
        }

        let mut substs = self.0.iter();

        let Some((addr, subst)) = substs.next() else {
            return f.write_str("{}")
        };

        f.write_str("{ ")?;

        write_subst(f, addr, subst)?;
        for (addr, subst) in substs {
            write!(f, ", ")?;
            write_subst(f, addr, subst)?;
        }

        f.write_str(" }")?;

        Ok(())
    }
}

/// Escape a string to output in a TOML file.
fn str_escape(s: &str) -> Result<String, fmt::Error> {
    toml::to_string(s).map_err(|_| fmt::Error)
}

/// Escape a path to output in a TOML file.
fn path_escape(p: &Path) -> Result<String, fmt::Error> {
    str_escape(p.to_str().ok_or(fmt::Error)?)
}

fn format_deps(
    pkg_name: PM::PackageName,
    dependencies: Vec<(&Dependency, PM::PackageName, &Package)>,
) -> String {
    let mut s = format!("\nin {}", pkg_name).to_string();
    if !dependencies.is_empty() {
        for (dep, pkg_name, pkg) in dependencies {
            s.push_str("\n\t");
            s.push_str(
                format!(
                    "dependency {} for package {}",
                    DependencyTOML(pkg_name, dep),
                    PackageWithResolverTOML(pkg)
                )
                .as_str(),
            );
        }
    } else {
        s.push_str("\n\tno dependencies");
    }
    s
}

/// Checks if dependencies of a given package in two different dependency graph maps are the same,
/// checking both the dependency in the graph and the destination package (both can be different).
fn deps_equal<'a>(
    pkg_name: Symbol,
    combined_graph: &'a DependencyGraph,
    sub_graph: &'a DependencyGraph,
) -> (
    Vec<(&'a Dependency, PM::PackageName, &'a Package)>,
    Vec<(&'a Dependency, PM::PackageName, &'a Package)>,
) {
    let combined_edges = BTreeSet::from_iter(
        combined_graph
            .package_graph
            .edges(pkg_name)
            .map(|(_, pkg, dep)| (dep, pkg, combined_graph.package_table.get(&pkg).unwrap())),
    );
    let sub_pkg_edges = BTreeSet::from_iter(
        sub_graph
            .package_graph
            .edges(pkg_name)
            .map(|(_, pkg, dep)| (dep, pkg, sub_graph.package_table.get(&pkg).unwrap())),
    );

    let (combined_pkgs, sub_pkgs): (Vec<_>, Vec<_>) = combined_edges
        .symmetric_difference(&sub_pkg_edges)
        .partition(|dep| combined_edges.contains(dep));
    (combined_pkgs, sub_pkgs)
}
