// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This is the "orchestrator" of loading a package.
// The package loader is responsible for the management of packages, package loading and caching,
// and publishing packages to the VM.

use crate::{
    cache::type_cache::TypeCache, jit, natives::functions::NativeFunctions,
    shared::types::PackageStorageId, validation::verification,
};
use move_vm_config::runtime::VMConfig;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Package {
    pub verified: Arc<verification::ast::Package>,
    pub runtime: Arc<jit::runtime::ast::Package>,
}

type PackageCache = HashMap<PackageStorageId, Arc<Package>>;

/// A Loaded Function for driving VM Calls
// #[allow(dead_code)]
// pub struct LoadedFunction {
//     compiled_module: Arc<CompiledModule>,
//     loaded_module: Arc<Module>,
//     function: ArenaPointer<Function>,
//     /// Parameters for the function call
//     pub parameters: Vec<Type>,
//     /// Function return type
//     pub return_: Vec<Type>,
// }

/// The loader for the VM. This is the data structure is used to resolve packages and cache them
/// and their types. This is then used to create the VTables for the VM.
#[derive(Debug)]
pub struct MoveCache {
    pub(crate) natives: Arc<NativeFunctions>,
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) type_cache: Arc<RwLock<TypeCache>>,
    pub(crate) package_cache: Arc<RwLock<PackageCache>>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl MoveCache {
    pub fn new(natives: Arc<NativeFunctions>, vm_config: Arc<VMConfig>) -> Self {
        Self {
            natives,
            vm_config,
            package_cache: Arc::new(RwLock::new(HashMap::new())),
            type_cache: Arc::new(RwLock::new(TypeCache::new())),
        }
    }

    // -------------------------------------------
    // Caching Operations
    // -------------------------------------------

    // TODO: Make this a VM Result
    pub fn add_to_cache(
        &self,
        package_key: PackageStorageId,
        verified: verification::ast::Package,
        runtime: jit::runtime::ast::Package,
    ) {
        assert!(!self.package_cache.read().contains_key(&package_key));
        let verified = Arc::new(verified);
        let runtime = Arc::new(runtime);
        let package = Package { verified, runtime };
        self.package_cache()
            .write()
            .insert(package_key, Arc::new(package));
    }

    pub fn cached_package_at(&self, package_key: PackageStorageId) -> Option<Arc<Package>> {
        self.package_cache.read().get(&package_key).map(Arc::clone)
    }

    // -------------------------------------------
    // Getters
    // -------------------------------------------

    pub fn type_cache(&self) -> &RwLock<TypeCache> {
        &self.type_cache
    }

    pub fn package_cache(&self) -> &RwLock<PackageCache> {
        &self.package_cache
    }
}

// -------------------------------------------------------------------------------------------------
// Other Impls
// -------------------------------------------------------------------------------------------------

impl Clone for MoveCache {
    /// Makes a shallow copy of the VM Cache by cloning all the internal `Arc`s.
    fn clone(&self) -> Self {
        let MoveCache {
            natives,
            vm_config,
            type_cache,
            package_cache,
        } = self;
        Self {
            natives: natives.clone(),
            vm_config: vm_config.clone(),
            type_cache: type_cache.clone(),
            package_cache: package_cache.clone(),
        }
    }
}
