// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use colored::Colorize;
use move_symbol_pool::Symbol;
use petgraph::{algo, prelude::DiGraphMap, Direction};
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    fmt,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    package_hooks,
    source_package::{
        manifest_parser::{parse_dependency, parse_move_manifest_from_file, parse_substitution},
        parsed_manifest as PM,
    },
};

use super::{
    dependency_cache::DependencyCache,
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
/// necessary, or by reading its serialized contents from a lock file.  Both these processes will
/// fail if any of the criteria above cannot be met (e.g. if the graph contains a cycle, the same
/// package is fetched multiple times from different sources, or information about a package's
/// source is not available).
///
/// In order to be `BuildConfig` agnostic, it contains `dev-dependencies` as well as `dependencies`
/// and labels edges in the graph accordingly, as `DevOnly`, or `Always` dependencies.
///
/// When building a dependency graph, different versions of the same (transitively) dependent
/// package can be encountered. If this is indeed the case, a single version must be chosen by the
/// developer to be the override, and this override must be specified in a manifest file whose
/// package dominates all the conflicting "uses" of the dependent package. These overrides are taken
/// into consideration during the dependency graph construction.
///
/// When constructing the graph (top to bottom) for internal dependencies (external dependencies are
/// batch-processed at the end of the graph construction), we maintain a set of the current
/// overrides collected when processing dependencies (starting with an empty set at the root package).
///
/// When processing dependencies of a given package, we process overrides first to collect a list of
/// overrides to be added to the set (if they are not yet in the set - outer overrides "win") and
/// then process non-overridden dependencies using a freshly updated overrides set. We use this
/// overrides set when attempting to insert a package into the graph (with an entry for this package
/// already existing or not) via the `process_graph_entry` function. After a package is fully
/// processed, remove its own overrides from the set.
///
/// External dependencies are provided by external resolvers as fully formed dependency sub-graphs
/// that need to be inserted into the "main" dependency graph being constructed. Whenever an
/// external dependency is encountered, it's "recorded" along with the set of overrides available at
/// the point of sub-graph insertion, and batch-merged (using the `merge` function) after
/// construction of the entire internally resolved graph is completed.
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
}

#[derive(Debug, Clone, Eq)]
pub struct Package {
    pub kind: PM::DependencyKind,
    pub version: Option<PM::Version>,
    /// Optional field set if the package was externally resolved.
    resolver: Option<Symbol>,
    /// Set if the package was inserted while some overrides were active.
    overridden_path: bool,
}

impl PartialEq for Package {
    fn eq(&self, other: &Self) -> bool {
        // comparison should neither contain overridden_path (as it's only used to determine if
        // package should be re-inspected) nor the type of resolver (as it would actually lead to
        // incorrect result when comparing packages during insertion of externally resolved ones -
        // an internally resolved existing package in the graph would not be recognized as a
        // potential different version of the externally resolved one)
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

/// Keeps information about external resolution request
#[derive(Debug, Clone)]
pub struct ExternalRequest {
    mode: DependencyMode,
    from: Symbol,
    to: Symbol,
    resolver: Symbol,
    pkg_path: PathBuf,
    /// overrides at the point of external graph insertion
    overrides: BTreeMap<PM::PackageName, Package>,
}

/// Wrapper struct to display a package as an inline table in the lock file (matching the
/// convention in the source manifest).  This is necessary becase the `toml` crate does not
/// currently support serializing types as inline tables.
struct PackageTOML<'a>(&'a Package);
struct PackageWithResolverTOML<'a>(&'a Package);
struct DependencyTOML<'a>(PM::PackageName, &'a Dependency);
struct SubstTOML<'a>(&'a PM::Substitution);

