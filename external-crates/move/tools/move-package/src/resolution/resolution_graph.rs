// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use move_command_line_common::files::{find_move_filenames, FileHash};
use move_core_types::account_address::AccountAddress;
use ptree::{print_tree, TreeBuilder};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    source_package::{
        layout::SourcePackageLayout,
        manifest_parser::parse_move_manifest_from_file,
        parsed_manifest::{
            FileName, NamedAddress, PackageDigest, PackageName, SourceManifest, SubstOrRename,
        },
    },
    BuildConfig,
};

use super::{
    dependency_cache::DependencyCache, dependency_graph as DG, digest::compute_digest, local_path,
    resolving_table::ResolvingTable,
};

/// The graph after resolution in which all named addresses have been assigned a value.
///
/// Named addresses can be assigned values in a couple different ways:
/// 1. They can be assigned a value in the declaring package. In this case the value of that
///    named address will always be that value.
/// 2. Can be left unassigned in the declaring package. In this case it can receive its value
///    through unification across the package graph.
///
/// Named addresses can also be renamed in a package and will be re-exported under thes new names in
/// this case.
#[derive(Debug, Clone)]
pub struct ResolvedGraph {
    pub graph: DG::DependencyGraph,
    /// Build options
    pub build_options: BuildConfig,
    /// A mapping of package name to its resolution
    pub package_table: PackageTable,
}

// rename_to => (from_package_name, from_address_name)
pub type Renaming = BTreeMap<NamedAddress, (PackageName, NamedAddress)>;
pub type ResolvedTable = BTreeMap<NamedAddress, AccountAddress>;
type PackageTable = BTreeMap<PackageName, Package>;

#[derive(Debug, Clone)]
pub struct Package {
    /// Source manifest for this package
    pub source_package: SourceManifest,
    /// Where this package is located on the filesystem
    pub package_path: PathBuf,
    /// The renaming of addresses performed by this package
    pub renaming: Renaming,
    /// The mapping of addresses that are in scope for this package.
    pub resolved_table: ResolvedTable,
    /// The digest of the contents of all source files and manifest under the package root
    pub source_digest: PackageDigest,
}

