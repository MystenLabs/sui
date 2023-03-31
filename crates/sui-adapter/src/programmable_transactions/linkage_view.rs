// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::RefCell,
    collections::{hash_map::Entry, BTreeMap, HashMap, HashSet},
    str::FromStr,
};

use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, SuiError},
    move_package::{MovePackage, TypeOrigin, UpgradeInfo},
};

use super::types::StorageView;

/// Exposes module and linkage resolution to the Move runtime.  The first by delegating to
/// `StorageView` and the second via linkage information that is loaded from a move package.
pub struct LinkageView<'state, S: StorageView> {
    /// Immutable access to the store for the transaction.
    state_view: &'state S,
    /// An explicit linkage context. (If none is set, then re-linking is disabled everywhere).
    linkage_info: Option<LinkageInfo>,
    /// Cache containing the type origin information from every package that has been set as the
    /// link context, and every other type that has been requested by the loader in this session.
    /// It's okay to retain entries in this cache between different link contexts because a type's
    /// Runtime ID and Defining ID are invariant between across link contexts.
    ///
    /// Cache is keyed first by the Runtime ID of the type's module, and then the type's identifier.
    /// The value is the ObjectID/Address of the package that introduced the type.
    type_origin_cache: RefCell<HashMap<ModuleId, HashMap<Identifier, AccountAddress>>>,
    /// Cache of past package addresses that have been the link context -- if a package is in this
    /// set, then we will not try to load its type origin table when setting it as a context (again).
    past_contexts: RefCell<HashSet<ObjectID>>,
}

pub struct LinkageInfo {
    link_context: AccountAddress,
    original_package_id: AccountAddress,
    linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
}

impl<'state, S: StorageView> LinkageView<'state, S> {
    pub fn new(state_view: &'state S) -> Self {
        Self {
            state_view,
            linkage_info: None,
            type_origin_cache: RefCell::new(HashMap::new()),
            past_contexts: RefCell::new(HashSet::new()),
        }
    }

    pub fn reset_linkage(&mut self) {
        self.linkage_info = None;
    }

    /// Set the linkage context to the information based on the linkage and type origin tables from
    /// the `context` package.  Returns the original package ID (aka the runtime ID) of the context
    /// package on success.
    pub fn set_linkage(&mut self, context: &MovePackage) -> Result<AccountAddress, ExecutionError> {
        let info = LinkageInfo::from(context);
        let storage_id = context.id();
        let runtime_id = info.original_package_id;
        self.linkage_info = Some(info);

        if !self.past_contexts.borrow_mut().insert(storage_id) {
            return Ok(runtime_id);
        }

        // Pre-populate the type origin cache with entries from the current package -- this is
        // necessary to serve "defining module" requests for unpublished packages, but will also
        // speed up other requests.
        for TypeOrigin {
            module_name,
            struct_name,
            package: defining_id,
        } in context.type_origin_table()
        {
            let Ok(module_name) = Identifier::from_str(module_name) else {
                return Err(ExecutionError::invariant_violation(format!(
                    "Module name isn't an identifier: {module_name}"
                )));
            };

            let Ok(struct_name) = Identifier::from_str(struct_name) else {
                return Err(ExecutionError::invariant_violation(format!(
                    "Struct name isn't an identifier: {struct_name}"
                )));
            };

            let runtime_id = ModuleId::new(runtime_id, module_name);
            self.add_type_origin(runtime_id, struct_name, *defining_id)?;
        }

        Ok(runtime_id)
    }