impl DependencyGraph {
    /// Build a graph from the transitive dependencies and dev-dependencies of `root_package`.
    ///
    /// `skip_fetch_latest_git_deps` controls whether package resolution will fetch the latest
    /// versions of remote dependencies, even if a version already exists locally.
    ///
    /// `progress_output` is an output stream that is written to while generating the graph, to
    /// provide human-readable progress updates.
    pub fn new<Progress: Write>(
        root_package: &PM::SourceManifest,
        root_path: PathBuf,
        dependency_cache: &mut DependencyCache,
        progress_output: &mut Progress,
    ) -> Result<DependencyGraph> {
        let mut graph = DependencyGraph {
            root_path: root_path.clone(),
            root_package: root_package.package.name,
            package_graph: DiGraphMap::new(),
            package_table: BTreeMap::new(),
            always_deps: BTreeSet::new(),
        };

        // Ensure there's always a root node, even if it has no edges.
        graph.package_graph.add_node(graph.root_package);
        // Collect external resolution requests and process them later to check for "safe"
        // overlapping packages existing in both externally and internally resolved graphs.
        let mut external_requests = vec![];
        graph
            .extend_graph(
                &PM::DependencyKind::default(),
                root_package,
                &root_path,
                dependency_cache,
                &mut external_requests,
                &mut BTreeMap::new(),
                progress_output,
            )
            .with_context(|| {
                format!(
                    "Failed to resolve dependencies for package '{}'",
                    graph.root_package
                )
            })?;

        for ExternalRequest {
            mode,
            from,
            to,
            resolver,
            pkg_path,
            overrides,
        } in external_requests
        {
            graph
                .resolve_externally(
                    mode,
                    from,
                    to,
                    resolver,
                    &pkg_path,
                    &overrides,
                    progress_output,
                )
                .with_context(|| {
                    format!(
                        "Failed to resolve dependencies for package '{}'",
                        graph.root_package
                    )
                })?
        }

        graph.check_acyclic()?;
        graph.discover_always_deps();

        Ok(graph)
    }

