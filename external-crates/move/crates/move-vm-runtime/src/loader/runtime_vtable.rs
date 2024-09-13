// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This module is responsible for the building of the package VTables given a root package storage
// ID. The VTables are built by loading all the packages that are dependencies of the root package,
// and once they are loaded creating the VTables for each package, and populating the
// `loaded_packages` table (keyed by the _runtime_ package ID!) with the VTables for each package
// in the transitive closure of the root package.

use crate::loader::{
    arena::ArenaPointer,
    ast::{Function, LoadedPackage, RuntimePackageId, VTableKey},
    type_cache::TypeCache,
};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

/// The data structure that the VM uses to resolve all packages. Packages are loaded into this at
/// before the beginning of execution, based on the static call graph of the root package (that
/// is, contains the root package id).
///
/// This is a transient (transaction-scoped) data structure that is created at the beginning of the
/// transaction, is immutable for the execution of the transaction, and is dropped at the end of
/// the transaction.
///
/// TODO(tzakian): The representation can be optimized to use a more efficient data structure for
/// vtable/cross-package function resolution but we will keep it simple for now.
pub struct RuntimeVTables<'a> {
    pub(crate) loaded_packages: HashMap<RuntimePackageId, Arc<LoadedPackage>>,
    pub(crate) cached_types: &'a RwLock<TypeCache>,
}

/// The VM API that it will use to resolve packages and functions during execution of the
/// transaction.
impl<'a> RuntimeVTables<'a> {
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
        self.cached_types
    }
}
