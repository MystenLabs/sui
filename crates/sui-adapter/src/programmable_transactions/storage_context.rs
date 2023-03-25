// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cell::RefCell, collections::BTreeMap, fmt, marker::PhantomData};

use crate::programmable_transactions::types::StorageView;

use sui_types::{
    base_types::ObjectID,
    error::SuiResult,
    error::{ExecutionError, ExecutionErrorKind},
    move_package::{type_origin_table_to_map, TypeOrigin, UpgradeInfo},
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver},
};

use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};

pub struct LinkageInfo {
    pub pkg_id: ObjectID,
    linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
    type_origin_map: BTreeMap<(String, String), ObjectID>,
}

impl LinkageInfo {
    pub fn new(
        pkg_id: ObjectID,
        linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
        type_origin_table: &[TypeOrigin],
    ) -> Self {
        let type_origin_map = type_origin_table_to_map(type_origin_table);
        Self {
            pkg_id,
            linkage_table,
            type_origin_map,
        }
    }
}

pub struct StorageContext<'a, E, S> {
    pub storage_view: &'a S,
    linkage_info: RefCell<Option<LinkageInfo>>,
    _p: PhantomData<E>,
}

impl<
        'a,
        E,
        S: ResourceResolver<Error = E>
            + ModuleResolver<Error = E>
            + BackingPackageStore
            + ChildObjectResolver,
    > StorageContext<'a, E, S>
{
    pub fn new(storage_view: &'a S) -> Self {
        Self {
            storage_view,
            linkage_info: RefCell::new(None),
            _p: PhantomData,
        }
    }

    pub fn set_context(&self, linkage_info: LinkageInfo) -> Result<(), ExecutionError> {
        if self.linkage_info.borrow().is_some() {
            return Err(ExecutionErrorKind::VMInvariantViolation.into());
        }
        self.linkage_info.replace(Some(linkage_info));
        Ok(())
    }

    pub fn compute_context(&self, pkg_id: ObjectID) -> Result<(), ExecutionError> {
        if self.linkage_info.borrow().is_some() {
            return Err(ExecutionErrorKind::VMInvariantViolation.into());
        }
        let running_pkg = &self.storage_view.get_packages(&[pkg_id]).unwrap().unwrap()[0];
        self.set_context(LinkageInfo {
            pkg_id: running_pkg.id(),
            linkage_table: running_pkg.linkage_table().clone(),
            type_origin_map: running_pkg.type_origin_map(),
        })
    }

    pub fn reset_context(&self) {
        self.linkage_info.replace(None);
    }
}

impl<'a, E: fmt::Debug, S: StorageView<E>> ChildObjectResolver for StorageContext<'a, E, S> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        self.storage_view.read_child_object(parent, child)
    }
}

impl<'a, E: fmt::Debug, S: StorageView<E>> ModuleResolver for StorageContext<'a, E, S> {
    type Error = E;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.storage_view.get_module(module_id)
    }
}

impl<'a, E: fmt::Debug, S: StorageView<E>> ResourceResolver for StorageContext<'a, E, S> {
    type Error = E;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.storage_view.get_resource(address, struct_tag)
    }
}

impl<'a, E: fmt::Debug, S: StorageView<E>> LinkageResolver for StorageContext<'a, E, S> {
    type Error = E;

    /// The link context identifies the mapping from runtime `ModuleId`s to the `ModuleId`s in
    /// storage that they are loaded from as returned by `relocate`.
    fn link_context(&self) -> AccountAddress {
        self.linkage_info.borrow().as_ref().unwrap().pkg_id.into()
    }

    /// Translate the runtime `module_id` to the on-chain `ModuleId` that it should be loaded from.
    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        let old_id: ObjectID = (*module_id.address()).into();
        let linkage_info_opt = self.linkage_info.borrow();
        let linkage_info = linkage_info_opt.as_ref().unwrap();
        if linkage_info.pkg_id == old_id {
            // a linker may issue a query for a module in the package represented by the link
            // context, in which case the result is going to be the same module
            Ok(module_id.clone())
        } else {
            Ok(ModuleId::new(
                linkage_info
                    .linkage_table
                    .get(&old_id)
                    .unwrap()
                    .upgraded_id
                    .into(),
                module_id.name().into(),
            ))
        }
    }

    /// Translate the runtime fully-qualified struct name to the on-chain `ModuleId` that originally
    /// defined that type.
    fn defining_module(
        &self,
        module_id: &ModuleId,
        _struct: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        let m_name = module_id.name().to_string();
        let s_name = _struct.to_string();
        let linkage_info_opt = self.linkage_info.borrow();
        let linkage_info = linkage_info_opt.as_ref().unwrap();
        let mod_id: ObjectID = (*module_id.address()).into();
        let defining_pkg_id = if mod_id == linkage_info.pkg_id || mod_id == ObjectID::ZERO {
            // mod_id == linkage_info.pkg_id is a special case needed in case the type origin table
            // is not yet available in storage but may have to already serve queries (e.g. during
            // object publishing)
            // TODO: is the above really true or we don't need type origin table in the linkage context?
            // TODO: it seems like we are also getting mod_id == 0x0 - is looking up linkage
            // context's origin table OK in this case?
            *linkage_info.type_origin_map.get(&(m_name, s_name)).unwrap()
        } else {
            // TODO: load a package just to answer the query is expensive, but is it also safe to
            // call from the linker?
            let defining_org_pkg_id = linkage_info.linkage_table.get(&mod_id).unwrap().upgraded_id;
            let defining_org_pkg = &self
                .storage_view
                .get_packages(&[defining_org_pkg_id])
                .unwrap()
                .unwrap()[0];
            *defining_org_pkg
                .type_origin_table()
                .iter()
                .find_map(
                    |TypeOrigin {
                         module_name,
                         struct_name,
                         package,
                     }| {
                        if module_name == &m_name && struct_name == &s_name {
                            Some(package)
                        } else {
                            None
                        }
                    },
                )
                .unwrap()
        };

        Ok(ModuleId::new(
            (defining_pkg_id).into(),
            module_id.name().into(),
        ))
    }
}
