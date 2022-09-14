// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use std::collections::BTreeMap;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    error::{SuiError, SuiResult},
    object::Object,
    storage::{BackingPackageStore, DeleteKind, ParentSync, WriteKind},
};

// TODO: We should use AuthorityTemporaryStore instead.
// Keeping this functionally identical to AuthorityTemporaryStore is a pain.
#[derive(Debug, Default)]
pub struct InMemoryStorage {
    persistent: BTreeMap<ObjectID, Object>,
    last_entry_for_deleted: BTreeMap<ObjectID, ObjectRef>,
}

impl BackingPackageStore for InMemoryStorage {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        Ok(self.persistent.get(package_id).cloned())
    }
}

impl ParentSync for InMemoryStorage {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        debug_assert!(!self.persistent.contains_key(&object_id));
        Ok(self.last_entry_for_deleted.get(&object_id).copied())
    }
}

impl ModuleResolver for InMemoryStorage {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .get_package(&ObjectID::from(*module_id.address()))?
            .and_then(|package| {
                package
                    .data
                    .try_as_package()
                    .unwrap()
                    .serialized_module_map()
                    .get(module_id.name().as_str())
                    .cloned()
            }))
    }
}

impl ModuleResolver for &mut InMemoryStorage {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        (**self).get_module(module_id)
    }
}

impl InMemoryStorage {
    pub fn new(objects: Vec<Object>) -> Self {
        let mut persistent = BTreeMap::new();
        for o in objects {
            persistent.insert(o.id(), o);
        }
        Self {
            persistent,
            last_entry_for_deleted: BTreeMap::new(),
        }
    }

    pub fn get_object(&self, id: &ObjectID) -> Option<&Object> {
        self.persistent.get(id)
    }

    pub fn get_objects(&self, objects: &[ObjectID]) -> Vec<Option<&Object>> {
        let mut result = Vec::new();
        for id in objects {
            result.push(self.get_object(id));
        }
        result
    }

    pub fn insert_object(&mut self, object: Object) {
        let id = object.id();
        self.last_entry_for_deleted.remove(&id);
        self.persistent.insert(id, object);
    }

    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.persistent
    }

    pub fn into_inner(self) -> BTreeMap<ObjectID, Object> {
        self.persistent
    }

    pub fn finish(
        &mut self,
        written: BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
        deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    ) {
        debug_assert!(written.keys().all(|id| !deleted.contains_key(id)));
        for (_id, (_, new_object, _)) in written {
            debug_assert!(new_object.id() == _id);
            self.insert_object(new_object);
        }
        for (id, _) in deleted {
            if let Some(obj) = self.persistent.remove(&id) {
                self.last_entry_for_deleted
                    .insert(id, obj.compute_object_reference());
            }
        }
    }
}
