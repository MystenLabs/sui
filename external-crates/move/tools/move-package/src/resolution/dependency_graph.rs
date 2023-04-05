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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub kind: PM::DependencyKind,
    pub version: Option<PM::Version>,
    /// Optional field set if the package was externally resolved.
    resolver: Option<Symbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dependency {
    pub mode: DependencyMode,
    pub subst: Option<PM::Substitution>,
    pub digest: Option<PM::PackageDigest>,
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
        } in external_requests
        {
            graph
                .resolve_externally(mode, from, to, resolver, &pkg_path, progress_output)
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

    /// Add the graph in `extension` to `self` consuming it in the process.  Assumes the root of
    /// `extension` is the only shared node between the two, and fails if this is not the case.
    /// Labels packages coming from `extension` as being resolved by `resolver`.
    ///
    /// It is an error to attempt to merge into `self` after its `always_deps` (the set of packages
    /// that are always transitive dependencies of its root, regardless of mode) has been
    /// calculated.  This usually happens when the graph is created, so this function is intended
    /// primarily for internal use, but is exposed for testing.
    pub fn merge(&mut self, extension: DependencyGraph, resolver: Symbol) -> Result<()> {
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

        for (ext_name, mut ext_pkg) in ext_table {
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

            match self.package_table.entry(ext_name) {
                Entry::Vacant(entry) => {
                    entry.insert(ext_pkg);
                }

                // Seeing the same package in `extension` is OK only if it has the same set of
                // dependencies as the existing one.i
                Entry::Occupied(_) => {
                    let (self_deps, ext_deps) =
                        pkg_deps_equal(ext_name, &self.package_graph, &ext_graph);
                    if self_deps != ext_deps {
                        bail!(
                            "Conflicting dependencies found for '{ext_name}' during external resolution by '{resolver}':\n{}{}",
                            format_deps("\nExternal dependencies not found:", self_deps),
                            format_deps("\nNew external dependencies:", ext_deps),
                        );
                    }
                }
            }
        }

        // Because all the packages in `extensions`'s package table didn't exist in `self`'s, all
        // `ext_graph`'s edges are known to not occur in `self.package_graph` and can be added
        // without worrying about introducing duplicate edges.
        for (from, to, dep) in ext_graph.all_edges() {
            self.package_graph.add_edge(from, to, dep.clone());
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
        progress_output: &mut Progress,
    ) -> Result<()> {
        let from = package.package.name;
        for (to, dep) in &package.dependencies {
            match dep {
                PM::Dependency::External(resolver) => external_requests.push(ExternalRequest {
                    mode: DependencyMode::Always,
                    from,
                    to: *to,
                    resolver: *resolver,
                    pkg_path: package_path.to_path_buf(),
                }),
                PM::Dependency::Internal(dep) => self.resolve_internally(
                    DependencyMode::Always,
                    from,
                    *to,
                    parent,
                    dep.clone(),
                    dependency_cache,
                    external_requests,
                    progress_output,
                )?,
            }
        }

        for (to, dep) in &package.dev_dependencies {
            match dep {
                PM::Dependency::External(resolver) => external_requests.push(ExternalRequest {
                    mode: DependencyMode::DevOnly,
                    from,
                    to: *to,
                    resolver: *resolver,
                    pkg_path: package_path.to_path_buf(),
                }),

                PM::Dependency::Internal(dep) => self.resolve_internally(
                    DependencyMode::DevOnly,
                    from,
                    *to,
                    parent,
                    dep.clone(),
                    dependency_cache,
                    external_requests,
                    progress_output,
                )?,
            }
        }

        Ok(())
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

        self.merge(sub_graph, resolver).with_context(|| {
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
        progress_output: &mut Progress,
    ) -> Result<()> {
        let PM::InternalDependency {
            kind,
            version,
            subst,
            digest,
        } = dep;

        let mut pkg = Package {
            kind,
            version,
            resolver: None,
        };

        pkg.kind.reroot(parent)?;
        self.process_dependency(
            pkg,
            to,
            dependency_cache,
            external_requests,
            progress_output,
        )?;
        self.package_graph.add_edge(
            from,
            to,
            Dependency {
                mode,
                subst,
                digest,
            },
        );

        Ok(())
    }

    /// Ensures that package `pkg_name` and all its transitive dependencies are present in the
    /// graph, all sourced from their respective packages, `pkg`.  Fails if any of the packages in
    /// the dependency sub-graph rooted at `pkg_name` are already present in `self` but sourced from
    /// a different dependency.
    fn process_dependency<Progress: Write>(
        &mut self,
        pkg: Package,
        name: PM::PackageName,
        dependency_cache: &mut DependencyCache,
        external_requests: &mut Vec<ExternalRequest>,
        progress_output: &mut Progress,
    ) -> Result<()> {
        let pkg = match self.package_table.entry(name) {
            Entry::Vacant(entry) => entry.insert(pkg),

            // Seeing the same package again, pointing to the same dependency: OK, return early.
            Entry::Occupied(entry) if entry.get() == &pkg => {
                return Ok(());
            }

            // Seeing the same package again, but pointing to a different dependency: Not OK.
            Entry::Occupied(entry) => {
                bail!(
                    "Conflicting dependencies found:\n{0} = {1}\n{0} = {2}",
                    name,
                    PackageWithResolverTOML(entry.get()),
                    PackageWithResolverTOML(&pkg),
                );
            }
        };

        dependency_cache
            .download_and_update_if_remote(name, &pkg.kind, progress_output)
            .with_context(|| format!("Fetching '{}'", name))?;

        let pkg_path = self.root_path.join(local_path(&pkg.kind));
        let manifest = parse_move_manifest_from_file(&pkg_path)
            .with_context(|| format!("Parsing manifest for '{}'", name))?;

        let kind = pkg.kind.clone();
        self.extend_graph(
            &kind,
            &manifest,
            &pkg_path,
            dependency_cache,
            external_requests,
            progress_output,
        )
        .with_context(|| format!("Resolving dependencies for package '{}'", name))
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

/// Checks if dependencies of a given package in two different dependency graph maps are the
/// same.
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
