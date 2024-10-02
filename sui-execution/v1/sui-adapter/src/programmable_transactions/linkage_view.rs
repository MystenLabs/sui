// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::RefCell,
    collections::{hash_map::Entry, BTreeMap, HashMap, HashSet},
    str::FromStr,
};

use crate::execution_value::SuiResolver;
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};
use sui_types::storage::{get_module, PackageObject};
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, SuiError, SuiResult},
    move_package::{MovePackage, TypeOrigin, UpgradeInfo},
    storage::BackingPackageStore,
};

/// Exposes module and linkage resolution to the Move runtime.  The first by delegating to
/// `resolver` and the second via linkage information that is loaded from a move package.
pub struct LinkageView<'state> {
    /// Interface to resolve packages, modules and resources directly from the store.
    resolver: Box<dyn SuiResolver + 'state>,
    /// Information used to change module and type identities during linkage.
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

#[derive(Debug)]
pub struct LinkageInfo {
    storage_id: AccountAddress,
    runtime_id: AccountAddress,
    link_table: BTreeMap<ObjectID, UpgradeInfo>,
}

pub struct SavedLinkage(LinkageInfo);

impl<'state> LinkageView<'state> {
    pub fn new(resolver: Box<dyn SuiResolver + 'state>) -> Self {
        Self {
            resolver,
            linkage_info: None,
            type_origin_cache: RefCell::new(HashMap::new()),
            past_contexts: RefCell::new(HashSet::new()),
        }
    }

    pub fn reset_linkage(&mut self) {
        self.linkage_info = None;
    }

    /// Indicates whether this `LinkageView` has had its context set to match the linkage in
    /// `context`.
    pub fn has_linkage(&self, context: ObjectID) -> bool {
        self.linkage_info
            .as_ref()
            .is_some_and(|l| l.storage_id == *context)
    }

    /// Reset the linkage, but save the context that existed before, if there was one.
    pub fn steal_linkage(&mut self) -> Option<SavedLinkage> {
        Some(SavedLinkage(self.linkage_info.take()?))
    }

    /// Restore a previously saved linkage context.  Fails if there is already a context set.
    pub fn restore_linkage(&mut self, saved: Option<SavedLinkage>) -> Result<(), ExecutionError> {
        let Some(SavedLinkage(saved)) = saved else {
            return Ok(());
        };

        if let Some(existing) = &self.linkage_info {
            invariant_violation!(
                "Attempt to overwrite linkage by restoring: {saved:#?} \
                 Existing linkage: {existing:#?}",
            )
        }

        // No need to populate type origin cache, because a saved context must have been set as a
        // linkage before, and the cache would have been populated at that time.
        self.linkage_info = Some(saved);
        Ok(())
    }

    /// Set the linkage context to the information based on the linkage and type origin tables from
    /// the `context` package.  Returns the original package ID (aka the runtime ID) of the context
    /// package on success.
    pub fn set_linkage(&mut self, context: &MovePackage) -> Result<AccountAddress, ExecutionError> {
        if let Some(existing) = &self.linkage_info {
            invariant_violation!(
                "Attempt to overwrite linkage info with context from {}. \
                    Existing linkage: {existing:#?}",
                context.id(),
            )
        }

        let linkage = LinkageInfo::from(context);
        let storage_id = context.id();
        let runtime_id = linkage.runtime_id;
        self.linkage_info = Some(linkage);

        if !self.past_contexts.borrow_mut().insert(storage_id) {
            return Ok(runtime_id);
        }

        // Pre-populate the type origin cache with entries from the current package -- this is
        // necessary to serve "defining module" requests for unpublished packages, but will also
        // speed up other requests.
        for TypeOrigin {
            module_name,
            datatype_name: struct_name,
            package: defining_id,
        } in context.type_origin_table()
        {
            let Ok(module_name) = Identifier::from_str(module_name) else {
                invariant_violation!("Module name isn't an identifier: {module_name}");
            };

            let Ok(struct_name) = Identifier::from_str(struct_name) else {
                invariant_violation!("Struct name isn't an identifier: {struct_name}");
            };

            let runtime_id = ModuleId::new(runtime_id, module_name);
            self.add_type_origin(runtime_id, struct_name, *defining_id)?;
        }

        Ok(runtime_id)
    }

