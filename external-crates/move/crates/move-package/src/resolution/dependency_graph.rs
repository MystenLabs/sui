// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use colored::Colorize;
use move_symbol_pool::Symbol;
use petgraph::{algo, prelude::DiGraphMap, Direction};

use std::io::BufRead;
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, VecDeque},
    fmt,
    fs::File,
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    lock_file::{schema, LockFile},
    package_hooks::{custom_resolve_pkg_id, resolve_version, PackageIdentifier},
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
/// the criteria above cannot be met (e.g. if the graph contains a cycle, or information about a
/// package's source is not available). The same package can be fetched from multiple sources as
/// long as both fetches produce a matching output.
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
    /// Root package identifier as resolved by the package id resolution hook.
    pub root_package_id: PackageIdentifier,
    /// Root package name as defined in its manifest (can be different from the resolved identifier).
    pub root_package_name: PM::PackageName,

    /// Transitive dependency graph, with dependency edges `P -> Q` labelled according to whether Q
    /// is always a dependency of P or only in dev-mode.
    pub package_graph: DiGraphMap<PackageIdentifier, Dependency>,

    /// The dependency that each package (keyed by id) originates from. The root package is the
    /// only node in `package_graph` that does not have an entry in `package_table`.
    pub package_table: BTreeMap<PackageIdentifier, Package>,

    /// Packages that are transitive dependencies regardless of mode (the transitive closure of
    /// `DependencyMode::Always` edges in `package_graph`).
    pub always_deps: BTreeSet<PackageIdentifier>,

    /// A hash of the manifest file content this lock file was generated from.
    pub manifest_digest: String,
    /// A hash of all the dependencies (their lock file content) this lock file depends on.
    pub deps_digest: String,
}

/// A helper to store additional information about a dependency graph
#[derive(Debug, Clone)]
pub struct DependencyGraphInfo {
    /// The dependency graph itself.
    pub g: DependencyGraph,
    /// A mode of the dependency that the dependency graph represents.
    pub mode: DependencyMode,
    /// Is the dependency this graph represents an override?
    pub is_override: bool,
    /// Is the dependency graph externally resolved?
    pub is_external: bool,
    /// Resolved version of the root package (based on version resolution hook)
    pub version: Option<Symbol>,
}

impl DependencyGraphInfo {
    pub fn new(
        g: DependencyGraph,
        mode: DependencyMode,
        is_override: bool,
        is_external: bool,
        version: Option<Symbol>,
    ) -> Self {
        Self {
            g,
            mode,
            is_override,
            is_external,
            version,
        }
    }
}

#[derive(Debug, Clone, Eq, Ord, PartialOrd)]
pub struct Package {
    pub kind: PM::DependencyKind,
    pub version: Option<Symbol>,
    /// Optional field set if the package was externally resolved.
    resolver: Option<Symbol>,
}

impl PartialEq for Package {
    fn eq(&self, other: &Self) -> bool {
        // When the resolve_version hook is defined (both packages have a version),
        // we compare the packages based on their version rather than their location (`PM::DependencyKind`)
        // as defined in their parent manifest. When the hook is not defined (or returns None) for both packages,
        // we compare the packages based on their location. If the version resolves for one package but is None for
        // the other, we consider the packages to be different.
        // Comparison omits the type of resolver (as it would actually lead to incorrect result when
        // comparing packages during insertion of externally resolved ones - an internally resolved
        // existing package in the graph would not be recognized as a potential different version of
        // the externally resolved one).
        match (&self.version, &other.version) {
            (Some(this), Some(other)) => this == other,
            (None, None) => self.kind == other.kind,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialOrd, Ord)]
pub struct Dependency {
    pub mode: DependencyMode,
    pub subst: Option<PM::Substitution>,
    pub digest: Option<PM::PackageDigest>,
    pub dep_override: PM::DepOverride,
    /// Original dependency name as defined in parent manifest since it can be different from the
    /// resolved identifier. Used for printing user-friendly error messages.
    pub dep_name: PM::PackageName,
}

impl PartialEq for Dependency {
    // We store the original dependency name in the graph for printing user-friendly error messages,
    // but we don't want to consider it when comparing dependencies for equality.
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.subst == other.subst
            && self.digest == other.digest
            && self.dep_override == other.dep_override
    }
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
struct DependencyTOML<'a>(PackageIdentifier, &'a Dependency);
struct SubstTOML<'a>(&'a PM::Substitution);

/// A builder for `DependencyGraph`
pub struct DependencyGraphBuilder<Progress: Write> {
    /// Used to avoid re-fetching dependencies that are already present locally
    pub dependency_cache: DependencyCache,
    /// Logger
    pub progress_output: Progress,
    /// A chain of visited dependencies used for cycle detection
    visited_dependencies: VecDeque<(PackageIdentifier, PM::InternalDependency)>,
    /// Installation directory for compiled artifacts (from BuildConfig).
    install_dir: PathBuf,
}

impl<Progress: Write> DependencyGraphBuilder<Progress> {
    pub fn new(
        skip_fetch_latest_git_deps: bool,
        progress_output: Progress,
        install_dir: PathBuf,
    ) -> Self {
        DependencyGraphBuilder {
            dependency_cache: DependencyCache::new(skip_fetch_latest_git_deps),
            progress_output,
            visited_dependencies: VecDeque::new(),
            install_dir,
        }
    }

