// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use crate::programmable_transactions::types::StorageView;

use sui_types::{
    base_types::ObjectID, error::SuiResult, move_package::MovePackage, object::Object,
    storage::ChildObjectResolver,
};

use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};

pub struct LinkageInfo {
    pub running_pkg: MovePackage,
}

pub struct StorageContext<'a, E> {
    storage_view: &'a (dyn StorageView<E> + 'a),
    linkage_info: LinkageInfo,
}

impl<'a, E> StorageContext<'a, E> {
    pub fn new(storage_view: &'a (dyn StorageView<E> + 'a), running_pkg: MovePackage) -> Self {
        Self {
            storage_view,
            linkage_info: LinkageInfo { running_pkg },
        }
    }
}

impl<'a, E: fmt::Debug> ChildObjectResolver for StorageContext<'a, E> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        self.storage_view.read_child_object(parent, child)
    }
}

impl<'a, E: fmt::Debug> ModuleResolver for StorageContext<'a, E> {
    type Error = E;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.storage_view.get_module(module_id)
    }
}

impl<'a, E: fmt::Debug> ResourceResolver for StorageContext<'a, E> {
    type Error = E;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.storage_view.get_resource(address, struct_tag)
    }
}

impl<'a, E: fmt::Debug> LinkageResolver for StorageContext<'a, E> {
    type Error = E;

    fn link_context(&self) -> AccountAddress {
        self.linkage_info.running_pkg.id().into()
    }

    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        Ok(module_id.clone())
        /*
        let old_id: ObjectID = (*module_id.address()).into();
        let new_id = self
            .linkage_info
            .running_pkg
            .linkage_table()
            .get(&old_id)
            .unwrap()
            .upgraded_id;
        Ok(ModuleId::new(new_id.into(), module_id.name().into()))
        */
    }
}