    pub fn original_package_id(&self) -> Option<AccountAddress> {
        Some(self.linkage_info.as_ref()?.runtime_id)
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
                    invariant_violation!(
                        "Conflicting defining ID for {}::{}: {} and {}",
                        runtime_id,
                        entry.key(),
                        defining_id,
                        entry.get(),
                    );
                }
            }
        }

        Ok(())
    }

    pub(crate) fn link_context(&self) -> AccountAddress {
        self.linkage_info
            .as_ref()
            .map_or(AccountAddress::ZERO, |l| l.storage_id)
    }

    pub(crate) fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, SuiError> {
        let Some(linkage) = &self.linkage_info else {
            invariant_violation!("No linkage context set while relocating {module_id}.")
        };

        // The request is to relocate a module in the package that the link context is from.  This
        // entry will not be stored in the linkage table, so must be handled specially.
        if module_id.address() == &linkage.runtime_id {
            return Ok(ModuleId::new(
                linkage.storage_id,
                module_id.name().to_owned(),
            ));
        }

        let runtime_id = ObjectID::from_address(*module_id.address());
        let Some(upgrade) = linkage.link_table.get(&runtime_id) else {
            invariant_violation!(
                "Missing linkage for {runtime_id} in context {}, runtime_id is {}",
                linkage.storage_id,
                linkage.runtime_id
            );
        };

        Ok(ModuleId::new(
            upgrade.upgraded_id.into(),
            module_id.name().to_owned(),
        ))
    }

    pub(crate) fn defining_module(
        &self,
        runtime_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Result<ModuleId, SuiError> {
        if self.linkage_info.is_none() {
            invariant_violation!(
                "No linkage context set for defining module query on {runtime_id}::{struct_}."
            )
        }

        if let Some(cached) = self.get_cached_type_origin(runtime_id, struct_) {
            return Ok(ModuleId::new(cached, runtime_id.name().to_owned()));
        }

        let storage_id = ObjectID::from(*self.relocate(runtime_id)?.address());
        let Some(package) = self.resolver.get_package_object(&storage_id)? else {
            invariant_violation!("Missing dependent package in store: {storage_id}",)
        };

        for TypeOrigin {
            module_name,
            datatype_name: struct_name,
            package,
        } in package.move_package().type_origin_table()
        {
            if module_name == runtime_id.name().as_str() && struct_name == struct_.as_str() {
                self.add_type_origin(runtime_id.clone(), struct_.to_owned(), *package)?;
                return Ok(ModuleId::new(**package, runtime_id.name().to_owned()));
            }
        }

        invariant_violation!(
            "{runtime_id}::{struct_} not found in type origin table in {storage_id} (v{})",
            package.move_package().version(),
        )
    }
}

impl From<&MovePackage> for LinkageInfo {
    fn from(package: &MovePackage) -> Self {
        Self {
            storage_id: package.id().into(),
            runtime_id: package.original_package_id().into(),
            link_table: package.linkage_table().clone(),
        }
    }
}

impl<'state> LinkageResolver for LinkageView<'state> {
    type Error = SuiError;

    fn link_context(&self) -> AccountAddress {
        LinkageView::link_context(self)
    }

    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        LinkageView::relocate(self, module_id)
    }

    fn defining_module(
        &self,
        runtime_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        LinkageView::defining_module(self, runtime_id, struct_)
    }
}

/** Remaining implementations delegated to state_view *************************/

impl<'state> ResourceResolver for LinkageView<'state> {
    type Error = SuiError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.resolver.get_resource(address, typ)
    }
}

impl<'state> ModuleResolver for LinkageView<'state> {
    type Error = SuiError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        get_module(self, id)
    }
}

impl<'state> BackingPackageStore for LinkageView<'state> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        self.resolver.get_package_object(package_id)
    }
}