    /// Get a new graph by either reading it from Move.lock file (if this file is up-to-date, in
    /// which case also return false) or by computing a new graph based on the content of the
    /// Move.toml (manifest) file (in which case also return true).
    pub fn get_graph(
        &mut self,
        parent: &PM::DependencyKind,
        root_path: PathBuf,
        manifest_string: String,
        lock_string_opt: Option<String>,
    ) -> Result<(DependencyGraph, bool)> {
        let toml_manifest = parse_move_manifest_string(manifest_string.clone())?;
        let root_manifest = parse_source_manifest(toml_manifest)?;

        // compute digests eagerly as even if we can't reuse existing lock file, they need to become
        // part of the newly computed dependency graph
        let new_manifest_digest = digest_str(manifest_string.into_bytes().as_slice());
        let lock_path = root_path.join(SourcePackageLayout::Lock.path());
        let lock_file = File::open(lock_path);
        let digest_and_lock_contents = lock_file
            .map(|mut lock_file| match schema::Header::read(&mut lock_file) {
                Ok(header) if header.version < schema::VERSION => None, // outdated lock file - regenerate
                Ok(header) => Some((header.manifest_digest, header.deps_digest, lock_string_opt)),
                Err(_) => None, // malformed header - regenerate lock file
            })
            .unwrap_or(None);

        // collect sub-graphs for "regular" and "dev" dependencies
        let root_pkg_id = custom_resolve_pkg_id(&root_manifest).with_context(|| {
            format!(
                "Resolving package name for '{}'",
                root_manifest.package.name
            )
        })?;
        let root_pkg_name = root_manifest.package.name;
        let (mut dep_graphs, resolved_id_deps, mut dep_names, mut overrides) = self
            .collect_graphs(
                parent,
                root_pkg_id,
                root_pkg_name,
                root_path.clone(),
                DependencyMode::Always,
                root_manifest.dependencies.clone(),
            )?;
        let dep_lock_files = dep_graphs
            .values()
            // write_to_lock should create a fresh lockfile for computing the dependency digest, hence the `None` arg below
            .map(|graph_info| graph_info.g.write_to_lock(self.install_dir.clone(), None))
            .collect::<Result<Vec<LockFile>>>()?;
        let (dev_dep_graphs, dev_resolved_id_deps, dev_dep_names, dev_overrides) = self
            .collect_graphs(
                parent,
                root_pkg_id,
                root_pkg_name,
                root_path.clone(),
                DependencyMode::DevOnly,
                root_manifest.dev_dependencies.clone(),
            )?;

        let dev_dep_lock_files = dev_dep_graphs
            .values()
            // write_to_lock should create a fresh lockfile for computing the dependency digest, hence the `None` arg below
            .map(|graph_info| graph_info.g.write_to_lock(self.install_dir.clone(), None))
            .collect::<Result<Vec<LockFile>>>()?;
        let new_deps_digest = self.dependency_digest(dep_lock_files, dev_dep_lock_files)?;
        let (manifest_digest, deps_digest) = match digest_and_lock_contents {
            Some((old_manifest_digest, old_deps_digest, Some(lock_string)))
                if old_manifest_digest == new_manifest_digest
                    && old_deps_digest == new_deps_digest =>
            {
                return Ok((
                    DependencyGraph::read_from_lock(
                        root_path,
                        root_pkg_id,
                        root_pkg_name,
                        &mut lock_string.as_bytes(), // safe since old_deps_digest exists
                        None,
                    )?,
                    false,
                ));
            }
            _ => (new_manifest_digest, new_deps_digest),
        };

        dep_graphs.extend(dev_dep_graphs);
        dep_names.extend(dev_dep_names);

        let mut combined_graph = DependencyGraph {
            root_path,
            root_package_id: root_pkg_id,
            root_package_name: root_pkg_name,
            package_graph: DiGraphMap::new(),
            package_table: BTreeMap::new(),
            always_deps: BTreeSet::new(),
            manifest_digest,
            deps_digest,
        };
        // ensure there's always a root node, even if it has no edges
        combined_graph
            .package_graph
            .add_node(combined_graph.root_package_id);

        for (
            dep_id,
            DependencyGraphInfo {
                g,
                mode,
                is_override,
                is_external: _,
                version: _,
            },
        ) in dep_graphs.iter_mut()
        {
            g.prune_subgraph(
                root_pkg_name,
                *dep_id,
                *dep_names.get(dep_id).unwrap(),
                *is_override,
                *mode,
                &overrides,
                &dev_overrides,
            )?;
        }

        let mut all_deps = resolved_id_deps;
        all_deps.extend(dev_resolved_id_deps);

        // we can mash overrides together as the sets cannot overlap (it's asserted during pruning)
        overrides.extend(dev_overrides);

        combined_graph.merge(
            dep_graphs,
            parent,
            &all_deps,
            &overrides,
            &dep_names,
            root_pkg_name,
        )?;

        combined_graph.check_acyclic()?;
        combined_graph.discover_always_deps();

        Ok((combined_graph, true))
    }

    /// Given all dependencies from the parent manifest file, collects all the sub-graphs
    /// representing these dependencies (both internally and externally resolved).
    fn collect_graphs(
        &mut self,
        parent: &PM::DependencyKind,
        parent_pkg_id: PackageIdentifier,
        parent_pkg_name: PM::PackageName,
        root_path: PathBuf,
        mode: DependencyMode,
        dependencies: PM::Dependencies,
    ) -> Result<(
        BTreeMap<PackageIdentifier, DependencyGraphInfo>,
        BTreeMap<PackageIdentifier, PM::Dependency>,
        BTreeMap<PackageIdentifier, PM::PackageName>,
        BTreeMap<PackageIdentifier, Package>,
    )> {
        let mut dep_graphs = BTreeMap::new();
        let mut resolved_id_deps = BTreeMap::new();
        let mut dep_orig_names = BTreeMap::new();
        let mut overrides = BTreeMap::new();

        for (dep_pkg_name, dep) in dependencies {
            let new_deps = self
                .new_for_dep(
                    parent,
                    &dep,
                    mode,
                    parent_pkg_id,
                    parent_pkg_name,
                    dep_pkg_name,
                    root_path.clone(),
                )
                .with_context(|| {
                    format!(
                        "Failed to resolve dependencies for package '{}'",
                        parent_pkg_name
                    )
                })?;

            for (pkg_graph, is_override, is_external, resolved_pkg_id, resolved_version) in new_deps
            {
                dep_graphs.insert(
                    resolved_pkg_id,
                    DependencyGraphInfo::new(
                        pkg_graph,
                        mode,
                        is_override,
                        is_external,
                        resolved_version,
                    ),
                );
                resolved_id_deps.insert(resolved_pkg_id, dep.clone());
                dep_orig_names.insert(resolved_pkg_id, dep_pkg_name);

                if is_override {
                    let kind = match dep {
                        PM::Dependency::Internal(ref d) => d.kind.clone(),
                        PM::Dependency::External(_) => {
                            // external dependencies cannot be overrides
                            panic!("Unexpected external dependency override")
                        }
                    };
                    let mut dep_pkg = Package {
                        kind,
                        resolver: None,
                        version: resolved_version,
                    };
                    dep_pkg.kind.reroot(parent)?;
                    overrides.insert(resolved_pkg_id, dep_pkg);
                }
            }
        }
        Ok((dep_graphs, resolved_id_deps, dep_orig_names, overrides))
    }

