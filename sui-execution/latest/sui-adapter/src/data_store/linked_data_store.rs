// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::linkage::resolved_linkage::RootedLinkage,
};
use move_core_types::{
    account_address::AccountAddress,
    resolver::{ModuleResolver, SerializedPackage},
};
use move_vm_runtime::shared::types::VersionId;
use sui_types::error::{SuiError, SuiResult};

/// A `LinkedDataStore` is a wrapper around a `PackageStore` (i.e., a package store where
/// we can also resolve types to defining IDs) along with a specific `linkage`. These two together
/// allow us to resolve modules and types in a way that is consistent with the `linkage` provided
/// and allow us to then pass this into the VM. Until we have a linkage set it is not possible to
/// construct a valid `DataStore` for execution in the VM as it needs to be able to resolve modules
/// under a specific linkage.
pub struct LinkedDataStore<'a> {
    pub linkage: &'a RootedLinkage,
    pub store: &'a dyn PackageStore,
}

impl<'a> LinkedDataStore<'a> {
    pub fn new(linkage: &'a RootedLinkage, store: &'a dyn PackageStore) -> Self {
        Self { linkage, store }
    }

    fn fetch_package(&self, package_version_id: VersionId) -> SuiResult<Option<SerializedPackage>> {
        Ok(self
            .store
            .get_package(&package_version_id.into())?
            .map(|pkg| pkg.into_serialized_move_package()))
    }
}

// Better days have arrived!
impl ModuleResolver for LinkedDataStore<'_> {
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
