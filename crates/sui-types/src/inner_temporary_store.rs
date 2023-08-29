// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::VersionDigest;
use crate::effects::TransactionEvents;
use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    object::Object,
    storage::{DeleteKind, WriteKind},
};
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::sync::Arc;

pub type WrittenObjects = BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>;
pub type ObjectMap = BTreeMap<ObjectID, Object>;
pub type TxCoins = (ObjectMap, WrittenObjects);

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InnerTemporaryStore {
    pub objects: ObjectMap,
    pub mutable_inputs: BTreeMap<ObjectID, VersionDigest>,
    // All the written objects' sequence number should have been updated to the lamport version.
    pub written: WrittenObjects,
    // deleted or wrapped or unwrap-then-delete. The sequence number should have been updated to
    // the lamport version.
    pub deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    pub loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    pub events: TransactionEvents,
    pub max_binary_format_version: u32,
    pub no_extraneous_module_bytes: bool,
    pub runtime_packages_loaded_from_db: BTreeMap<ObjectID, Object>,
}

pub struct TemporaryModuleResolver<'a, R> {
    temp_store: &'a InnerTemporaryStore,
    fallback: R,
}

impl<'a, R> TemporaryModuleResolver<'a, R> {
    pub fn new(temp_store: &'a InnerTemporaryStore, fallback: R) -> Self {
        Self {
            temp_store,
            fallback,
        }
    }
}

impl<R> GetModule for TemporaryModuleResolver<'_, R>
where
    R: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>,
{
    type Error = anyhow::Error;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        let obj = self.temp_store.written.get(&ObjectID::from(*id.address()));
        if let Some((_, o, _)) = obj {
            if let Some(p) = o.data.try_as_package() {
                return Ok(Some(Arc::new(p.deserialize_module(
                    &id.name().into(),
                    self.temp_store.max_binary_format_version,
                    self.temp_store.no_extraneous_module_bytes,
                )?)));
            }
        }
        self.fallback.get_module_by_id(id)
    }
}