    /// Given a dependency in the parent's manifest file, creates a sub-graph for this dependency.
    fn new_for_dep(
        &mut self,
        parent: &PM::DependencyKind,
        dep: &PM::Dependency,
        mode: DependencyMode,
        parent_pkg_id: PackageIdentifier,
        parent_pkg_name: PM::PackageName,
        dep_pkg_name: PM::PackageName,
        dep_pkg_path: PathBuf,
    ) -> Result<Vec<(DependencyGraph, bool, bool, Symbol, Option<Symbol>)>> {
        match dep {
            PM::Dependency::Internal(d) => {
                self.dependency_cache
                    .download_and_update_if_remote(dep_pkg_name, &d.kind, &mut self.progress_output)
                    .with_context(|| format!("Fetching '{}'", dep_pkg_name))?;
                let pkg_path = dep_pkg_path.join(local_path(&d.kind));
                let manifest_string =
                    std::fs::read_to_string(pkg_path.join(SourcePackageLayout::Manifest.path()))
                        .with_context(|| format!("Parsing manifest for '{}'", dep_pkg_name))?;
                let lock_string =
                    std::fs::read_to_string(pkg_path.join(SourcePackageLayout::Lock.path())).ok();

                // resolve name and version
                let manifest =
                    parse_source_manifest(parse_move_manifest_string(manifest_string.clone())?)?;
                let resolved_pkg_id = custom_resolve_pkg_id(&manifest)
                    .with_context(|| format!("Resolving package name for '{}'", dep_pkg_name))?;
                let resolved_version = resolve_version(&manifest)
                    .with_context(|| format!("Resolving version for '{}'", dep_pkg_name))?;
                check_for_dep_cycles(
                    d.clone(),
                    resolved_pkg_id,
                    dep_pkg_name,
                    &mut self.visited_dependencies,
                )?;

                // save dependency for cycle detection
                self.visited_dependencies
                    .push_front((resolved_pkg_id, d.clone()));
                let (mut pkg_graph, modified) =
                    self.get_graph(&d.kind, pkg_path, manifest_string, lock_string)?;
                self.visited_dependencies.pop_front();
                // reroot all packages to normalize local paths across all graphs
                for (_, p) in pkg_graph.package_table.iter_mut() {
                    if modified {
                        // new sub-graph has been constructed whose paths are already re-rooted with
                        // respect to its immediate parent
                        p.kind.reroot(parent)?;
                    } else {
                        // a graph has been read from the lock file and its immediate parent is
                        // specified in the dependency pointing to this sub-graph which must be used
                        // for re-rooting
                        p.kind.reroot(&d.kind)?;
                    }
                }
                Ok(vec![(
                    pkg_graph,
                    d.dep_override,
                    false,
                    resolved_pkg_id,
                    resolved_version,
                )])
            }
            PM::Dependency::External(resolver) => {
                let external_deps = DependencyGraph::get_external(
                    mode,
                    parent_pkg_id,
                    parent_pkg_name,
                    dep_pkg_name,
                    *resolver,
                    &dep_pkg_path,
                    &mut self.progress_output,
                )?;

                Ok(external_deps
                    .into_iter()
                    .map(|(pkg_graph, _, resolved_pkg_id, resolved_pkg_version)| {
                        (
                            pkg_graph,
                            false,
                            true,
                            resolved_pkg_id,
                            resolved_pkg_version,
                        )
                    })
                    .collect())
            }
        }
    }

    /// Computes dependency hashes.
    fn dependency_hashes(&mut self, lock_files: Vec<LockFile>) -> Result<Vec<String>> {
        let mut hashed_lock_files = Vec::new();
        for mut lock_file in lock_files {
            let mut lock_string: String = "".to_string();
            lock_file.read_to_string(&mut lock_string)?;
            hashed_lock_files.push(digest_str(lock_string.as_bytes()));
        }
        Ok(hashed_lock_files)
    }

    /// Computes a digest of all dependencies in a manifest file (or digest of empty list if there
    /// are no dependencies).
    fn dependency_digest(
        &mut self,
        dep_lock_files: Vec<LockFile>,
        dev_dep_lock_files: Vec<LockFile>,
    ) -> Result<String> {
        let mut dep_hashes = self.dependency_hashes(dep_lock_files)?;

        let dev_dep_hashes = self.dependency_hashes(dev_dep_lock_files)?;

        if dep_hashes.is_empty() && dev_dep_hashes.is_empty() {
            Ok(digest_str(&[]))
        } else {
            dep_hashes.extend(dev_dep_hashes);
            Ok(hashed_files_digest(dep_hashes))
        }
    }
}

impl DependencyGraph {
    /// Main driver from sub-graph pruning based on information about overrides.
    fn prune_subgraph(
        &mut self,
        root_package_name: PM::PackageName,
        dep_id: PackageIdentifier,
        dep_name: PM::PackageName,
        is_override: bool,
        mode: DependencyMode,
        overrides: &BTreeMap<PackageIdentifier, Package>,
        dev_overrides: &BTreeMap<PackageIdentifier, Package>,
    ) -> Result<()> {
        if is_override {
            // when pruning an overridden dependency, we must not prune the package actually
            // specified by this dependency so we remove it from the set of overrides (see the
            // diamond_problem_dual_override test for an example of what should be pruned and what
            // should not)
            let mut o = overrides.clone();
            let mut dev_o = dev_overrides.clone();
            DependencyGraph::remove_dep_override(
                root_package_name,
                dep_id,
                dep_name,
                &mut o,
                &mut dev_o,
                mode == DependencyMode::DevOnly,
            )?;
            self.prune_overriden_pkgs(root_package_name, mode, &o, &dev_o)?;
        } else {
            self.prune_overriden_pkgs(root_package_name, mode, overrides, dev_overrides)?;
        }
        Ok(())
    }

