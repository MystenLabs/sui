// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::swap_authenticator_and_id;

use anyhow::Result;

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_cli::sandbox::utils::on_disk_state_view::OnDiskStateView;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
    resolver::{ModuleResolver, ResourceResolver},
};

use fastx_types::{
    base_types::*,
    object::Object,
    storage::Storage,
};
use std::{collections::BTreeMap};

pub struct FastXStateView {
    pub inner: OnDiskStateView,
    objects: BTreeMap<ObjectID, Object>,
}

impl FastXStateView {
    pub fn create(build_dir: &str, storage_dir: &str) -> Result<Self> {
        let state_view = OnDiskStateView::create(build_dir, storage_dir)?;
        Ok(Self { inner: state_view, objects: BTreeMap::new() })
    }
}

impl ModuleResolver for FastXStateView {
    type Error = anyhow::Error;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.inner.get_module(module_id)
    }
}

impl ResourceResolver for FastXStateView {
    type Error = anyhow::Error;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        // do the reverse of the ID/authenticator address splice performed on saving transfer events
        Ok(
            match self
                .inner
                .get_resource_bytes(*address, struct_tag.clone())?
            {
                Some(mut obj) => {
                    swap_authenticator_and_id(*address, &mut obj);
                    Some(obj)
                }
                None => None,
            },
        )
    }
}

impl GetModule for FastXStateView {
    type Error = anyhow::Error;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<CompiledModule>, Self::Error> {
        self.inner.get_module_by_id(id)
    }
}

// Use basic in-mem for now
// TODO: extend for persistence
impl Storage for FastXStateView {
    fn read_object(&self, id: &ObjectID) -> Option<Object> {
        self.objects.get(id).cloned()
    }

    fn write_object(&mut self, object: Object) {
        self.objects.insert(object.id(), object);
    }

    fn delete_object(&mut self, id: &ObjectID) {
        self.objects.remove(id);
    }
}
