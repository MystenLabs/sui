// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::loader::Loader;

use move_binary_format::errors::*;
use move_core_types::{
    account_address::AccountAddress,
    effects::{AccountChangeSet, ChangeSet, Op},
    gas_algebra::NumBytes,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    resolver::MoveResolver,
    runtime_value::MoveTypeLayout,
    vm_status::StatusCode,
};
use move_vm_types::{
    data_store::DataStore,
    loaded_data::runtime_types::Type,
    values::{GlobalValue, Value},
};
use std::collections::btree_map::BTreeMap;

pub struct AccountDataCache {
    data_map: BTreeMap<Type, (MoveTypeLayout, GlobalValue)>,
    module_map: BTreeMap<Identifier, Vec<u8>>,
}

impl AccountDataCache {
    fn new() -> Self {
        Self {
            data_map: BTreeMap::new(),
            module_map: BTreeMap::new(),
        }
    }
}

/// Transaction data cache. Keep updates within a transaction so they can all be published at
/// once when the transaction succeeds.
///
/// It also provides an implementation for the opcodes that refer to storage and gives the
/// proper guarantees of reference lifetime.
///
/// Dirty objects are serialized and returned in make_write_set.
///
/// It is a responsibility of the client to publish changes once the transaction is executed.
///
/// The Move VM takes a `DataStore` in input and this is the default and correct implementation
/// for a data store related to a transaction. Clients should create an instance of this type
/// and pass it to the Move VM.
pub(crate) struct TransactionDataCache<'l, S> {
    remote: S,
    loader: &'l Loader,
    account_map: BTreeMap<AccountAddress, AccountDataCache>,
}

impl<'l, S: MoveResolver> TransactionDataCache<'l, S> {
    /// Create a `TransactionDataCache` with a `RemoteCache` that provides access to data
    /// not updated in the transaction.
    pub(crate) fn new(remote: S, loader: &'l Loader) -> Self {
        TransactionDataCache {
            remote,
            loader,
            account_map: BTreeMap::new(),
        }
    }

    /// Make a write set from the updated (dirty, deleted) global resources along with
    /// published modules.
    ///
    /// Gives all proper guarantees on lifetime of global data as well.
    pub(crate) fn into_effects(mut self) -> (PartialVMResult<ChangeSet>, S) {
        (self.impl_into_effects(), self.remote)
    }
    fn impl_into_effects(&mut self) -> PartialVMResult<ChangeSet> {
        let mut change_set = ChangeSet::new();
        for (addr, account_data_cache) in std::mem::take(&mut self.account_map).into_iter() {
            let mut modules = BTreeMap::new();
            for (module_name, module_blob) in account_data_cache.module_map {
                modules.insert(module_name, Op::New(module_blob));
            }

            let mut resources = BTreeMap::new();
            for (ty, (layout, gv)) in account_data_cache.data_map {
                let op = match gv.into_effect() {
                    Some(op) => op,
                    None => continue,
                };

                let struct_tag = match self.loader.type_to_type_tag(&ty)? {
                    TypeTag::Struct(struct_tag) => *struct_tag,
                    _ => return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)),
                };

                match op {
                    Op::New(val) => {
                        let resource_blob = val
                            .simple_serialize(&layout)
                            .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?;
                        resources.insert(struct_tag, Op::New(resource_blob));
                    }
                    Op::Modify(val) => {
                        let resource_blob = val
                            .simple_serialize(&layout)
                            .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?;
                        resources.insert(struct_tag, Op::Modify(resource_blob));
                    }
                    Op::Delete => {
                        resources.insert(struct_tag, Op::Delete);
                    }
                }
            }
            if !modules.is_empty() || !resources.is_empty() {
                change_set
                    .add_account_changeset(
                        addr,
                        AccountChangeSet::from_modules_resources(modules, resources),
                    )
                    .expect("accounts should be unique");
            }
        }

        Ok(change_set)
    }

    pub(crate) fn num_mutated_accounts(&self, sender: &AccountAddress) -> u64 {
        // The sender's account will always be mutated.
        let mut total_mutated_accounts: u64 = 1;
        for (addr, entry) in self.account_map.iter() {
            if addr != sender && entry.data_map.values().any(|(_, v)| v.is_mutated()) {
                total_mutated_accounts += 1;
            }
        }
        total_mutated_accounts
    }

    pub(crate) fn get_remote_resolver(&self) -> &S {
        &self.remote
    }

    pub(crate) fn get_remote_resolver_mut(&mut self) -> &mut S {
        &mut self.remote
    }
}

// `DataStore` implementation for the `TransactionDataCache`
impl<'l, S: MoveResolver> DataStore for TransactionDataCache<'l, S> {
    // Retrieve data from the local cache or loads it from the remote cache into the local cache.
    // All operations on the global data are based on this API and they all load the data
    // into the cache.
    fn load_resource(
        &mut self,
        addr: AccountAddress,
        ty: &Type,
    ) -> PartialVMResult<(&mut GlobalValue, Option<Option<NumBytes>>)> {
        let account_cache = self
            .account_map
            .entry(addr)
            .or_insert_with(AccountDataCache::new);

        let mut load_res = None;
        if !account_cache.data_map.contains_key(ty) {
            let ty_tag = match self.loader.type_to_type_tag(ty)? {
                TypeTag::Struct(s_tag) => s_tag,
                _ =>
                // non-struct top-level value; can't happen
                {
                    return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))
                }
            };
            // TODO(Gas): Shall we charge for this?
            let ty_layout = self.loader.type_to_type_layout(ty)?;

            let gv = match self.remote.get_resource(&addr, &ty_tag) {
                Ok(Some(blob)) => {
                    load_res = Some(Some(NumBytes::new(blob.len() as u64)));
                    let val = match Value::simple_deserialize(&blob, &ty_layout) {
                        Some(val) => val,
                        None => {
                            let msg =
                                format!("Failed to deserialize resource {} at {}!", ty_tag, addr);
                            return Err(PartialVMError::new(
                                StatusCode::FAILED_TO_DESERIALIZE_RESOURCE,
                            )
                            .with_message(msg));
                        }
                    };

                    GlobalValue::cached(val)?
                }
                Ok(None) => {
                    load_res = Some(None);
                    GlobalValue::none()
                }
                Err(err) => {
                    let msg = format!("Unexpected storage error: {:?}", err);
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(msg),
                    );
                }
            };

            account_cache.data_map.insert(ty.clone(), (ty_layout, gv));
        }

        Ok((
            account_cache
                .data_map
                .get_mut(ty)
                .map(|(_ty_layout, gv)| gv)
                .expect("global value must exist"),
            load_res,
        ))
    }

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
        if let Some(account_cache) = self.account_map.get(module_id.address()) {
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
            .account_map
            .entry(*module_id.address())
            .or_insert_with(AccountDataCache::new);

        account_cache
            .module_map
            .insert(module_id.name().to_owned(), blob);

        Ok(())
    }
}