    /// Finds packages in a sub-graph that should be pruned as a result of applying an override from
    /// the outer graph. A package should be pruned if it's dominated by an overridden package.
    fn find_pruned_pkgs(
        &self,
        pruned_pkgs: &mut BTreeSet<PM::PackageName>,
        reachable_pkgs: &mut BTreeSet<PM::PackageName>,
        root_pkg_name: PM::PackageName,
        from_pkg_id: PackageIdentifier,
        from_pkg_name: PM::PackageName,
        mode: DependencyMode,
        overrides: &BTreeMap<PM::PackageName, Package>,
        dev_overrides: &BTreeMap<PM::PackageName, Package>,
        overridden_path: bool,
    ) -> Result<()> {
        if overridden_path {
            // we are on a path originating at the overridden package
            if !reachable_pkgs.contains(&from_pkg_id) {
                // not (yet) reached via regular (non-overridden) path
                pruned_pkgs.insert(from_pkg_id);
            }
        }
        let mut override_found = overridden_path;

        if !override_found {
            override_found = self
                .get_dep_override(
                    root_pkg_name,
                    from_pkg_id,
                    from_pkg_name,
                    overrides,
                    dev_overrides,
                    mode == DependencyMode::DevOnly,
                )?
                .is_some();

            if override_found {
                // we also prune overridden package - we can do this safely as the outgoing edges
                // from other package graph nodes to the overridden package will be preserved (see
                // the nested_pruned_override test for additional explanation how nodes are removed
                // from the package graph)
                pruned_pkgs.insert(from_pkg_id);
            } else {
                // we are on a regular path, not involving an override (overridden_path == false)
                // and we did not find an override
                reachable_pkgs.insert(from_pkg_id);
            }
        }

        for to_pkg_id in self
            .package_graph
            .neighbors_directed(from_pkg_id, Direction::Outgoing)
        {
            let dep = self
                .package_graph
                .edge_weight(from_pkg_id, to_pkg_id)
                .unwrap();
            let to_pkg_name = dep.dep_name;
            self.find_pruned_pkgs(
                pruned_pkgs,
                reachable_pkgs,
                root_pkg_name,
                to_pkg_id,
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
        mode: DependencyMode,
        overrides: &BTreeMap<PackageIdentifier, Package>,
        dev_overrides: &BTreeMap<PackageIdentifier, Package>,
    ) -> Result<()> {
        let from_pkg_id = self.root_package_id;
        let from_pkg_name = self.root_package_name;
        let mut pruned_pkgs = BTreeSet::new();
        let mut reachable_pkgs = BTreeSet::new();
        self.find_pruned_pkgs(
            &mut pruned_pkgs,
            &mut reachable_pkgs,
            root_pkg_name,
            from_pkg_id,
            from_pkg_name,
            mode,
            overrides,
            dev_overrides,
            false,
        )?;

        // if there was a package candidate for pruning, it should be removed from the list if it
        // can be reached via a regular path
        pruned_pkgs.retain(|p| !reachable_pkgs.contains(p));
        for pkg in pruned_pkgs {
            self.package_graph.remove_node(pkg);
            self.package_table.remove(&pkg);
        }

        Ok(())
    }

    /// Given all sub-graphs representing dependencies of the parent manifest file, combines all
    /// subgraphs to form the parent dependency graph.
    pub fn merge(
        &mut self,
        mut dep_graphs: BTreeMap<PackageIdentifier, DependencyGraphInfo>,
        parent: &PM::DependencyKind,
        dependencies: &BTreeMap<PackageIdentifier, PM::Dependency>,
        overrides: &BTreeMap<PackageIdentifier, Package>,
        dep_names: &BTreeMap<PackageIdentifier, PM::PackageName>,
        root_pkg_name: PM::PackageName,
    ) -> Result<()> {
        if !self.always_deps.is_empty() {
            bail!("Merging dependencies into a graph after calculating its 'always' dependencies");
        }

        // insert direct dependency edges and (if necessary) packages for the remaining graph nodes
        // (not present in package table)
        for (dep_id, graph_info) in &dep_graphs {
            let dep_name = dep_names.get(dep_id).unwrap_or(dep_id);

            let Some(dep) = dependencies.get(dep_id) else {
                bail!(
                    "Can't merge dependencies for '{}' because nothing depends on it",
                    dep_name
                );
            };

            let internally_resolved = self.insert_direct_dep(
                dep,
                *dep_id,
                *dep_name,
                graph_info.version,
                &graph_info.g,
                graph_info.mode,
                parent,
            )?;

            if internally_resolved {
                // insert edges from the directly dependent package to its neighbors for
                // internally resolved sub-graphs - due to how external graphs are constructed,
                // edges between directly dependent packages and their neighbors are already in
                // the sub-graph and would have been inserted in the first loop in this function
                for (_, to_pkg_id, sub_dep) in graph_info.g.package_graph.edges(*dep_id) {
                    self.package_graph
                        .add_edge(*dep_id, to_pkg_id, sub_dep.clone());
                }
            }
        }

        // collect all package names in all graphs in package table
        let mut all_packages: BTreeSet<PM::PackageName> =
            BTreeSet::from_iter(self.package_table.keys().cloned());
        for graph_info in dep_graphs.values() {
            all_packages.extend(graph_info.g.package_table.keys());
        }

        dep_graphs.insert(
            self.root_package_id,
            DependencyGraphInfo::new(self.clone(), DependencyMode::Always, false, false, None),
        );

        // analyze all packages to determine if any of these packages represent a conflicting
        // dependency; insert the packages and their respective edges into the combined graph along
        // the way
        for pkg_id in all_packages {
            let mut existing_pkg_info: Option<(
                PackageIdentifier,
                &DependencyGraph,
                &Package,
                bool,
            )> = None;
            for (dep_id, graph_info) in dep_graphs.iter() {
                let Some(pkg) = graph_info.g.package_table.get(&pkg_id) else {
                    continue;
                };
                // graph g has a package with name pkg_name
                let Some((
                    existing_immediate_dep_id,
                    existing_graph,
                    existing_pkg,
                    existing_is_external,
                )) = existing_pkg_info
                else {
                    // first time this package was encountered
                    existing_pkg_info = Some((*dep_id, &graph_info.g, pkg, graph_info.is_external));
                    continue;
                };

                let existing_immediate_dep_name = dep_names
                    .get(&existing_immediate_dep_id)
                    .unwrap_or(&existing_immediate_dep_id);
                let immediate_dep_name = dep_names.get(dep_id).unwrap_or(dep_id);

                // it's the subsequent time package with pkg_name has been encountered
                if pkg != existing_pkg {
                    let existing_conflict_dep_orig_name =
                        get_original_dep_name(existing_graph, pkg_id).to_string();
                    let conflict_dep_orig_name =
                        get_original_dep_name(&graph_info.g, pkg_id).to_string();

                    bail!(
                        "When resolving dependencies for package {root_pkg_name}, conflicting versions \
                         of package {conflict_dep_orig_name} found:\
                         \nAt {2}\n\t{existing_conflict_dep_orig_name} = {0}\
                         \nAt {3}\n\t{conflict_dep_orig_name} = {1}",
                        PackageWithResolverTOML(existing_pkg),
                        PackageWithResolverTOML(pkg),
                        dep_path_from_root(
                            self.root_package_id,
                            root_pkg_name,
                            existing_graph,
                            *existing_immediate_dep_name,
                            pkg_id,
                            existing_is_external
                        )?,
                        dep_path_from_root(
                            self.root_package_id,
                            root_pkg_name,
                            &graph_info.g,
                            *immediate_dep_name,
                            pkg_id,
                            graph_info.is_external
                        )?
                    );
                }

                // both packages are the same but we need to check if their dependencies
                // are the same as well
                match deps_equal(
                    pkg_id,
                    existing_graph,
                    self.pkg_table_for_deps_compare(
                        pkg_id,
                        existing_graph,
                        &dep_graphs,
                        existing_is_external,
                    ),
                    &graph_info.g,
                    self.pkg_table_for_deps_compare(
                        pkg_id,
                        &graph_info.g,
                        &dep_graphs,
                        graph_info.is_external,
                    ),
                    overrides,
                ) {
                    Ok(_) => continue,
                    Err((existing_pkg_deps, pkg_deps)) => {
                        bail!(
                            "When resolving dependencies for package {root_pkg_name}, \
                             conflicting dependencies found:{}{}",
                            format_deps(
                                dep_path_from_root(
                                    self.root_package_id,
                                    root_pkg_name,
                                    existing_graph,
                                    *existing_immediate_dep_name,
                                    pkg_id,
                                    existing_is_external
                                )?,
                                existing_pkg_deps
                            ),
                            format_deps(
                                dep_path_from_root(
                                    self.root_package_id,
                                    root_pkg_name,
                                    &graph_info.g,
                                    *immediate_dep_name,
                                    pkg_id,
                                    graph_info.is_external,
                                )?,
                                pkg_deps,
                            ),
                        )
                    }
                }
            }
            if let Some((_, g, existing_pkg, _)) = existing_pkg_info {
                // update combined graph with the new package and its dependencies
                self.package_table.insert(pkg_id, existing_pkg.clone());
                for (_, to_pkg_name, sub_dep) in g.package_graph.edges(pkg_id) {
                    self.package_graph
                        .add_edge(pkg_id, to_pkg_name, sub_dep.clone());
                }
            }
        }

        Ok(())
    }

    /// The merge algorithm relies on the combined graph to be pre-populated with direct
    /// dependencies for internally resolved packages (in terms of both entries in the package table
    /// and nodes/edges in the package graph). This means that if a conflict is detected between
    /// pre-populated combined graph and one of the sub-graphs, the pre-populated graph does not
    /// (yet) contains target packages for edges outgoing from direct dependencies, and these are
    /// needed to verify that direct dependencies are the same. Fortunately, these packages are
    /// available in a package tables of a respective (dependent) subgraph. This function return the
    /// right package table, depending on whether conflict was detected between pre-populated
    /// combined graph and another sub-graph or between two separate sub-graphs. If we tried to use
    /// combined graphs's package table "as is" we would get an error in all cases similar to the one
    /// in the direct_and_indirect_dep test where A is a direct dependency of Root (as C would be
    /// missing from the combined graph's table):
    ///
    /// ```text
    ///                 +----+
    ///           +---->| B  |----+
    ///           |     +----+    |
    ///           |               |
    /// +----+    |               +-->+----+     +----+
    /// |Root|----+------------------>| A  |---->| C  |
    /// +----+                        +----+     +----+
    /// ```
    ///
    fn pkg_table_for_deps_compare<'a>(
        &self,
        dep_name: Symbol,
        g: &'a DependencyGraph,
        dep_graphs: &'a BTreeMap<PackageIdentifier, DependencyGraphInfo>,
        external: bool,
    ) -> &'a BTreeMap<PM::PackageName, Package> {
        if !external && g.root_package_id == self.root_package_id {
            // unwrap is safe since dep_graphs are actually built using information about
            // dependencies (including their name, represented here by dep_name) from the root
            // package
            let g_with_nodes = &dep_graphs.get(&dep_name).unwrap().g;
            &g_with_nodes.package_table
        } else {
            &g.package_table
        }
    }