impl ResolvedGraph {
    pub fn resolve<Progress: Write>(
        graph: DG::DependencyGraph,
        mut build_options: BuildConfig,
        dependency_cache: &mut DependencyCache,
        progress_output: &mut Progress,
    ) -> Result<ResolvedGraph> {
        let mut package_table = PackageTable::new();
        let mut resolving_table = ResolvingTable::new();

        let dep_mode = if build_options.dev_mode {
            DG::DependencyMode::DevOnly
        } else {
            DG::DependencyMode::Always
        };

        // Resolve transitive dependencies in reverse topological order so that a package's
        // dependencies get resolved before it does.
        for pkg_name in graph.topological_order().into_iter().rev() {
            // Skip dev-mode packages if not in dev-mode.
            if !(build_options.dev_mode || graph.always_deps.contains(&pkg_name)) {
                continue;
            }

            // Make sure the package is available locally.
            let package_path = if pkg_name == graph.root_package {
                graph.root_path.clone()
            } else {
                let pkg = &graph.package_table[&pkg_name];
                dependency_cache
                    .download_and_update_if_remote(pkg_name, &pkg.kind, progress_output)
                    .with_context(|| format!("Fetching '{pkg_name}'"))?;
                graph.root_path.join(local_path(&pkg.kind))
            };

            let mut resolved_pkg = Package::new(package_path, &build_options)
                .with_context(|| format!("Resolving package '{pkg_name}'"))?;

            if pkg_name != resolved_pkg.source_package.package.name {
                bail!(
                    "Name of dependency '{}' does not match dependency's package name '{}'",
                    pkg_name,
                    resolved_pkg.source_package.package.name
                )
            }

            resolved_pkg
                .define_addresses_in_package(&mut resolving_table)
                .with_context(|| format!("Resolving addresses for '{pkg_name}'"))?;

            for (dep_name, dep, _pkg) in graph.immediate_dependencies(pkg_name, dep_mode) {
                resolved_pkg
                    .process_dependency(dep_name, dep, &package_table, &mut resolving_table)
                    .with_context(|| {
                        format!("Processing dependency '{dep_name}' of '{pkg_name}'")
                    })?;
            }

            package_table.insert(pkg_name, resolved_pkg);
        }

        // Add additional addresses to all package resolution tables.
        for (name, addr) in &build_options.additional_named_addresses {
            let name = NamedAddress::from(name.as_str());
            for pkg in package_table.keys() {
                resolving_table
                    .define((*pkg, name), Some(*addr))
                    .with_context(|| {
                        format!("Adding additional address '{name}' to package '{pkg}'")
                    })?;
            }
        }

        let root_package = &package_table[&graph.root_package];

        // Add dev addresses, but only for the root package
        if build_options.dev_mode {
            let mut addr_to_name_mapping = BTreeMap::new();
            for (name, addr) in resolving_table.bindings(graph.root_package) {
                if let Some(addr) = addr {
                    addr_to_name_mapping
                        .entry(*addr)
                        .or_insert_with(Vec::new)
                        .push(name);
                };
            }

            for (name, addr) in root_package
                .source_package
                .dev_address_assignments
                .iter()
                .flatten()
            {
                let root_dev_addr = (graph.root_package, *name);
                if !resolving_table.contains(root_dev_addr) {
                    bail!(
                        "Found unbound dev address assignment '{} = 0x{}' in root package '{}'. \
                         Dev addresses cannot introduce new named addresses",
                        name,
                        addr.short_str_lossless(),
                        graph.root_package,
                    );
                }

                resolving_table
                    .define(root_dev_addr, Some(*addr))
                    .with_context(|| {
                        format!(
                            "Unable to resolve named address '{}' in package '{}' when resolving \
                             dependencies in dev mode",
                            name, graph.root_package,
                        )
                    })?;

                if let Some(conflicts) = addr_to_name_mapping.insert(*addr, vec![*name]) {
                    bail!(
                        "Found non-unique dev address assignment '{name} = 0x{addr}' in root \
                         package '{pkg}'. Dev address assignments must not conflict with any other \
                         assignments in order to ensure that the package will compile with any \
                         possible address assignment. \
                         Assignment conflicts with previous assignments: {conflicts} = 0x{addr}",
                        name = name,
                        addr = addr.short_str_lossless(),
                        pkg = graph.root_package,
                        conflicts = conflicts
                            .iter()
                            .map(NamedAddress::as_str)
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                }
            }
        }

        if build_options.architecture.is_none() {
            if let Some(info) = &root_package.source_package.build {
                build_options.architecture = info.architecture;
            }
        }

        // Now that all address unification has happened, individual package resolution tables can
        // be unified.
        for pkg in package_table.values_mut() {
            pkg.finalize_address_resolution(&resolving_table)
                .with_context(|| {
                    format!(
                        "Unresolved addresses found. To fix this, add an entry for each unresolved \
                         address to the [addresses] section of {}/Move.toml: e.g.,\n\n\
                         \
                         [addresses]\n\
                         std = \"0x1\"\n\n\
                         \
                         Alternatively, you can also define [dev-addresses] and call with the -d \
                         flag",
                        graph.root_path.display()
                    )
                })?;
        }

        Ok(ResolvedGraph {
            graph,
            build_options,
            package_table,
        })
    }

    pub fn root_package(&self) -> PackageName {
        self.graph.root_package
    }

    pub fn get_package(&self, name: PackageName) -> &Package {
        self.package_table.get(&name).unwrap()
    }

    /// Return the names of packages in this resolution graph in topological order.
    pub fn topological_order(&self) -> Vec<PackageName> {
        let mut order = self.graph.topological_order();
        if !self.build_options.dev_mode {
            order.retain(|pkg| self.graph.always_deps.contains(pkg));
        }
        order
    }

    fn print_info_dfs(&self, current_node: &PackageName, tree: &mut TreeBuilder) -> Result<()> {
        let pkg = self.package_table.get(current_node).unwrap();

        for (name, addr) in &pkg.resolved_table {
            tree.add_empty_child(format!("{}:0x{}", name, addr.short_str_lossless()));
        }

        for dep in pkg.immediate_dependencies(self) {
            tree.begin_child(dep.to_string());
            self.print_info_dfs(&dep, tree)?;
            tree.end_child();
        }

        Ok(())
    }

    pub fn print_info(&self) -> Result<()> {
        let root = self.root_package();
        let mut tree = TreeBuilder::new(root.to_string());
        self.print_info_dfs(&root, &mut tree)?;
        let tree = tree.build();
        print_tree(&tree)?;
        Ok(())
    }

    pub fn extract_named_address_mapping(
        &self,
    ) -> impl Iterator<Item = (NamedAddress, AccountAddress)> {
        self.package_table
            .get(&self.root_package())
            .expect("Failed to find root package in package table -- this should never happen")
            .resolved_table
            .clone()
            .into_iter()
    }

    pub fn file_sources(&self) -> BTreeMap<FileHash, (FileName, String)> {
        self.package_table
            .iter()
            .flat_map(|(_, rpkg)| {
                rpkg.get_sources(&self.build_options)
                    .unwrap()
                    .iter()
                    .map(|fname| {
                        let contents = fs::read_to_string(fname.as_str()).unwrap();
                        let fhash = FileHash::new(&contents);
                        (fhash, (*fname, contents))
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .collect()
    }
}

impl Package {
    fn new(package_path: PathBuf, config: &BuildConfig) -> Result<Package> {
        Ok(Package {
            source_package: parse_move_manifest_from_file(&package_path)?,
            source_digest: package_digest_for_config(&package_path, config)?,
            package_path,
            renaming: Renaming::new(),
            resolved_table: ResolvedTable::new(),
        })
    }

    fn define_addresses_in_package(&self, resolving_table: &mut ResolvingTable) -> Result<()> {
        let package = self.source_package.package.name;
        for (name, addr) in self.source_package.addresses.iter().flatten() {
            resolving_table.define((package, *name), *addr)?;
        }
        Ok(())
    }

    fn process_dependency(
        &mut self,
        dep_name: PackageName,
        dep: &DG::Dependency,
        package_table: &PackageTable,
        resolving_table: &mut ResolvingTable,
    ) -> Result<()> {
        let pkg_name = self.source_package.package.name;
        let mut dep_renaming = BTreeMap::new();

        for (to, subst) in dep.subst.iter().flatten() {
            match subst {
                SubstOrRename::Assign(addr) => {
                    resolving_table.define((pkg_name, *to), Some(*addr))?;
                }

                SubstOrRename::RenameFrom(from) => {
                    if !resolving_table.contains((dep_name, *from)) {
                        bail!(
                            "Tried to rename named address {0} from package '{1}', \
                             however {1} does not contain that address",
                            from,
                            dep_name,
                        )
                    }

                    if let Some((prev_dep, prev_from)) =
                        self.renaming.insert(*to, (dep_name, *from))
                    {
                        bail!(
                            "Duplicate renaming of named address '{to}' in dependencies of \
                             '{pkg_name}'. Substituted with '{from}' from dependency '{dep_name}' \
                             and '{prev_from}' from dependency '{prev_dep}'.",
                        )
                    }

                    dep_renaming.insert(*from, *to);
                }
            }
        }

        let bound_in_dep: Vec<_> = resolving_table
            .bindings(dep_name)
            .map(|(from, _)| from)
            .collect();

        for from in bound_in_dep {
            let to = *dep_renaming.get(&from).unwrap_or(&from);
            resolving_table.unify((pkg_name, to), (dep_name, from))?;
        }

        let Some(resolved_dep) = package_table.get(&dep_name) else {
            bail!(
                "Unable to find resolved information for dependency '{dep_name}' of \
                 '{pkg_name}'",
            );
        };

        if let Some(digest) = dep.digest {
            if digest != resolved_dep.source_digest {
                bail!(
                    "Source digest mismatch in dependency '{dep_name}' of '{pkg_name}'. \
                     Expected '{digest}' but got '{}'.",
                    resolved_dep.source_digest
                )
            }
        }

        Ok(())
    }

    fn finalize_address_resolution(&mut self, resolving_table: &ResolvingTable) -> Result<()> {
        let mut unresolved_addresses = Vec::new();

        let pkg_name = self.source_package.package.name;
        for (name, addr) in resolving_table.bindings(pkg_name) {
            match *addr {
                Some(addr) => {
                    self.resolved_table.insert(name, addr);
                }
                None => {
                    unresolved_addresses
                        .push(format!("  Named address '{name}' in package '{pkg_name}'"));
                }
            }
        }

        if !unresolved_addresses.is_empty() {
            bail!(
                "Unresolved addresses: [\n{}\n]",
                unresolved_addresses.join("\n"),
            )
        }

        Ok(())
    }

    pub fn immediate_dependencies(&self, graph: &ResolvedGraph) -> BTreeSet<PackageName> {
        graph
            .graph
            .immediate_dependencies(
                self.source_package.package.name,
                if graph.build_options.dev_mode {
                    DG::DependencyMode::DevOnly
                } else {
                    DG::DependencyMode::Always
                },
            )
            .map(|(name, _, _)| name)
            .collect()
    }

    pub fn get_sources(&self, config: &BuildConfig) -> Result<Vec<FileName>> {
        let places_to_look = source_paths_for_config(&self.package_path, config);
        Ok(find_move_filenames(&places_to_look, false)?
            .into_iter()
            .map(FileName::from)
            .collect())
    }
}

fn source_paths_for_config(package_path: &Path, config: &BuildConfig) -> Vec<PathBuf> {
    let mut places_to_look = Vec::new();
    let mut add_path = |layout_path: SourcePackageLayout| {
        let path = package_path.join(layout_path.path());
        if layout_path.is_optional() && !path.exists() {
            return;
        }
        places_to_look.push(path)
    };

    add_path(SourcePackageLayout::Sources);
    add_path(SourcePackageLayout::Scripts);

    if config.dev_mode {
        add_path(SourcePackageLayout::Examples);
        add_path(SourcePackageLayout::Tests);
    }

    places_to_look
}

fn package_digest_for_config(package_path: &Path, config: &BuildConfig) -> Result<PackageDigest> {
    let mut source_paths = source_paths_for_config(package_path, config);
    source_paths.push(package_path.join(SourcePackageLayout::Manifest.path()));
    compute_digest(&source_paths)
}
