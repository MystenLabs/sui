// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    error::{SuiError, SuiResult},
    event::Event,
    move_package::UpgradeInfo,
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver, ObjectChange, ParentSync, Storage},
};

use super::types::StorageView;

/// TODO Docs
#[allow(dead_code)]
pub struct LinkageView<'state, S: StorageView> {
    state_view: &'state S,
    linkage_info: Option<LinkageInfo>,
}

/// TODO Docs
#[allow(dead_code)]
pub struct LinkageInfo {
    link_context: ObjectID,
    linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
}

impl<'state, S: StorageView> LinkageView<'state, S> {
    pub fn new(state_view: &'state S) -> Self {
        Self {
            state_view,
            linkage_info: None,
        }
    }

    pub fn storage(&self) -> &'state S {
        self.state_view
    }
}

impl<'state, S: StorageView> LinkageResolver for LinkageView<'state, S> {
    type Error = SuiError;

    fn link_context(&self) -> AccountAddress {
        AccountAddress::ZERO
    }

    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        Ok(module_id.clone())
    }

    fn defining_module(
        &self,
        module_id: &ModuleId,
        _struct: &move_core_types::identifier::IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        Ok(module_id.clone())
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

impl<'state, S: StorageView> BackingPackageStore for LinkageView<'state, S> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        self.state_view.get_package_object(package_id)
    }
}

impl<'state, S: StorageView> Storage for LinkageView<'state, S> {
    fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        self.state_view.read_object(id)
    }

    fn reset(&mut self) {
        unimplemented!("Read-only storage only.")
    }

    fn log_event(&mut self, _event: Event) {
        unimplemented!("Read-only storage only.")
    }

    fn apply_object_changes(&mut self, _changes: BTreeMap<ObjectID, ObjectChange>) {
        unimplemented!("Read-only storage only.")
    }
}

impl<'state, S: StorageView> ParentSync for LinkageView<'state, S> {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        self.state_view.get_latest_parent_entry_ref(object_id)
    }
}

impl<'state, S: StorageView> ChildObjectResolver for LinkageView<'state, S> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        self.state_view.read_child_object(parent, child)
    }
}