    /// Inserts a single direct dependency with given (package) name representing a sub-graph into
    /// the combined graph. Returns true if the dependency was internally resolved and false if it
    /// was externally resolved.
    fn insert_direct_dep(
        &mut self,
        dep: &PM::Dependency,
        dep_pkg_id: PackageIdentifier,
        dep_name: PM::PackageName,
        dep_version: Option<Symbol>,
        sub_graph: &DependencyGraph,
        mode: DependencyMode,
        parent: &PM::DependencyKind,
    ) -> Result<bool> {
        match dep {
            PM::Dependency::Internal(PM::InternalDependency {
                kind,
                subst,
                digest,
                dep_override,
            }) => {
                if let Entry::Vacant(entry) = self.package_table.entry(dep_pkg_id) {
                    let mut pkg = Package {
                        kind: kind.clone(),
                        resolver: None,
                        version: dep_version,
                    };
                    pkg.kind.reroot(parent)?;
                    entry.insert(pkg);
                }
                self.package_graph.add_edge(
                    self.root_package_id,
                    dep_pkg_id,
                    Dependency {
                        mode,
                        subst: subst.clone(),
                        digest: *digest,
                        dep_override: *dep_override,
                        dep_name,
                    },
                );
                Ok(true)
            }
            PM::Dependency::External(_) => {
                // External dependencies exist in the subgraph of the root package and are added to
                // the package_graph as such.
                let d = sub_graph
                    .package_graph
                    .edge_weight(self.root_package_id, dep_pkg_id)
                    .unwrap();
                self.package_graph
                    .add_edge(self.root_package_id, dep_pkg_id, d.clone());
                Ok(false)
            }
        }
    }

