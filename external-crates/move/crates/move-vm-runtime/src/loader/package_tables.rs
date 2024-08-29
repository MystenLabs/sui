// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This module is responsible for the building of the package VTables given a root package storage
// ID. The VTables are built by loading all the packages that are dependencies of the root package,
// and once they are loaded creating the VTables for each package, and populating the
// `loaded_packages` table (keyed by the _runtime_ package ID!) with the VTables for each package
// in the transitive closure of the root package.

use super::{
    arena::ArenaPointer,
    ast::Function,
    linkage_checker,
    package_cache::{LoadedPackage, RuntimePackageId, VTableKey},
    package_loader::PackageLoader,
    type_cache::TypeCache,
};
use move_binary_format::errors::{PartialVMError, PartialVMResult, VMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_types::data_store::DataStore;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

/// The data structure that the VM uses to resolve all packages. Packages are loaded into this at
/// before the beginning of execution, and based on the static call graph of the package that
/// contains the root package id.
///
/// This is a transient (transaction-scoped) data structure that is created at the beginning of the
/// transaction, is immutable for the execution of the transaction, and is dropped at the end of
/// the transaction.
///
/// TODO(tzakian): The representation can be optimized to use a more efficient data structure for
/// vtable/cross-package function resolution but we will keep it simple for now.
pub struct PackageTables<'a> {
    loaded_packages: HashMap<RuntimePackageId, Arc<LoadedPackage>>,
    cached_types: &'a RwLock<TypeCache>,
}

/// The VM API that it will use to resolve packages and functions during execution of the
/// transaction.
impl<'a> PackageTables<'a> {
    /// Given a root package id, a type cache, and a data store, this function creates a new map of
    /// loaded packages that consist of the root package and all of its dependencies as specified
    /// by the root package.
    ///
    /// The resuling map of vtables _must_ be closed under the static dependency graph of the root
    /// package w.r.t, to the current linkage context in `data_store`.
    pub fn new(data_store: &impl DataStore, package_runtime: &'a PackageLoader) -> VMResult<Self> {
        let mut loaded_packages = HashMap::new();

        // Make sure the root package and all of its dependencies (under the current linkage
        // context) are loaded.
        let cached_packages = package_runtime.load_and_cache_link_context(data_store)?;

        // Verify that the linkage and cyclic checks pass for all packages under the current
        // linkage context.
        linkage_checker::verify_linkage_and_cyclic_checks(&cached_packages)?;
        cached_packages.into_iter().for_each(|(_, p)| {
            loaded_packages.insert(p.runtime_id, p);
        });

        Ok(Self {
            loaded_packages,
            cached_types: &package_runtime.type_cache,
        })
    }
    pub fn get_package(&self, id: &RuntimePackageId) -> PartialVMResult<Arc<LoadedPackage>> {
        self.loaded_packages.get(id).cloned().ok_or_else(|| {
            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                .with_message(format!("Package {} not found", id))
        })
    }

    pub fn resolve_function(
        &self,
        vtable_key: &VTableKey,
    ) -> PartialVMResult<&ArenaPointer<Function>> {
        self.loaded_packages
            .get(&vtable_key.package_key)
            .map(|pkg| &pkg.vtable)
            .and_then(|vtable| {
                vtable.get(&(
                    vtable_key.module_name.to_owned(),
                    vtable_key.function_name.to_owned(),
                ))
            })
            .map(|f| f.as_ref())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY).with_message(format!(
                    "Function {}::{} not found in package {}",
                    vtable_key.module_name, vtable_key.function_name, vtable_key.package_key
                ))
            })
    }

    pub fn type_cache(&self) -> &'a RwLock<TypeCache> {
        &self.cached_types
    }
}
