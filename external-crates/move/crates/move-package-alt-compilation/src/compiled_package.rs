// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_package_alt::{graph::NamedAddress, schema::PackageName};

use crate::build_config::BuildConfig;

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_bytecode_utils::Modules;
use move_compiler::{compiled_unit::CompiledUnit, shared::files::MappedFiles};
use move_core_types::{account_address::AccountAddress, parsing::address::NumericalAddress};
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

#[derive(Clone, Debug)]
pub struct BuildNamedAddresses {
    pub inner: BTreeMap<Symbol, NumericalAddress>,
}

/// Represents a compiled package in memory.
#[derive(Clone, Debug)]
pub struct CompiledPackage {
    /// Meta information about the compilation of this `CompiledPackage`
    pub compiled_package_info: CompiledPackageInfo,
    /// The output compiled bytecode in the root package (both module, and scripts) along with its
    /// source file
    pub root_compiled_units: Vec<CompiledUnitWithSource>,
    /// The output compiled bytecode for dependencies
    pub deps_compiled_units: Vec<(Symbol, CompiledUnitWithSource)>,

    // Optional artifacts from compilation
    /// filename -> doctext
    pub compiled_docs: Option<Vec<(String, String)>>,
    /// The mapping of file hashes to file names and contents
    pub file_map: MappedFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledPackageInfo {
    /// The name of the compiled package
    pub package_name: Symbol,
    /// The instantiations for all named addresses that were used for compilation
    // pub address_alias_instantiation: BTreeMap<String, String>,
    /// The hash of the source directory at the time of compilation. `None` if the source for this
    /// package is not available/this package was not compiled.
    // pub source_digest: Option<String>,
    /// The build flags that were used when compiling this package.
    pub build_flags: BuildConfig,
}

#[derive(Debug, Clone)]
pub struct CompiledUnitWithSource {
    pub unit: CompiledUnit,
    pub source_path: PathBuf,
}

impl CompiledPackage {
    /// Return an iterator over all compiled units in this package, including dependencies
    pub fn all_compiled_units_with_source(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units
            .iter()
            .chain(self.deps_compiled_units.iter().map(|(_, unit)| unit))
    }

    /// Returns all compiled units for this package in transitive dependencies. Order is not
    /// guaranteed.
    pub fn all_compiled_units(&self) -> impl Iterator<Item = &CompiledUnit> {
        self.all_compiled_units_with_source().map(|unit| &unit.unit)
    }

    /// Returns compiled modules for this package and its transitive dependencies
    pub fn all_modules_map(&self) -> Modules<'_> {
        Modules::new(self.all_compiled_units().map(|unit| &unit.module))
    }

    /// `root_compiled_units` filtered over `CompiledUnit::Module`
    pub fn root_modules(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units.iter()
    }

    /// Return an iterator over all bytecode modules in this package, including dependencies
    pub fn get_modules_and_deps(&self) -> impl Iterator<Item = &CompiledModule> {
        self.all_compiled_units_with_source()
            .map(|m| &m.unit.module)
    }

