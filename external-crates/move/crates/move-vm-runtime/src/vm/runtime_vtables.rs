// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This module is responsible for the building of the package VTables given a root package storage
// ID. The VTables are built by loading all the packages that are dependencies of the root package,
// and once they are loaded creating the VTables for each package, and populating the
// `loaded_packages` table (keyed by the _runtime_ package ID!) with the VTables for each package
// in the transitive closure of the root package.

use crate::{
    cache::{arena::ArenaPointer, type_cache::TypeCache},
    jit::runtime::ast::{Function, Module, Package, VTableKey},
    on_chain::ast::RuntimePackageId,
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult, VMResult},
    CompiledModule,
};
use move_core_types::{language_storage::ModuleId, vm_status::StatusCode};

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
#[derive(Clone)]
pub struct RuntimeVTables {
    pub(crate) loaded_packages: HashMap<RuntimePackageId, Arc<Package>>,
    cached_types: Arc<RwLock<TypeCache>>,
}

/// The VM API that it will use to resolve packages and functions during execution of the
/// transaction.
impl RuntimeVTables {
    /// Create a new RuntimeVTables instance.
    /// NOTE: This assumes linkage has already occured.
    pub fn new(
        loaded_packages: HashMap<RuntimePackageId, Arc<Package>>,
        cached_types: Arc<RwLock<TypeCache>>,
    ) -> VMResult<Self> {
        Ok(Self {
            loaded_packages,
            cached_types,
        })
    }

    pub fn get_package(&self, id: &RuntimePackageId) -> PartialVMResult<Arc<Package>> {
        self.loaded_packages.get(id).cloned().ok_or_else(|| {
            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                .with_message(format!("Package {} not found", id))
        })
    }

    pub fn resolve_compiled_module(
        &self,
        runtime_id: &ModuleId,
    ) -> PartialVMResult<Arc<CompiledModule>> {
        let (package, module_id) = runtime_id.into();
        let package = self.loaded_packages.get(package).ok_or_else(|| {
            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                .with_message(format!("Package {} not found", package))
        })?;
        package
            .compiled_modules
            .get(module_id)
            .map(|value| value.clone())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                    .with_message(format!("Module {} not found", module_id))
            })
    }

    pub fn resolve_loaded_module(&self, runtime_id: &ModuleId) -> PartialVMResult<Arc<Module>> {
        let (package, module_id) = runtime_id.into();
        let package = self.loaded_packages.get(package).ok_or_else(|| {
            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                .with_message(format!("Package {} not found", package))
        })?;
        package
            .loaded_modules
            .get(module_id)
            .map(|value| value.clone())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                    .with_message(format!("Module {} not found", module_id))
            })
    }

    pub fn resolve_function(
        &self,
        vtable_key: &VTableKey,
    ) -> PartialVMResult<ArenaPointer<Function>> {
        self.loaded_packages
            .get(&vtable_key.package_key)
            .map(|pkg| &pkg.vtable)
            .and_then(|vtable| {
                vtable.get(&(
                    vtable_key.module_name.to_owned(),
                    vtable_key.function_name.to_owned(),
                ))
            })
            .map(|f| f.as_ref().clone())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY).with_message(format!(
                    "Function {}::{} not found in package {}",
                    vtable_key.module_name, vtable_key.function_name, vtable_key.package_key
                ))
            })
    }

    pub fn type_cache(&self) -> Arc<RwLock<TypeCache>> {
        self.cached_types.clone()
    }
}