    /// Helper function to get overrides for "regular" dependencies (`dev_only` is false) or "dev"
    /// dependencies (`dev_only` is true).
    fn get_dep_override<'a>(
        &self,
        root_pkg_name: PM::PackageName,
        pkg_id: PackageIdentifier,
        pkg_name: PM::PackageName,
        overrides: &'a BTreeMap<PackageIdentifier, Package>,
        dev_overrides: &'a BTreeMap<PackageIdentifier, Package>,
        dev_only: bool,
    ) -> Result<Option<&'a Package>> {
        // for "regular" dependencies override can come only from "regular" dependencies section,
        // but for "dev" dependencies override can come from "regular" or "dev" dependencies section
        if let Some(pkg) = overrides.get(&pkg_id) {
            // "regular" dependencies section case
            if let Some(dev_pkg) = dev_overrides.get(&pkg_id) {
                bail!(
                    "Conflicting \"regular\" and \"dev\" overrides found in {0}:\n{1} = {2}\n{1} = {3}",
                    root_pkg_name,
                    pkg_name,
                    PackageWithResolverTOML(pkg),
                    PackageWithResolverTOML(dev_pkg),
                );
            }
            return Ok(Some(pkg));
        } else if let Some(dev_pkg) = dev_overrides.get(&pkg_id) {
            // "dev" dependencies section case
            return Ok(dev_only.then_some(dev_pkg));
        }
        Ok(None)
    }

    /// Helper function to remove an override for a package with a given name for "regular"
    /// dependencies (`dev_only` is false) or "dev" dependencies (`dev_only` is true).
    fn remove_dep_override(
        root_pkg_name: PM::PackageName,
        pkg_id: PackageIdentifier,
        pkg_name: PM::PackageName,
        overrides: &mut BTreeMap<Symbol, Package>,
        dev_overrides: &mut BTreeMap<Symbol, Package>,
        dev_only: bool,
    ) -> Result<()> {
        // for "regular" dependencies override can come only from "regular" dependencies section,
        // but for "dev" dependencies override can come from "regular" or "dev" dependencies section
        if let Some(pkg) = overrides.remove(&pkg_id) {
            // "regular" dependencies section case
            if let Some(dev_pkg) = dev_overrides.get(&pkg_id) {
                bail!(
                    "Conflicting \"regular\" and \"dev\" overrides found in {0}:\n{1} = {2}\n{1} = {3}",
                    root_pkg_name,
                    pkg_name,
                    PackageWithResolverTOML(&pkg),
                    PackageWithResolverTOML(dev_pkg),
                );
            }
        } else if dev_only {
            // "dev" dependencies section case
            dev_overrides.remove(&pkg_id);
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
        root_package_id: PackageIdentifier,
        root_package_name: PM::PackageName,
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
        package_graph.add_node(root_package_id);

        for schema::Dependency {
            id: dep_id,
            name: dep_name,
            subst,
            digest,
        } in packages.root_dependencies.into_iter().flatten()
        {
            package_graph.add_edge(
                root_package_id,
                PackageIdentifier::from(dep_id.as_str()),
                Dependency {
                    mode: DependencyMode::Always,
                    subst: subst.map(parse_substitution).transpose()?,
                    digest: digest.map(Symbol::from),
                    dep_override: false,
                    dep_name: PM::PackageName::from(dep_name),
                },
            );
        }

        for schema::Dependency {
            id: dep_id,
            name: dep_name,
            subst,
            digest,
        } in packages.root_dev_dependencies.into_iter().flatten()
        {
            package_graph.add_edge(
                root_package_id,
                PackageIdentifier::from(dep_id.as_str()),
                Dependency {
                    mode: DependencyMode::DevOnly,
                    subst: subst.map(parse_substitution).transpose()?,
                    digest: digest.map(Symbol::from),
                    dep_override: false,
                    dep_name: PM::PackageName::from(dep_name),
                },
            );
        }

        // Fill in the remaining dependencies, and the package source information from the lock
        // file.
        for schema::Package {
            id: pkg_id,
            source,
            version,
            dependencies,
            dev_dependencies,
        } in packages.packages.into_iter().flatten()
        {
            let pkg_id = PackageIdentifier::from(pkg_id.as_str());
            let source = parse_dependency(source)
                .with_context(|| format!("Deserializing dependency '{pkg_id}'"))?;

            let source = match source {
                PM::Dependency::Internal(source) => source,
                PM::Dependency::External(resolver) => {
                    bail!("Unexpected dependency '{pkg_id}' resolved externally by '{resolver}'");
                }
            };

            if source.subst.is_some() {
                bail!("Unexpected 'addr_subst' in source for '{pkg_id}'")
            }

            if source.digest.is_some() {
                bail!("Unexpected 'digest' in source for '{pkg_id}'")
            }

            let pkg = Package {
                kind: source.kind,
                resolver,
                version: version.map(Symbol::from),
            };

            match package_table.entry(pkg_id) {
                Entry::Vacant(entry) => {
                    entry.insert(pkg);
                }

                // Seeing the same package twice in the same lock file: Not OK even if all their
                // properties match as a properly created lock file should de-duplicate packages.
                Entry::Occupied(entry) => {
                    bail!(
                        "Conflicting dependencies found:\n{0} = {1}\n{0} = {2}",
                        pkg_id,
                        PackageWithResolverTOML(entry.get()),
                        PackageWithResolverTOML(&pkg),
                    );
                }
            };

            for schema::Dependency {
                id: dep_id,
                name: dep_name,
                subst,
                digest,
            } in dependencies.into_iter().flatten()
            {
                package_graph.add_edge(
                    pkg_id,
                    PackageIdentifier::from(dep_id.as_str()),
                    Dependency {
                        mode: DependencyMode::Always,
                        subst: subst.map(parse_substitution).transpose()?,
                        digest: digest.map(Symbol::from),
                        dep_override: false,
                        dep_name: PM::PackageName::from(dep_name),
                    },
                );
            }

            for schema::Dependency {
                id: dep_id,
                name: dep_name,
                subst,
                digest,
            } in dev_dependencies.into_iter().flatten()
            {
                package_graph.add_edge(
                    pkg_id,
                    PackageIdentifier::from(dep_id.as_str()),
                    Dependency {
                        mode: DependencyMode::DevOnly,
                        subst: subst.map(parse_substitution).transpose()?,
                        digest: digest.map(Symbol::from),
                        dep_override: false,
                        dep_name: PM::PackageName::from(dep_name),
                    },
                );
            }
        }

        let mut graph = DependencyGraph {
            root_path,
            root_package_id,
            root_package_name,
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

    /// Serializes this dependency graph into a lock file and returns it.
    ///
    /// This operation fails, writing nothing, if the graph contains a cycle, and can fail with an
    /// undefined output if it cannot be represented in a TOML file.
    ///
    /// `install_dir` is a working directory to create a lock file with dependency graph info.
    /// `lock_path` is an optional parameter: if it is set, and exists, this `Move.lock` will be
    /// updated with the dependency graph content. If not, the lock file is created from scratch.
    pub fn write_to_lock(
        &self,
        install_dir: PathBuf,
        lock_path: Option<PathBuf>,
    ) -> Result<LockFile> {
        use fmt::Write;
        let mut writer = String::new();

        self.write_dependencies_to_lock(self.root_package_id, &mut writer)?;

        for (id, pkg) in &self.package_table {
            writeln!(writer, "\n[[move.package]]")?;

            writeln!(writer, "id = {}", str_escape(id.as_str())?)?;
            writeln!(writer, "source = {}", PackageTOML(pkg))?;
            if let Some(version) = &pkg.version {
                writeln!(writer, "version = {}", str_escape(version.as_str())?)?;
            }

            self.write_dependencies_to_lock(*id, &mut writer)?;
        }

        let mut dependencies = None;
        let mut dev_dependencies = None;
        let mut packages = None;
        if !writer.is_empty() {
            let toml = writer.parse::<toml_edit::Document>()?;
            if let Some(value) = toml.get("dependencies").and_then(|v| v.as_value()) {
                dependencies = Some(value.clone());
            }
            if let Some(value) = toml.get("dev-dependencies").and_then(|v| v.as_value()) {
                dev_dependencies = Some(value.clone());
            }
            packages = toml
                .get("move")
                .and_then(|m| m.as_table())
                .and_then(|move_table| move_table.get("package"))
                .and_then(|v| v.as_array_of_tables().cloned());
        }

        let mut lock = match lock_path {
            // Get a handle to update an existing Move.lock. Since dependency graph updates are
            // compatible across all Move.lock schema versions, we can rely on the existing version.
            Some(lock_path) if lock_path.exists() => LockFile::from(install_dir, &lock_path)?,
            // Initialize a lock file if no existing lock_path is set for this operation.
            _ => {
                use std::io::Seek;
                let mut lock = LockFile::new(
                    install_dir,
                    self.manifest_digest.clone(),
                    self.deps_digest.clone(),
                )?;
                lock.flush()?;
                lock.rewind()?;
                lock
            }
        };
        schema::update_dependency_graph(
            &mut lock,
            self.manifest_digest.clone(),
            self.deps_digest.clone(),
            dependencies,
            dev_dependencies,
            packages,
        )?;
        Ok(lock)
    }

    /// Helper function to output the dependencies and dev-dependencies of `name` from this
    /// dependency graph, to the lock file under `writer`.
    fn write_dependencies_to_lock<W: fmt::Write>(
        &self,
        id: PackageIdentifier,
        writer: &mut W,
    ) -> Result<()> {
        let mut deps: Vec<_> = self
            .package_graph
            .edges(id)
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

    /// Resolves the packages described at dependency `to` the dependency specified by`from`
    /// with manifest at path `package_path` by running the binary `resolver`. `mode`
    /// decides whether the resulting packages are added to `self` as dependencies of
    /// `package_name` or dev-dependencies.
    ///
    /// Sends progress updates to `progress_output`, including stderr from the resolver, and
    /// captures stdout. The output is expected to be one or more null-separated string content.
    /// Each string is interpreted as a dependency graph, as represented by the Move.lock TOML
    /// schema. It returns each subgraph to be merged into the whole program dependency graph.
    fn get_external<Progress: Write>(
        mode: DependencyMode,
        from_id: PackageIdentifier,
        from_name: PM::PackageName,
        to_name: PM::PackageName,
        resolver: Symbol,
        package_path: &Path,
        progress_output: &mut Progress,
    ) -> Result<
        Vec<(
            DependencyGraph,
            PM::Dependency,
            PM::PackageName,
            Option<Symbol>, // version
        )>,
    > {
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
            "{progress_label} {to_name} {} {from_name} {} {resolver}",
            "FROM".bold().green(),
            "WITH".bold().green(),
        )?;

        // Call out to the external resolver
        // TODO(optimization): this will collect all stdout in memory, but can be streamed instead.
        let output = Command::new(resolver.as_str())
            .arg(format!("--resolve-move-{mode_label}"))
            .arg(to_name.as_str())
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
                "'{resolver}' failed to resolve {mode_label} for dependency '{to_name}' of package \
                 '{from_name}'"
            );

            if let Some(code) = output.status.code() {
                bail!("{err_msg}. Exited with code: {code}");
            } else {
                bail!("{err_msg}. Terminated by signal");
            }
        }

        let mut result = Vec::new();
        let mut reader = BufReader::new(output.stdout.as_slice());
        let mut buffer = Vec::new();
        // Loop over null-separated lock file contents, creating the graph and adding it to the result.
        loop {
            match reader.read_until(0, &mut buffer) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    // Remove the null byte if it's present
                    if buffer.last() == Some(&0) {
                        buffer.pop();
                    }

                    let sub_graph = DependencyGraph::read_from_lock(
                        package_path.to_path_buf(),
                        from_id,
                        from_name,
                        &mut buffer.as_slice(),
                        Some(resolver),
                    ).with_context(|| {
                        format!("Parsing response from '{resolver}' for dependency '{to_name}' of package '{from_id}'")
                    })?;

                    let root_sub_package_id = match sub_graph
                        .package_graph
                        .edges(from_id)
                        .collect::<Vec<_>>()
                        .as_slice()
                    {
                        [(_, id, _)] => *id,
                        // TODO: We can in fact allow allow multiple root packages / graphs and relax this constraint.
                        _ => bail!("Expected a single root dependency but none or multiple found"),
                    };
                    let root_sub_package_version = sub_graph
                        .package_table
                        .get(&root_sub_package_id)
                        .unwrap()
                        .version;

                    let new_dep = PM::Dependency::External(root_sub_package_id);
                    result.push((
                        sub_graph,
                        new_dep,
                        root_sub_package_id,
                        root_sub_package_version,
                    ));
                    buffer.clear();
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(result)
    }

    /// Checks that every dependency in the graph, excluding the root package, is present in the
    /// package table.
    fn check_consistency(&self) -> Result<()> {
        for package in self.package_graph.nodes() {
            if package == self.root_package_id {
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
            return Ok(());
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
        let mut frontier = vec![self.root_package_id];
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

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
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

            PM::DependencyKind::OnChain(PM::OnChainInfo { id }) => {
                write!(f, "id = ")?;
                f.write_str(&str_escape(id.as_str())?)?;
            }
        }

        Ok(())
    }
}

impl<'a> fmt::Display for PackageTOML<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{ ")?;
        write!(f, "{}", self.0)?;
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
            id,
            Dependency {
                mode: _,
                subst,
                digest,
                dep_override: _,
                dep_name,
            },
        ) = self;

        f.write_str("{ ")?;

        write!(f, "id = ")?;
        f.write_str(&str_escape(id.as_str())?)?;

        write!(f, ", name = ")?;
        f.write_str(&str_escape(dep_name.as_str())?)?;

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
                    f.write_str(&str_escape(
                        &account.to_canonical_string(/* with_prefix */ false),
                    )?)?;
                }
            }

            Ok(())
        }

        let mut substs = self.0.iter();

        let Some((addr, subst)) = substs.next() else {
            return f.write_str("{}");
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
    pkg_path: String,
    dependencies: Vec<(&Dependency, PM::PackageName, &Package)>,
) -> String {
    let mut s = format!("\nAt {}", pkg_path);
    if !dependencies.is_empty() {
        for (dep, _, pkg) in dependencies {
            let pkg_name = dep.dep_name;
            s.push_str("\n\t");
            s.push_str(&format!("{pkg_name} = "));
            s.push_str("{ ");
            s.push_str(&format!("{pkg}"));
            if let Some(digest) = dep.digest {
                s.push_str(&format!(", digest = {digest}"));
            }
            if let Some(subst) = &dep.subst {
                s.push_str(&format!(", addr_subst = {}", SubstTOML(subst)));
            }
            s.push_str(" }");
        }
    } else {
        s.push_str("\n\tno dependencies");
    }
    s
}

