// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data_store::PackageStore;
use indexmap::IndexMap;
use move_core_types::identifier::IdentStr;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, SuiResult},
    move_package::MovePackage,
    storage::BackingPackageStore,
};

/// A package store that caches packages in memory and indexes type origins. This is useful for
/// speeding up package loading and type resolution.
///
/// There is a `max_package_cache_size` that determines how many packages we will cache. If the cache is
/// full, we will drop the cache.
///
/// The `max_type_cache_size` determines how many types we will cache. If the cache is full, we
/// will drop the cache.
///
/// FUTURE: Most/all of this will be replaced by the VM runtime cache in the new VM. For now
/// though, we need to use this.
pub struct CachedPackageStore<'state> {
    /// Underlying store the fetch packages from
    pub package_store: Box<dyn BackingPackageStore + 'state>,
    /// A cache of packages that we've loaded so far. This is used to speed up package loading.
    /// Elements in this are safe to be evicted based on cache decisions.
    package_cache: RefCell<BTreeMap<ObjectID, Option<Rc<MovePackage>>>>,
    /// A cache of type origins that we've loaded so far. This is used to speed up type resolution.
    /// Elements in this are safe to be evicted based on cache decisions.
    type_origin_cache: RefCell<CachedTypeOriginMap>,
    /// Elements in this are packages that are being published or have been published in the current
    /// transaction.
    /// Elements in this are _not_ safe to evict, unless evicted through `pop_package`.
    new_packages: RefCell<IndexMap<ObjectID, Rc<MovePackage>>>,
    /// Maximum size (in number of packages) that the `package_cache` can grow to before it is
    /// cleared.
    max_package_cache_size: usize,
    /// Maximum size (in number of keys) that the `type_origin_cache` can grow to before it is
    /// cleared.,
    max_type_cache_size: usize,
}

type TypeOriginMap = BTreeMap<ObjectID, BTreeMap<(String, String), ObjectID>>;

#[derive(Debug)]
struct CachedTypeOriginMap {
    /// Tracker of all packages that we've loaded so far. This is used to determine if the
    /// `TypeOriginMap` needs to be updated when loading a package, or if that package has already
    /// contributed to the `TypeOriginMap`.
    pub cached_type_origins: BTreeSet<ObjectID>,
    /// A mapping of the (original package ID)::<module_name>::<type_name> to the defining ID for
    /// that type.
    pub type_origin_map: TypeOriginMap,
}

impl CachedTypeOriginMap {
    pub fn new() -> Self {
        Self {
            cached_type_origins: BTreeSet::new(),
            type_origin_map: TypeOriginMap::new(),
        }
    }
}

impl<'state> CachedPackageStore<'state> {
    pub const DEFAULT_MAX_PACKAGE_CACHE_SIZE: usize = 200;
    pub const DEFAULT_MAX_TYPE_ORIGIN_CACHE_SIZE: usize = 1000;

    pub fn new(package_store: Box<dyn BackingPackageStore + 'state>) -> Self {
        Self {
            package_store,
            package_cache: RefCell::new(BTreeMap::new()),
            type_origin_cache: RefCell::new(CachedTypeOriginMap::new()),
            new_packages: RefCell::new(IndexMap::new()),
            max_package_cache_size: Self::DEFAULT_MAX_PACKAGE_CACHE_SIZE,
            max_type_cache_size: Self::DEFAULT_MAX_TYPE_ORIGIN_CACHE_SIZE,
        }
    }

    /// Push a new package into the new packages. This is used to track packages that are being
    /// published or have been published.
    pub fn push_package(
        &self,
        id: ObjectID,
        package: Rc<MovePackage>,
    ) -> Result<(), ExecutionError> {
        // Check that the package ID is not already present anywhere.
        debug_assert!(self.fetch_package(&id).unwrap().is_none());

        // Insert the package into the new packages
        // If the package already exists, we will overwrite it and signal an error.
        if self.new_packages.borrow_mut().insert(id, package).is_some() {
            invariant_violation!(
                "Package with ID {} already exists in the new packages. This should never happen.",
                id
            );
        }

        Ok(())
    }