    /// Return an iterator over the root bytecode modules in this package, excluding dependencies
    pub fn root_modules_map(&self) -> Modules<'_> {
        Modules::new(
            self.root_compiled_units
                .iter()
                .map(|unit| &unit.unit.module),
        )
    }

    /// Return the bytecode modules in this package, topologically sorted in dependency order.
    /// This is the function to call if you would like to publish or statically analyze the modules.
    pub fn get_dependency_sorted_modules(
        &self,
        with_unpublished_deps: bool,
    ) -> Vec<CompiledModule> {
        let all_modules = Modules::new(self.get_modules_and_deps());

        // SAFETY: package built successfully
        let modules = all_modules.compute_topological_order().unwrap();

        if with_unpublished_deps {
            // For each transitive dependent module, if they are not to be published, they must have
            // a non-zero address (meaning they are already published on-chain).
            modules
                .filter(|module| module.address() == &AccountAddress::ZERO)
                .cloned()
                .collect()
        } else {
            // Collect all module IDs from the current package to be published (module names are not
            // sufficient as we may have modules with the same names in user code and in Sui
            // framework which would result in the latter being pulled into a set of modules to be
            // published).
            let self_modules: HashSet<_> = self
                .root_modules_map()
                .iter_modules()
                .iter()
                .map(|m| m.self_id())
                .collect();

            modules
                .filter(|module| self_modules.contains(&module.self_id()))
                .cloned()
                .collect()
        }
    }

    /// Return a serialized representation of the bytecode modules in this package, topologically
    /// sorted in dependency order.
    pub fn get_package_bytes(&self, with_unpublished_deps: bool) -> Vec<Vec<u8>> {
        self.get_dependency_sorted_modules(with_unpublished_deps)
            .iter()
            .map(|m| {
                let mut bytes = Vec::new();
                m.serialize_with_version(m.version, &mut bytes).unwrap(); // safe because package built successfully
                bytes
            })
            .collect()
    }

    pub fn get_module_by_name(
        &self,
        package_name: &str,
        module_name: &str,
    ) -> Result<&CompiledUnitWithSource> {
        if self.compiled_package_info.package_name.as_str() == package_name {
            return self.get_module_by_name_from_root(module_name);
        }

        self.deps_compiled_units
            .iter()
            .filter(|(dep_package, _)| dep_package.as_str() == package_name)
            .map(|(_, unit)| unit)
            .find(|unit| unit.unit.name().as_str() == module_name)
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Unable to find module with name '{}' in package {}",
                    module_name,
                    self.compiled_package_info.package_name
                )
            })
    }

    pub fn get_module_by_name_from_root(
        &self,
        module_name: &str,
    ) -> Result<&CompiledUnitWithSource> {
        self.root_modules()
            .find(|unit| unit.unit.name().as_str() == module_name)
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Unable to find module with name '{}' in package {}",
                    module_name,
                    self.compiled_package_info.package_name
                )
            })
    }
}

impl BuildNamedAddresses {
    /// For "publish"/"upgrade" operations, we want root to always be `0x0`.
    pub fn root_as_zero(value: BTreeMap<PackageName, NamedAddress>) -> Self {
        Self {
            inner: format_named_addresses(value, true /* root as zero */),
        }
    }
}

impl From<BTreeMap<PackageName, NamedAddress>> for BuildNamedAddresses {
    fn from(value: BTreeMap<PackageName, NamedAddress>) -> Self {
        Self {
            inner: format_named_addresses(value, false /* root as zero */),
        }
    }
}

impl From<BuildNamedAddresses> for BTreeMap<Symbol, AccountAddress> {
    fn from(val: BuildNamedAddresses) -> Self {
        val.inner
            .into_iter()
            .map(|(pkg, address)| (pkg, address.into_inner()))
            .collect()
    }
}

fn format_named_addresses(
    value: BTreeMap<PackageName, NamedAddress>,
    root_as_zero: bool,
) -> BTreeMap<Symbol, NumericalAddress> {
    let mut addresses: BTreeMap<Symbol, NumericalAddress> = BTreeMap::new();
    for (dep_name, dep) in value {
        let name = dep_name.as_str().into();

        let addr = match dep {
            NamedAddress::RootPackage(Some(addr)) => {
                if root_as_zero {
                    AccountAddress::ZERO
                } else {
                    addr.0
                }
            }
            NamedAddress::RootPackage(None) => AccountAddress::ZERO,
            NamedAddress::Unpublished { dummy_addr } => dummy_addr.0,
            NamedAddress::Defined(original_id) => original_id.0,
        };

        let addr: NumericalAddress =
            NumericalAddress::new(addr.into_bytes(), move_compiler::shared::NumberFormat::Hex);
        addresses.insert(name, addr);
    }

    addresses
}