/// Checks if dependencies of a given package in two different dependency graph maps are the same,
/// checking both the dependency in the graph and the destination package (both can be
/// different). Returns Ok(()) if they are and the two parts of the symmetric different between
/// dependencies inside Err if they aren't.
fn deps_equal<'a>(
    pkg_id: Symbol,
    graph1: &'a DependencyGraph,
    graph1_pkg_table: &'a BTreeMap<PackageIdentifier, Package>,
    graph2: &'a DependencyGraph,
    graph2_pkg_table: &'a BTreeMap<PackageIdentifier, Package>,
    overrides: &'a BTreeMap<PackageIdentifier, Package>,
) -> std::result::Result<
    (),
    (
        Vec<(&'a Dependency, PackageIdentifier, &'a Package)>,
        Vec<(&'a Dependency, PackageIdentifier, &'a Package)>,
    ),
> {
    // Unwraps in the code below are safe as these edges (and target nodes) must exist either in the
    // sub-graph or in the pre-populated combined graph (see pkg_table_for_deps_compare's doc
    // comment for a more detailed explanation). If these were to fail, it would indicate a bug in
    // the algorithm so it's OK to panic here.
    let graph1_edges = graph1
        .package_graph
        .edges(pkg_id)
        .map(|(_, pkg, dep)| {
            (
                pkg,
                (
                    dep,
                    graph1_pkg_table
                        .get(&pkg)
                        .or_else(|| overrides.get(&pkg))
                        .unwrap(),
                ),
            )
        })
        .collect::<BTreeMap<PM::PackageName, (&Dependency, &Package)>>();
    let graph2_edges = graph2
        .package_graph
        .edges(pkg_id)
        .map(|(_, pkg, dep)| {
            (
                pkg,
                (
                    dep,
                    graph2_pkg_table
                        .get(&pkg)
                        .or_else(|| overrides.get(&pkg))
                        .unwrap(),
                ),
            )
        })
        .collect::<BTreeMap<PM::PackageName, (&Dependency, &Package)>>();

    // Compare deps in both graphs. See `PartialEq` implementation for `Package` for more details.
    let mut graph1_pkgs = vec![];
    for (k, v) in graph1_edges.iter() {
        if !graph2_edges.contains_key(k) || graph2_edges.get(k) != Some(v) {
            graph1_pkgs.push((v.0, *k, v.1));
        }
    }
    let mut graph2_pkgs = vec![];
    for (k, v) in graph2_edges.iter() {
        if !graph1_edges.contains_key(k) || graph1_edges.get(k) != Some(v) {
            graph2_pkgs.push((v.0, *k, v.1));
        }
    }

    if graph1_pkgs.is_empty() && graph2_pkgs.is_empty() {
        Ok(())
    } else {
        Err((graph1_pkgs, graph2_pkgs))
    }
}