    /// Rollback a package that was pushed into the new packages. We keep the invariant that:
    /// * You can only pop the most recent package that was pushed.
    /// * The element being popped _must_ exist in the new packages.
    ///
    /// Otherwise this returns an invariant violation.
    pub fn pop_package(&self, id: ObjectID) -> Result<Rc<MovePackage>, ExecutionError> {
        if self
            .new_packages
            .borrow()
            .last()
            .is_none_or(|(pkg_id, _)| *pkg_id != id)
        {
            make_invariant_violation!(
                "Tried to pop package {} from new packages, but new packages was empty or \
                it is not the most recent package inserted. This should never happen.",
                id
            );
        }

        let Some((pkg_id, pkg)) = self.new_packages.borrow_mut().pop() else {
            unreachable!(
                "We just checked that new packages is not empty, so this should never happen."
            );
        };
        assert_eq!(
            pkg_id, id,
            "Popped package ID {} does not match requested ID {}. This should never happen as was checked above.",
            pkg_id, id
        );

        Ok(pkg)
    }

    pub fn to_new_packages(&self) -> Vec<MovePackage> {
        self.new_packages
            .borrow()
            .iter()
            .map(|(_, pkg)| pkg.as_ref().clone())
            .collect()
    }

    /// Get a package by its package ID (i.e., not original ID). This will first look in the new
    /// packages, then in the cache, and then finally try and fetch the pacakge from the underlying
    /// package store.
    ///
    /// Once the package is fetched it will be added to the type origin cache if it is not already
    /// present in the type origin cache.
    pub fn get_package(&self, object_id: &ObjectID) -> SuiResult<Option<Rc<MovePackage>>> {
        let Some(pkg) = self.fetch_package(object_id)? else {
            return Ok(None);
        };

        let package_id = pkg.id();

        // If the number of type origins that we have cached exceeds the max size, drop the cache.
        if self.type_origin_cache.borrow().cached_type_origins.len() >= self.max_type_cache_size {
            *self.type_origin_cache.borrow_mut() = CachedTypeOriginMap::new();
        }

        if !self
            .type_origin_cache
            .borrow()
            .cached_type_origins
            .contains(&package_id)
        {
            let cached_type_origin_map = &mut self.type_origin_cache.borrow_mut();
            cached_type_origin_map
                .cached_type_origins
                .insert(package_id);
            let original_package_id = pkg.original_package_id();
            let package_types = cached_type_origin_map
                .type_origin_map
                .entry(original_package_id)
                .or_default();
            for ((module_name, type_name), defining_id) in pkg.type_origin_map().into_iter() {
                if let Some(other) = package_types.insert(
                    (module_name.to_string(), type_name.to_string()),
                    defining_id,
                ) {
                    assert_eq!(
                        other, defining_id,
                        "type origin map should never have conflicting entries"
                    );
                }
            }
        }
        Ok(Some(pkg))
    }

    /// Get a package by its package ID (i.e., not original ID). This will first look in the new
    /// packages, then in the cache, and then finally try and fetch the pacakge from the underlying
    /// package store. NB: this does not do any indexing of the package.
    fn fetch_package(&self, id: &ObjectID) -> SuiResult<Option<Rc<MovePackage>>> {
        // Look for package in new packages
        if let Some(pkg) = self.new_packages.borrow().get(id).cloned() {
            return Ok(Some(pkg));
        }

        // Look for package in cache
        if let Some(pkg) = self.package_cache.borrow().get(id).cloned() {
            return Ok(pkg);
        }

        if self.package_cache.borrow().len() >= self.max_package_cache_size {
            self.package_cache.borrow_mut().clear();
        }

        let pkg = self
            .package_store
            .get_package_object(id)?
            .map(|pkg_obj| Rc::new(pkg_obj.move_package().clone()));
        self.package_cache.borrow_mut().insert(*id, pkg.clone());
        Ok(pkg)
    }
}

impl PackageStore for CachedPackageStore<'_> {
    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Rc<MovePackage>>> {
        self.get_package(id)
    }

    fn resolve_type_to_defining_id(
        &self,
        module_address: ObjectID,
        module_name: &IdentStr,
        type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>> {
        let Some(pkg) = self.get_package(&module_address)? else {
            return Ok(None);
        };

        Ok(self
            .type_origin_cache
            .borrow()
            .type_origin_map
            .get(&pkg.original_package_id())
            .and_then(|module_map| {
                module_map
                    .get(&(module_name.to_string(), type_name.to_string()))
                    .copied()
            }))
    }
}