    pub fn storage(&self) -> &'state S {
        self.state_view
    }

    pub fn original_package_id(&self) -> AccountAddress {
        if let Some(info) = &self.linkage_info {
            info.original_package_id
        } else {
            AccountAddress::ZERO
        }
    }

    fn get_cached_type_origin(
        &self,
        runtime_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Option<AccountAddress> {
        self.type_origin_cache
            .borrow()
            .get(runtime_id)?
            .get(struct_)
            .cloned()
    }

    fn add_type_origin(
        &self,
        runtime_id: ModuleId,
        struct_: Identifier,
        defining_id: ObjectID,
    ) -> Result<(), ExecutionError> {
        let mut cache = self.type_origin_cache.borrow_mut();
        let module_cache = cache.entry(runtime_id.clone()).or_default();

        match module_cache.entry(struct_) {
            Entry::Vacant(entry) => {
                entry.insert(*defining_id);
            }

            Entry::Occupied(entry) => {
                if entry.get() != &*defining_id {
                    return Err(ExecutionError::invariant_violation(format!(
                        "Conflicting defining ID for {}::{}: {} and {}",
                        runtime_id,
                        entry.key(),
                        defining_id,
                        entry.get(),
                    )));
                }
            }
        }

        Ok(())
    }
}

impl From<&MovePackage> for LinkageInfo {
    fn from(package: &MovePackage) -> Self {
        Self {
            link_context: package.id().into(),
            original_package_id: package.original_package_id().into(),
            linkage_table: package.linkage_table().clone(),
        }
    }
}

impl<'state, S: StorageView> LinkageResolver for LinkageView<'state, S> {
    type Error = SuiError;

    fn link_context(&self) -> AccountAddress {
        if let Some(LinkageInfo { link_context, .. }) = &self.linkage_info {
            *link_context
        } else {
            AccountAddress::ZERO
        }
    }

    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        let Some(linkage) = &self.linkage_info else {
            return Ok(module_id.clone());
        };

        // The request is to relocate a module in the package that the link context is from.  This
        // entry will not be stored in the linkage table, so must be handled specially.
        if module_id.address() == &linkage.original_package_id {
            return Ok(ModuleId::new(
                linkage.link_context,
                module_id.name().to_owned(),
            ));
        }

        let runtime_id = ObjectID::from_address(*module_id.address());
        let Some(upgrade) = linkage.linkage_table.get(&runtime_id) else {
            return Err(ExecutionError::invariant_violation(format!(
                "Missing linkage for {runtime_id} in context {}",
                linkage.link_context,
            )).into());
        };

        Ok(ModuleId::new(
            upgrade.upgraded_id.into(),
            module_id.name().to_owned(),
        ))
    }

    fn defining_module(
        &self,
        runtime_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        if self.linkage_info.is_none() {
            return Ok(runtime_id.clone());
        }

        if let Some(cached) = self.get_cached_type_origin(runtime_id, struct_) {
            return Ok(ModuleId::new(cached, runtime_id.name().to_owned()));
        }

        let storage_id = ObjectID::from(*self.relocate(runtime_id)?.address());
        let Some(package) = self.state_view.get_package(&storage_id)? else {
            return Err(ExecutionError::invariant_violation(format!(
                "Missing dependent package in store: {storage_id}",
            )).into())
        };

        for TypeOrigin {
            module_name,
            struct_name,
            package,
        } in package.type_origin_table()
        {
            if module_name == runtime_id.name().as_str() && struct_name == struct_.as_str() {
                self.add_type_origin(runtime_id.clone(), struct_.to_owned(), *package)?;
                return Ok(ModuleId::new(**package, runtime_id.name().to_owned()));
            }
        }

        Err(ExecutionError::invariant_violation(format!(
            "{runtime_id}::{struct_} not found in type origin table in {storage_id} (v{})",
            package.version(),
        ))
        .into())
    }
}

/** Remaining implementations delegated to StorageView ************************/

impl<'state, S: StorageView> ResourceResolver for LinkageView<'state, S> {
    type Error = <S as ResourceResolver>::Error;

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.state_view.get_resource(address, typ)
    }
}

impl<'state, S: StorageView> ModuleResolver for LinkageView<'state, S> {
    type Error = <S as ModuleResolver>::Error;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.state_view.get_module(id)
    }
}
