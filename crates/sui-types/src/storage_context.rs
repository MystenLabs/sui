// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    base_types::{ObjectID, ObjectRef},
    error::{SuiError, SuiResult},
    event::Event,
    object::Object,
    storage::{
        BackingPackageStore, ChildObjectResolver, LinkageInitializer, ObjectChange, ParentSync,
        Storage,
    },
    temporary_store::TemporaryStore,
};

use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};

pub struct LinkageInfo {
    pub running_pkg_id: Option<ObjectID>,
}

pub struct StorageContext<S> {
    temp_store: TemporaryStore<S>,
    linkage_info: LinkageInfo,
}

impl<S> StorageContext<S> {
    pub fn new(temp_store: TemporaryStore<S>) -> Self {
        Self {
            temp_store,
            linkage_info: LinkageInfo {
                running_pkg_id: None,
            },
        }
    }

    pub fn temp_store(&self) -> &TemporaryStore<S> {
        &self.temp_store
    }

    pub fn temp_store_mut(&mut self) -> &mut TemporaryStore<S> {
        &mut self.temp_store
    }

    pub fn into_temp_store(self) -> TemporaryStore<S> {
        self.temp_store
    }
}

impl<S: BackingPackageStore> BackingPackageStore for StorageContext<S> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        self.temp_store.get_package_object(package_id)
    }
}

impl<S: ChildObjectResolver> ChildObjectResolver for StorageContext<S> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        self.temp_store.read_child_object(parent, child)
    }
}

impl<S: ParentSync> ParentSync for StorageContext<S> {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        self.temp_store.get_latest_parent_entry_ref(object_id)
    }
}

impl<S: ChildObjectResolver> Storage for StorageContext<S> {
    fn reset(&mut self) {
        TemporaryStore::drop_writes(&mut self.temp_store)
    }

    fn log_event(&mut self, event: Event) {
        TemporaryStore::log_event(&mut self.temp_store, event)
    }

    fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        TemporaryStore::read_object(&self.temp_store, id)
    }

    fn apply_object_changes(&mut self, changes: BTreeMap<ObjectID, ObjectChange>) {
        TemporaryStore::apply_object_changes(&mut self.temp_store, changes)
    }
}

impl<S: BackingPackageStore> ModuleResolver for StorageContext<S> {
    type Error = SuiError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.temp_store.get_module(module_id)
    }
}

impl<S> ResourceResolver for StorageContext<S> {
    type Error = SuiError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.temp_store.get_resource(address, struct_tag)
    }
}

impl<S: BackingPackageStore> LinkageInitializer for StorageContext<S> {
    fn init(&mut self, id: ObjectID) {
        self.linkage_info.running_pkg_id = Some(id);
    }
}

impl<S: BackingPackageStore> LinkageResolver for StorageContext<S> {
    type Error = SuiError;

    fn link_context(&self) -> AccountAddress {
        self.linkage_info.running_pkg_id.unwrap().into()
    }

    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        Ok(module_id.clone())
    }
}