    /// Create a dependency graph by reading a lock file.
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
    ) -> Result<DependencyGraph> {
        let mut package_graph = DiGraphMap::new();
        let mut package_table = BTreeMap::new();

        let packages = schema::Packages::read(lock)?;

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
                resolver: None,
                overridden_path: false,
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
        };

        graph.check_consistency()?;
        graph.check_acyclic()?;
        graph.discover_always_deps();
        Ok(graph)
    }

    /// Serialize this dependency graph into a lock file and return it.
    ///
    /// This operation fails, writing nothing, if the graph contains a cycle, and can fail with an
    /// undefined output if it cannot be represented in a TOML file.
    pub fn write_to_lock(&self, install_dir: PathBuf) -> Result<LockFile> {
        let lock = LockFile::new(install_dir)?;
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

    /// A "root" function responsible for adding the graph in `extension` to `self` (the core of the
    /// merging process is implemented recursively in the `merge_pkg` function). Packages can be
    /// shared between the two as long as either:
    /// - they are consistent (have the same name and the same set of dependencies)
    /// - if a valid override exists for the otherwise conflicting packages
    ///
    /// Merging starts by creating an edge from the package containing the extension as its
    /// dependency (`from` argument) to the package being the "root" of the extension
    /// (`merged_pkg_name` argument). During merge, which happens on a per-package basis in the
    /// `merge_pkg` function, packages coming from `extension` are labeled as being resolved by
    /// `resolver`.
    ///
    /// It is an error to attempt to merge into `self` after its `always_deps` (the set of packages
    /// that are always transitive dependencies of its root, regardless of mode) has been
    /// calculated.  This usually happens when the graph is created, so this function is intended
    /// primarily for internal use, but is exposed for testing.
    pub fn merge(
        &mut self,
        from: PM::PackageName,
        merged_pkg_name: PM::PackageName,
        extension: DependencyGraph,
        resolver: Symbol,
        overrides: &BTreeMap<PM::PackageName, Package>,
    ) -> Result<()> {
        let DependencyGraph {
            root_package: ext_root,
            package_graph: ext_graph,
            package_table: ext_table,

            // Unnecessary in the context of the larger graph.
            root_path: _,

            // Will be recalculated for the larger graph.
            always_deps: _,
        } = extension;

        if !self.package_graph.contains_node(ext_root) {
            bail!("Can't merge dependencies for '{ext_root}' because nothing depends on it");
        }

        // If this has been calculated it is guaranteed to contain at least `self.root_package`.
        if !self.always_deps.is_empty() {
            bail!("Merging dependencies into a graph after calculating its 'always' dependencies");
        }

        if ext_table.is_empty() {
            // the external graph is effectively empty - nothing to merge
            return Ok(());
        }

        // unwrap safe as the table must have the package if the graph has it
        let merged_pkg = ext_table.get(&merged_pkg_name).unwrap();
        self.merge_pkg(
            merged_pkg.clone(),
            merged_pkg_name,
            &ext_graph,
            &ext_table,
            resolver,
            overrides,
        )?;
        // unwrap is safe as all edges have a Dependency weight
        let merged_dep = ext_graph.edge_weight(from, merged_pkg_name).unwrap();
        self.package_graph
            .add_edge(from, merged_pkg_name, merged_dep.clone());

        Ok(())
    }

    /// Recursively merge package from an `extension` graph (resolved by an external resolver) to
    /// `self`. The extension graph is traversed in a depth-first manner, successively adding
    /// packages and their connecting edges to `self` via the `process_graph_entry`
    /// function. Additionally, during traversal the algorithm detects which of the sub-graph's
    /// packages need to be overridden (in which case their dependencies in `extension` should no
    /// longer be inserted into `self`).
    fn merge_pkg(
        &mut self,
        mut ext_pkg: Package,
        ext_name: PM::PackageName,
        ext_graph: &DiGraphMap<PM::PackageName, Dependency>,
        ext_table: &BTreeMap<PM::PackageName, Package>,
        resolver: Symbol,
        overrides: &BTreeMap<PM::PackageName, Package>,
    ) -> Result<()> {
        ext_pkg.resolver = Some(resolver);

        // The root package is not present in the package table (because it doesn't have a
        // source).  If it appears in the other table, it indicates a cycle.
        if ext_name == self.root_package {
            bail!(
                "Conflicting dependencies found:\n{0} = 'root'\n{0} = {1}",
                ext_name,
                PackageWithResolverTOML(&ext_pkg),
            );
        }

        if self
            .process_graph_entry(
                &ext_pkg,
                ext_name,
                overrides,
                Some(ext_graph),
                Some(resolver),
            )?
            .is_some()
        {
            // existing entry was found
            return Ok(());
        }

        // if we are here, it means that a new package has been inserted into the graph - we need to
        // process its dependencies and add appropriate edges to them
        for dst in ext_graph.neighbors_directed(ext_name, Direction::Outgoing) {
            // unwrap safe as the table must have the package if the graph has it
            let dst_pkg = ext_table.get(&dst).unwrap();
            self.merge_pkg(
                dst_pkg.clone(),
                dst,
                ext_graph,
                ext_table,
                resolver,
                overrides,
            )?;
            // unwrap is safe as all edges have a Dependency weight
            let ext_dep = ext_graph.edge_weight(ext_name, dst).unwrap();
            self.package_graph.add_edge(ext_name, dst, ext_dep.clone());
        }

        Ok(())
    }

    /// Return packages in the graph in topological order (a package is ordered before its
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

    /// Return an iterator over `pkg`'s immediate dependencies in the graph.  If `mode` is
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

    /// Add the transitive dependencies and dev-dependencies from `package` to the dependency graph.
    fn extend_graph<Progress: Write>(
        &mut self,
        parent: &PM::DependencyKind,
        package: &PM::SourceManifest,
        package_path: &Path,
        dependency_cache: &mut DependencyCache,
        external_requests: &mut Vec<ExternalRequest>,
        overrides: &mut BTreeMap<PM::PackageName, Package>,
        progress_output: &mut Progress,
    ) -> Result<()> {
        let from = package.package.name;

        self.extend_with_dependencies(
            DependencyMode::Always,
            &package.dependencies,
            from,
            parent,
            package_path,
            dependency_cache,
            external_requests,
            overrides,
            progress_output,
        )?;

        self.extend_with_dependencies(
            DependencyMode::DevOnly,
            &package.dev_dependencies,
            from,
            parent,
            package_path,
            dependency_cache,
            external_requests,
            overrides,
            progress_output,
        )?;

        Ok(())
    }

    /// Iterate over the set of a given package's dependencies (overridden dependencies first). to
    /// add them to the dependency graph.
    fn extend_with_dependencies<Progress: Write>(
        &mut self,
        mode: DependencyMode,
        dependencies: &PM::Dependencies,
        from: Symbol,
        parent: &PM::DependencyKind,
        package_path: &Path,
        dependency_cache: &mut DependencyCache,
        external_requests: &mut Vec<ExternalRequest>,
        overrides: &mut BTreeMap<PM::PackageName, Package>,
        progress_output: &mut Progress,
    ) -> Result<()> {
        // partition dep into overrides and not
        let (overridden_deps, deps): (Vec<_>, Vec<_>) =
            dependencies.iter().partition(|(_, dep)| {
                matches!(
                    dep,
                    PM::Dependency::Internal(PM::InternalDependency {
                        dep_override: true,
                        ..
                    })
                )
            });

        // Process overrides first to include them in processing of non-overridden deps. It is
        // important to do so as a dependency override may "prune" portions of a dependency graph
        // that would otherwise prevent other dependencies from kicking in. In other words, a given
        // override may be the dominant one only if another override eliminates some graph
        // edges. See diamond_problem_dep_transitive_nested_override for an example (in tests) of
        // such situation.
        //
        // It's also pretty important that we do not extend overrides with the override being
        // currently processed. The reason for it is that in order to detect incorrect overrides
        // (such that do not dominate all package "uses") we rely on the package being reachable via
        // different paths:
        // - if it's reached via an overridden path again for the same override, it's OK
        // - if it's reached via an overridden path again for a different override, it's an error
        // - if it's reached via a non-overridden path, it's an error (insufficient override)
        //
        // While the first type of error could still be detected if we injected the currently
        // processed override into the overrides set, the second one would not.  Consider
        // diamond_problem_dep_incorrect_override_occupied example (in tests) to see a situation
        // when a non-overridden path is chosen first to insert the package and then insufficient
        // override could be considered correct if we injected it into the overrides set (as we will
        // not have another path to explore that would disqualify it).
        let mut local_overrides = BTreeMap::new();
        for (to, dep) in overridden_deps {
            let inserted_pkg = self.extend_with_dep(
                mode,
                from,
                *to,
                dep,
                parent,
                package_path,
                dependency_cache,
                external_requests,
                overrides,
                progress_output,
            )?;
            // do not include already overridden overrides
            if let Some(pkg) = inserted_pkg {
                if !overrides.contains_key(to) {
                    local_overrides.insert(*to, pkg);
                }
            }
        }

        // add new overrides to the set
        overrides.extend(local_overrides.clone());

        for (to, dep) in deps {
            self.extend_with_dep(
                mode,
                from,
                *to,
                dep,
                parent,
                package_path,
                dependency_cache,
                external_requests,
                overrides,
                progress_output,
            )?;
        }
        // remove locally added overrides from the set
        overrides.retain(|k, _| !local_overrides.contains_key(k));
        Ok(())
    }

    /// Extend the dependency graph with a single dependent package.
    fn extend_with_dep<Progress: Write>(
        &mut self,
        mode: DependencyMode,
        from: Symbol,
        to: Symbol,
        dep: &PM::Dependency,
        parent: &PM::DependencyKind,
        package_path: &Path,
        dependency_cache: &mut DependencyCache,
        external_requests: &mut Vec<ExternalRequest>,
        overrides: &mut BTreeMap<PM::PackageName, Package>,
        progress_output: &mut Progress,
    ) -> Result<Option<Package>> {
        let inserted_pkg = match dep {
            PM::Dependency::External(resolver) => {
                external_requests.push(ExternalRequest {
                    mode,
                    from,
                    to,
                    resolver: *resolver,
                    pkg_path: package_path.to_path_buf(),
                    overrides: overrides.clone(),
                });
                None
            }

            PM::Dependency::Internal(dep) => Some(self.resolve_internally(
                mode,
                from,
                to,
                parent,
                dep.clone(),
                dependency_cache,
                external_requests,
                overrides,
                progress_output,
            )?),
        };
        Ok(inserted_pkg)
    }

    /// Resolve the packages described at dependency `to` of package `from` with manifest at path
    /// `package_path` by running the binary `resolver.  `mode` decides whether the resulting
    /// packages are added to `self` as dependencies of `package_name` or dev-dependencies.
    ///
    /// Sends progress updates to `progress_output`, including stderr from the resolver, and
    /// captures stdout, which is assumed to be a lock file containing the result of package
    /// resolution.
    fn resolve_externally<Progress: Write>(
        &mut self,
        mode: DependencyMode,
        from: PM::PackageName,
        to: PM::PackageName,
        resolver: Symbol,
        package_path: &Path,
        overrides: &BTreeMap<PM::PackageName, Package>,
        progress_output: &mut Progress,
    ) -> Result<()> {
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
        )
        .with_context(|| {
            format!("Parsing response from '{resolver}' for dependency '{to}' of package '{from}'")
        })?;

        self.merge(from, to, sub_graph, resolver, overrides)
            .with_context(|| {
                format!("Adding dependencies from {resolver} for dependency '{to}' in '{from}'")
            })?;

        Ok(())
    }

    /// Use the internal resolution mechanism (which recursively explores transitive dependencies)
    /// to resolve packages reachable from `to` (inclusive), which was found as a dependency `dep`
    /// of package `from` whose source is `parent`, adding them to `self`.
    ///
    /// Avoids re-fetching git repositories if they are already available locally, when
    /// `skip_fetch_latest_git_deps` is true, and sends progress updates to `progress_output`.
    fn resolve_internally<Progress: Write>(
        &mut self,
        mode: DependencyMode,
        from: PM::PackageName,
        to: PM::PackageName,
        parent: &PM::DependencyKind,
        dep: PM::InternalDependency,
        dependency_cache: &mut DependencyCache,
        external_requests: &mut Vec<ExternalRequest>,
        overrides: &mut BTreeMap<PM::PackageName, Package>,
        progress_output: &mut Progress,
    ) -> Result<Package> {
        let PM::InternalDependency {
            kind,
            version,
            subst,
            digest,
            dep_override,
        } = dep;

        // are there active overrides for this path in the graph?
        let overridden_path = !overrides.is_empty();
        let mut pkg = Package {
            kind,
            version,
            resolver: None,
            overridden_path,
        };

        pkg.kind.reroot(parent)?;
        let inserted_pkg = self.process_dependency(
            pkg,
            to,
            dependency_cache,
            external_requests,
            overrides,
            progress_output,
        )?;
        self.package_graph.add_edge(
            from,
            to,
            Dependency {
                mode,
                subst,
                digest,
                dep_override,
            },
        );
        Ok(inserted_pkg)
    }

    /// Ensure that package `pkg_name` and all its transitive dependencies are present in the graph,
    /// all sourced from their respective packages, `pkg`.  Fails if any of the packages in the
    /// dependency sub-graph rooted at `pkg_name` are already present in `self` but sourced from a
    /// different dependency.
    fn process_dependency<Progress: Write>(
        &mut self,
        pkg: Package,
        name: PM::PackageName,
        dependency_cache: &mut DependencyCache,
        external_requests: &mut Vec<ExternalRequest>,
        overrides: &mut BTreeMap<PM::PackageName, Package>,
        progress_output: &mut Progress,
    ) -> Result<Package> {
        if let Some(existing_entry) = self.process_graph_entry(
            &pkg, name, overrides, /* external_subgraph */ None, /* resolver */ None,
        )? {
            // existing entry was found
            return Ok(existing_entry);
        }
        // a package has been inserted into the graph - process its dependencies

        dependency_cache
            .download_and_update_if_remote(name, &pkg.kind, progress_output)
            .with_context(|| format!("Fetching '{}'", name))?;

        let pkg_path = self.root_path.join(local_path(&pkg.kind));
        let manifest = parse_move_manifest_from_file(&pkg_path)
            .with_context(|| format!("Parsing manifest for '{}'", name))?;

        let inserted_pkg = pkg.clone();
        let kind = pkg.kind.clone();
        self.extend_graph(
            &kind,
            &manifest,
            &pkg_path,
            dependency_cache,
            external_requests,
            overrides,
            progress_output,
        )
        .with_context(|| format!("Resolving dependencies for package '{}'", name))?;
        Ok(inserted_pkg)
    }

    /// Attempt to insert a newly encountered package to the graph which may or may not already
    /// contain an entry for the same package name:
    ///    - if no package exists in the graph, insert it
    ///    - if a conflicting package already exists in the graph, override it if an override can be
    ///    found in the set (if it does not, report an error)
    ///    - if the same package already exists in the graph, and this package is on the "override
    ///    path" keep checking its dependencies to make sure that previously used overrides are
    ///    correct (dominate all uses of the package); a package is marked to be on the "override
    ///    path" if it is inserted into the graph while the overrides set is non-empty, making this
    ///    mark into a coarse indicator of whether portions of the graph need to be (re)validated.
    fn process_graph_entry(
        &mut self,
        pkg: &Package,
        name: PM::PackageName,
        overrides: &BTreeMap<PM::PackageName, Package>,
        external_subgraph: Option<&DiGraphMap<PM::PackageName, Dependency>>,
        resolver: Option<Symbol>,
    ) -> Result<Option<Package>> {
        match self.package_table.entry(name) {
            Entry::Vacant(entry) => {
                // Note that we simply insert a dependent package here without checking the
                // overrides set. The reason for it is that if there was an override for this entry,
                // it would have already been inserted as the overrides are processed before
                // non-overridden dependencies (and only after they are processed, the overrides set
                // is populated).
                entry.insert(pkg.clone());
                Ok(None)
            }

            // Seeing the same package again, pointing to the same dependency: OK, return early but
            // only if seeing a package that was not on an "override path" (was created when no
            // override was active); otherwise we need to keep inspecting the graph to make sure
            // that the overrides introduced on this path correctly dominate all "uses" of a given
            // package
            Entry::Occupied(entry) if entry.get() == pkg => {
                if let Some(ext_graph) = external_subgraph {
                    // when trying to insert a package into the graph as part of merging an external
                    // subgraph it's not enough to check for package equality as it does not capture
                    // dependencies that may differ between the internally and externally resolved
                    // packages.
                    let (self_deps, ext_deps) =
                        pkg_deps_equal(name, &self.package_graph, ext_graph);
                    if self_deps != ext_deps {
                        bail!(
                            "Conflicting dependencies found for '{name}' during external resolution by '{}':\n{}{}",
                            resolver.unwrap(), // safe because external_subgraph exists
                            format_deps("\nExternal dependencies not found:", self_deps),
                            format_deps("\nNew external dependencies:", ext_deps),
                        );
                    }
                }
                if entry.get().overridden_path {
                    // check if acyclic to avoid infinite recursion - see the
                    // diamond_problem_dep_incorrect_override_cycle (in tests) for an example of
                    // such situation
                    self.check_acyclic()?;
                    // inspect the rest of the graph and report error if a problem is found
                    self.override_verify(pkg, name, overrides)?;
                }
                Ok(Some(pkg.clone()))
            }

            // Seeing the same package again, but pointing to a different dependency: Not OK unless
            // there is an override.
            Entry::Occupied(mut entry) => {
                if let Some(overridden_pkg) = overrides.get(&name) {
                    // override found - use it and return - its dependencies have already been
                    // processed the first time override pkg was processed (before it was inserted
                    // into overrides set)
                    if overridden_pkg != entry.get() {
                        entry.insert(overridden_pkg.clone());
                    }
                    Ok(Some(overridden_pkg.clone()))
                } else {
                    bail!(
                        "Conflicting dependencies found:\n{0} = {1}\n{0} = {2}",
                        name,
                        PackageWithResolverTOML(entry.get()),
                        PackageWithResolverTOML(pkg),
                    );
                }
            }
        }
    }

    /// Inspect a portion of the graph by simply following existing nodes and edges. If during
    /// inspection we encounter a package inserted as a result of an override but this override is
    /// not in the current overrides set (or a different override for the same package is in the
    /// overrides set), then the previously used override was incorrect (insufficient) and an error
    /// must be reported.
    fn override_verify(
        &self,
        pkg: &Package,
        name: PM::PackageName,
        overrides: &BTreeMap<PM::PackageName, Package>,
    ) -> Result<()> {
        // check if any (should be 0 or 1) edges are the overrides of pkg
        let pkg_overrides: Vec<_> = self
            .package_graph
            .neighbors_directed(name, Direction::Incoming)
            .filter(|src| {
                // unwrap is safe as all edges have a Dependency weight
                self.package_graph
                    .edge_weight(*src, name)
                    .unwrap()
                    .dep_override
            })
            .collect();

        if !pkg_overrides.is_empty() {
            let Some(overridden_pkg) = overrides.get(&name) else {
                bail!("Incorrect override of {} in {} (an override should dominate all uses of the overridden package)", name, pkg_overrides[0]);
            };
            if overridden_pkg != pkg {
                // This should never happen as we process overridden dependencies first. Since the
                // overridden dependency is omitted from the set of all overrides (see a comment in
                // extend_with_dependencies to see why), a conflicting override will be caught as a
                // "simple" conflicting dependency
                bail!(
                    "Incorrect override of {} in {} (conflicting overrides)",
                    name,
                    pkg_overrides[0]
                );
            }
        }

        // recursively check all other packages in the subgraph
        for dst in self
            .package_graph
            .neighbors_directed(name, Direction::Outgoing)
        {
            // unwrap is safe as dst is in the graph so it must also be in package table
            let dst_pkg = self.package_table.get(&dst).unwrap();
            self.override_verify(dst_pkg, dst, overrides)?;
        }
        Ok(())
    }

    /// Check that every dependency in the graph, excluding the root package, is present in the
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

    /// Check that there isn't a cycle between packages in the dependency graph.  Returns `Ok(())`
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

    /// Add the transitive closure of `DependencyMode::Always` edges reachable from the root package
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
            overridden_path: _,
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

