// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::*;
use move_core_types::{
    account_address::AccountAddress,
    effects::{AccountChangeSet, ChangeSet, Op},
    identifier::{IdentStr, Identifier},
    language_storage::ModuleId,
    resolver::MoveResolver,
    vm_status::StatusCode,
};
use move_vm_types::data_store::DataStore;
use std::collections::btree_map::BTreeMap;

pub struct AccountDataCache {
    module_map: BTreeMap<Identifier, Vec<u8>>,
}

impl AccountDataCache {
    fn new() -> Self {
        Self {
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
pub(crate) struct TransactionDataCache<S> {
    remote: S,
    module_map: BTreeMap<AccountAddress, AccountDataCache>,
}

impl<S: MoveResolver> TransactionDataCache<S> {
    /// Create a `TransactionDataCache` with a `RemoteCache` that provides access to data
    /// not updated in the transaction.
    pub(crate) fn new(remote: S) -> Self {
        TransactionDataCache {
            remote,
            module_map: BTreeMap::new(),
        }
    }

    pub(crate) fn into_effects(mut self) -> (PartialVMResult<ChangeSet>, S) {
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
                    .add_account_changeset(addr, AccountChangeSet::from_modules(modules))
                    .expect("accounts should be unique");
            }
        }

        Ok(change_set)
    }

    pub(crate) fn get_remote_resolver(&self) -> &S {
        &self.remote
    }

    pub(crate) fn get_remote_resolver_mut(&mut self) -> &mut S {
        &mut self.remote
    }
}

// `DataStore` implementation for the `TransactionDataCache`
impl<S: MoveResolver> DataStore for TransactionDataCache<S> {
    fn link_context(&self) -> AccountAddress {
        self.remote.link_context()
    }

    fn relocate(&self, module_id: &ModuleId) -> PartialVMResult<ModuleId> {
        self.remote.relocate(module_id).map_err(|err| {
            PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!("Error relocating {module_id}: {err:?}"))
        })
    }

    fn defining_module(
        &self,
        module_id: &ModuleId,
        struct_: &IdentStr,
    ) -> PartialVMResult<ModuleId> {
        self.remote
            .defining_module(module_id, struct_)
            .map_err(|err| {
                PartialVMError::new(StatusCode::LINKER_ERROR).with_message(format!(
                    "Error finding defining module for {module_id}::{struct_}: {err:?}"
                ))
            })
    }

    fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>> {
        if let Some(account_cache) = self.module_map.get(module_id.address()) {
            if let Some(blob) = account_cache.module_map.get(module_id.name()) {
                return Ok(blob.clone());
            }
        }
        match self.remote.get_module(module_id) {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!("Cannot find {:?} in data cache", module_id))
                .finish(Location::Undefined)),
            Err(err) => {
                let msg = format!("Unexpected storage error: {:?}", err);
                Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(msg)
                        .finish(Location::Undefined),
                )
            }
        }
    }

    fn publish_module(&mut self, module_id: &ModuleId, blob: Vec<u8>) -> VMResult<()> {
        let account_cache = self
            .module_map
            .entry(*module_id.address())
            .or_insert_with(AccountDataCache::new);

        account_cache
            .module_map
            .insert(module_id.name().to_owned(), blob);

        Ok(())
    }
}