/// Cycle detection to avoid infinite recursion due to the way we construct internally resolved
/// sub-graphs, expecting to end recursion at leaf packages that have no dependencies.
fn check_for_dep_cycles(
    dep: PM::InternalDependency,
    dep_pkg_id: PackageIdentifier,
    dep_pkg_name: PM::PackageName,
    internal_dependencies: &mut VecDeque<(PM::PackageName, PM::InternalDependency)>,
) -> Result<()> {
    if internal_dependencies.contains(&(dep_pkg_id, dep.clone())) {
        let (mut processed_name, mut processed_dep) = internal_dependencies.pop_back().unwrap();
        while processed_name != dep_pkg_id || processed_dep != dep {
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

/// Find the shortest path from a root of the graph to a given dependency and return it in a string
/// format.
fn dep_path_from_root(
    root_package_id: PackageIdentifier,
    root_package_name: PM::PackageName,
    graph: &DependencyGraph,
    orig_dep_name: PM::PackageName,
    pkg_name: PM::PackageName,
    is_external: bool,
) -> Result<String> {
    match algo::astar(
        &graph.package_graph,
        graph.root_package_id,
        |dst| dst == pkg_name,
        |_| 1,
        |_| 0,
    ) {
        None => bail!(
            "When resolving dependencies for package {}, \
             expected a dependency path between {} and {} which does not exist",
            root_package_name,
            graph.root_package_name,
            pkg_name
        ),
        Some((_, p)) => {
            let mut path: Vec<&str> = vec![];

            let mut i = p.iter();
            if !is_external && root_package_id != graph.root_package_id {
                // Externally resolved graphs contain a path to the package in the enclosing graph.
                // This package has to be removed from the path for the output to be consistent
                // between internally and externally resolved graphs. We have a similar situation
                // when computing a path in an already combined graph (which was pre-populated with
                // direct dependencies).
                path.push(orig_dep_name.as_str());
            }
            let mut current = match i.next() {
                Some(dep) => dep,
                None => return Ok("".to_string()),
            };
            for next in i {
                let dep = match graph.package_graph.edge_weight(*current, *next) {
                    Some(dep) => dep,
                    None => return Ok(String::from("")),
                };
                path.push(dep.dep_name.as_str());
                current = next;
            }

            Ok(path.join(" -> "))
        }
    }
}

fn get_original_dep_name(graph: &DependencyGraph, id: PackageIdentifier) -> PM::PackageName {
    let map = graph
        .package_graph
        .edges(graph.root_package_id)
        .map(|(_, name, dep)| (name, dep.dep_name))
        .collect::<BTreeMap<_, _>>();
    *map.get(&id).unwrap_or(&id)
}