fn format_deps(msg: &str, dependencies: Vec<(&Dependency, PM::PackageName)>) -> String {
    let mut s = "".to_string();
    if !dependencies.is_empty() {
        s.push_str(msg);
        for (dep, pkg) in dependencies {
            s.push_str("\n\t");
            s.push_str(format!("{}", DependencyTOML(pkg, dep)).as_str());
        }
    }
    s
}

/// Check if dependencies of a given package in two different dependency graph maps are the same.
fn pkg_deps_equal<'a>(
    pkg_name: Symbol,
    pkg_graph: &'a DiGraphMap<PM::PackageName, Dependency>,
    other_graph: &'a DiGraphMap<PM::PackageName, Dependency>,
) -> (
    Vec<(&'a Dependency, PM::PackageName)>,
    Vec<(&'a Dependency, PM::PackageName)>,
) {
    let pkg_edges = BTreeSet::from_iter(pkg_graph.edges(pkg_name).map(|(_, pkg, dep)| (dep, pkg)));
    let other_edges =
        BTreeSet::from_iter(other_graph.edges(pkg_name).map(|(_, pkg, dep)| (dep, pkg)));

    let (pkg_deps, other_deps): (Vec<_>, Vec<_>) = pkg_edges
        .symmetric_difference(&other_edges)
        .partition(|dep| pkg_edges.contains(dep));
    (pkg_deps, other_deps)
}
