// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::shared::{
    data_store::DataStore,
    types::{OriginalId, VersionId},
};
use move_binary_format::errors::*;
use move_core_types::{
    account_address::AccountAddress,
    effects::{AccountChangeSet, ChangeSet, Op},
    identifier::Identifier,
    resolver::{ModuleResolver, SerializedPackage},
    vm_status::StatusCode,
};
use std::collections::{btree_map::BTreeMap, BTreeSet};

pub struct AccountDataCache {
    original_id: OriginalId,
    module_map: BTreeMap<Identifier, Vec<u8>>,
}

impl AccountDataCache {
    fn new(original_id: OriginalId) -> Self {
        Self {
            original_id,
            module_map: BTreeMap::new(),
        }
    }
}

/// Transaction data cache. Keep updates within a transaction so they can all be published at
/// once when the transaction succeeds.
///
/// The Move VM takes a `DataStore` in input and this is the default and correct implementation
/// for a data store related to a transaction. Clients should create an instance of this type
/// and pass it to the Move VM.
pub struct TransactionDataCache<S> {
    pub remote: S,
    module_map: BTreeMap<AccountAddress, AccountDataCache>,
}

impl<S: ModuleResolver> TransactionDataCache<S> {
    /// Create a `TransactionDataCache` with a `RemoteCache` that provides access to data
    /// not updated in the transaction.
    pub fn new(remote: S) -> Self {
        TransactionDataCache {
            remote,
            module_map: BTreeMap::new(),
        }
    }

    pub fn into_effects(mut self) -> (PartialVMResult<ChangeSet>, S) {
        (self.impl_into_effects(), self.remote)
    }

    fn impl_into_effects(&mut self) -> PartialVMResult<ChangeSet> {
        let mut change_set = ChangeSet::new();
        for (addr, account_data_cache) in std::mem::take(&mut self.module_map).into_iter() {
            let mut modules = BTreeMap::new();
            for (module_name, module_blob) in account_data_cache.module_map {
                modules.insert(module_name, Op::New(module_blob));
            }

            if !modules.is_empty() {
                change_set
                    .add_account_changeset(
                        addr,
                        AccountChangeSet::from_modules(account_data_cache.original_id, modules),
                    )
                    .expect("accounts should be unique");
            }
        }

        Ok(change_set)
    }

    pub fn get_remote_resolver(&self) -> &S {
        &self.remote
    }

    pub fn get_remote_resolver_mut(&mut self) -> &mut S {
        &mut self.remote
    }

    pub fn publish_package(
        &mut self,
        original_id: OriginalId,
        version_id: VersionId,
        modules: impl IntoIterator<Item = (Identifier, Vec<u8>)>,
    ) {
        let account_cache = self
            .module_map
            .entry(version_id)
            .or_insert_with(|| AccountDataCache::new(original_id));

        for (module_name, blob) in modules.into_iter() {
            account_cache.module_map.insert(module_name, blob);
        }
    }

    pub fn into_remote(self) -> S {
        let TransactionDataCache {
            remote,
            module_map: _,
        } = self;
        remote
    }
}

// `DataStore` implementation for the `TransactionDataCache`
impl<S: ModuleResolver> DataStore for TransactionDataCache<S> {
    fn load_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> VMResult<[SerializedPackage; N]> {
        // Once https://doc.rust-lang.org/stable/std/primitive.array.html#method.try_map is stable
        // we can use that here.
        // TODO: We can optimize this to take advantage of bulk-get a bit more if we desire.
        // However it's unlikely to be a bottleneck.
        let mut packages = ids.map(|id| SerializedPackage::empty(id, id));
        for package in packages.iter_mut() {
            let Some(account_cache) = self.module_map.get(&package.storage_id) else {
                return Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                    .with_message(format!(
                        "Cannot find package {:?} in data cache",
                        package.storage_id
                    ))
                    .finish(Location::Undefined));
            };
            // TODO(vm-rewrite): Update this to include linkage info and type origins
            package.modules = account_cache.module_map.clone();
            package.runtime_id = account_cache.original_id;
        }
        Ok(packages)
    }

    fn load_packages(&self, ids: &[AccountAddress]) -> VMResult<Vec<SerializedPackage>> {
        let mut cached = BTreeSet::new();
        // Fetch all packages that we have locally
        let mut cached_packages = ids
            .iter()
            .enumerate()
            .filter_map(|(idx, package_id)| {
                self.module_map.get(package_id).map(|account_cache| {
                    cached.insert(idx);
                    SerializedPackage::raw_package(
                        account_cache.module_map.clone(),
                        account_cache.original_id,
                        *package_id,
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter();
        let to_fetch_packages: Vec<_> = ids
            .iter()
            .enumerate()
            .filter(|(idx, _)| !cached.contains(idx))
            .map(|(_, package_id)| *package_id)
            .collect();

        // fetch all of the remaining packages from the remote
        let mut fetched_packages = match self.remote.get_packages(&to_fetch_packages) {
            Ok(pkgs) => pkgs
                .into_iter()
                .enumerate()
                .map(|(idx, pkg)| {
                    pkg.ok_or_else(|| {
                        PartialVMError::new(StatusCode::LINKER_ERROR)
                            .with_message(format!(
                                "Cannot find package {:?} in data cache",
                                to_fetch_packages[idx],
                            ))
                            .finish(Location::Undefined)
                    })
                })
                .collect::<VMResult<Vec<_>>>()?,
            Err(err) => {
                let msg = format!("Unexpected storage error: {:?}", err);
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(msg)
                        .finish(Location::Undefined),
                );
            }
        }
        .into_iter();
        let mut result: Vec<SerializedPackage> = Vec::with_capacity(ids.len());

        // Zip them back up. Relative ordering has been preserved so we can just merge them back.
        for idx in 0..ids.len() {
            if cached.contains(&idx) {
                result.push(cached_packages.next().unwrap());
            } else {
                result.push(fetched_packages.next().unwrap());
            }
        }

        // Should all be the same length, the the ordering should be preserved.
        debug_assert_eq!(result.len(), ids.len());
        for (pkg, id) in result.iter().zip(ids.iter()) {
            debug_assert_eq!(pkg.storage_id, *id);
        }

        Ok(result)
    }
}
