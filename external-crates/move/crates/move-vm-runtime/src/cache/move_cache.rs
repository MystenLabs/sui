// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This is the "orchestrator" of loading a package.
// The package loader is responsible for the management of packages, package loading and caching,
// and publishing packages to the VM.

use crate::{jit, shared::types::VersionId, validation::verification};
use move_vm_config::runtime::VMConfig;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Package {
    pub verified: Arc<verification::ast::Package>,
    pub runtime: Arc<jit::execution::ast::Package>,
}

type PackageCache = HashMap<VersionId, Arc<Package>>;

/// The loader for the VM. This is the data structure is used to resolve packages and cache them
/// and their types. This is then used to create the VTables for the VM.
#[derive(Debug)]
pub struct MoveCache {
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) package_cache: Arc<RwLock<PackageCache>>,
}

#[derive(Debug)]
pub enum ResolvedPackageResult {
    /// The package was found, loaded, and cached.
    Found(Arc<Package>),
    /// The package was not found.
    NotFound,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl MoveCache {
    pub fn new(vm_config: Arc<VMConfig>) -> Self {
        Self {
            vm_config,
            package_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -------------------------------------------
    // Caching Operations
    // -------------------------------------------

    /// Add a package to the cache. If the package is already present, this is a no-op.
    ///
    /// Important: it is not an error if a package is already present when we go to insert a
    /// package. This can happen in concurrent scenarios where multiple threads attempt to load the
    /// same package at the same time -- they could both check that the package is not yet cached
    /// with `cached_package_at`, and then both proceed to independenctly load and verify the
    /// package. As the write lock is not held between the `cached_package_at` call and the call to
    /// `add_to_cache` the package could be inserted by another thread in the meantime.
    pub fn add_to_cache(
        &self,
        package_key: VersionId,
        verified: verification::ast::Package,
        runtime: jit::execution::ast::Package,
    ) {
        // NB: We grab a write lock here to ensure that we don't double-insert a package.
        let mut package_cache = self.package_cache.write();

        if package_cache.contains_key(&package_key) {
            return;
        }
        let verified = Arc::new(verified);
        let runtime = Arc::new(runtime);
        let package = Package { verified, runtime };
        package_cache.insert(package_key, Arc::new(package));
    }

    /// Get a package from the cache, if it is present.
    /// If not present, returns `None`.
    pub fn cached_package_at(&self, package_key: VersionId) -> Option<Arc<Package>> {
        self.package_cache.read().get(&package_key).map(Arc::clone)
    }

    // -------------------------------------------
    // Getters
    // -------------------------------------------

    pub fn package_cache(&self) -> &RwLock<PackageCache> {
        &self.package_cache
    }

    // -------------------------------------------
    // Cache Eviction For Testing
    // -------------------------------------------

    /// For use with unit testing: remove a package, returning `true` if it was present.
    #[cfg(test)]
    pub(crate) fn remove_package(&self, version_id: &VersionId) -> bool {
        self.package_cache.write().remove(version_id).is_some()
    }
}

impl Package {
    pub(crate) fn new(
        verified: Arc<verification::ast::Package>,
        runtime: Arc<jit::execution::ast::Package>,
    ) -> Self {
        Self { verified, runtime }
    }

    /// Used for testing that the correct number of types are loaded
    #[allow(dead_code)]
    pub(crate) fn loaded_types_len(&self) -> usize {
        self.runtime.vtable.types.len()
    }
}

// -------------------------------------------------------------------------------------------------
// Other Impls
// -------------------------------------------------------------------------------------------------

impl Clone for MoveCache {
    /// Makes a shallow copy of the VM Cache by cloning all the internal `Arc`s.
    fn clone(&self) -> Self {
        let MoveCache {
            vm_config,
            package_cache,
        } = self;
        Self {
            vm_config: vm_config.clone(),
            package_cache: package_cache.clone(),
        }
    }
}
