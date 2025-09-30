// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use move_core_types::{
    account_address::AccountAddress,
    resolver::{ModuleResolver, SerializedPackage},
};
use move_vm_runtime::{
    shared::types::VersionId, validation::verification::ast::Package as VerifiedPackage,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, SuiError, SuiResult},
    move_package::MovePackage,
    storage::BackingPackageStore,
};

/// A `TransactionPackageStore` is a `ModuleResolver` that fetches packages from a backing store.
/// It also tracks packages that are being published in the current transaction and allows
/// "loading" of those packages as well.
///
/// It is used to provide package loading (from storage) for the Move VM.
#[allow(clippy::type_complexity)]
pub struct TransactionPackageStore<'a> {
    package_store: &'a dyn BackingPackageStore,

    /// Elements in this are packages that are being published or have been published in the current
    /// transaction.
    /// Elements in this are _not_ safe to evict, unless evicted through `pop_package`.
    new_packages: RefCell<IndexMap<ObjectID, (Rc<MovePackage>, Arc<VerifiedPackage>)>>,
}

impl<'a> TransactionPackageStore<'a> {
    pub fn new(package_store: &'a dyn BackingPackageStore) -> Self {
        Self {
            package_store,
            new_packages: RefCell::new(IndexMap::new()),
        }
    }

    /// Push a new package into the new packages. This is used to track packages that are being
    /// published or have been published.
    pub fn push_package(
        &self,
        id: ObjectID,
        package: Rc<MovePackage>,
        verified_package: VerifiedPackage,
    ) -> Result<(), ExecutionError> {
        // Check that the package ID is not already present anywhere.
        debug_assert!(!self.new_packages.borrow().contains_key(&id));

        // Insert the package into the new packages
        // If the package already exists, we will overwrite it and signal an error.
        if self
            .new_packages
            .borrow_mut()
            .insert(id, (package, Arc::new(verified_package)))
            .is_some()
        {
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
    pub fn pop_package(&self, id: ObjectID) -> Result<Arc<VerifiedPackage>, ExecutionError> {
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

        let Some((pkg_id, (_move_pkg, verified_pkg))) = self.new_packages.borrow_mut().pop() else {
            unreachable!(
                "We just checked that new packages is not empty, so this should never happen."
            );
        };
        assert_eq!(
            pkg_id, id,
            "Popped package ID {} does not match requested ID {}. This should never happen as was checked above.",
            pkg_id, id
        );

        Ok(verified_pkg)
    }

    /// Fetch a package that is being published in the current transaction, if it exists.
    /// This does not look in the backing store.
    pub fn fetch_new_package(
        &self,
        id: &ObjectID,
    ) -> Option<(Rc<MovePackage>, Arc<VerifiedPackage>)> {
        self.new_packages.borrow().get(id).cloned()
    }

    /// Return all new packages that have been added to this store in the transaction.
    pub fn to_new_packages(&self) -> Vec<MovePackage> {
        self.new_packages
            .borrow()
            .iter()
            .map(|(_, (move_pkg, _))| move_pkg.as_ref().clone())
            .collect()
    }

    /// Fetch a package by its version ID. This will first look in the new packages, and then in
    /// the backing store.
    /// If found, it will be returned as a SerializedPackage.
    fn fetch_package(&self, package_version_id: VersionId) -> SuiResult<Option<SerializedPackage>> {
        if let Some((move_pkg, _verified_pkg)) = self.fetch_new_package(&package_version_id.into())
        {
            return Ok(Some(move_pkg.into_serialized_move_package()));
        }

        Ok(self
            .package_store
            .get_package_object(&package_version_id.into())?
            .map(|pkg| pkg.move_package().into_serialized_move_package()))
    }
}

// Better days have arrived!
impl ModuleResolver for TransactionPackageStore<'_> {
    type Error = SuiError;
    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> Result<[Option<SerializedPackage>; N], Self::Error> {
        // Once https://doc.rust-lang.org/stable/std/primitive.array.html#method.try_map is stable
        // we can use that here.
        let mut packages = [const { None }; N];
        for (i, id) in ids.iter().enumerate() {
            packages[i] = self.fetch_package(*id)?;
        }

        Ok(packages)
    }

    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        ids.iter().map(|id| self.fetch_package(*id)).collect()
    }
}
