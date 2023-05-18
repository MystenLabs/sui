// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::VersionNumber;
use crate::storage::get_module_by_id;
use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    error::{SuiError, SuiResult},
    object::{Object, Owner},
    storage::{
        BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectStore, ParentSync, WriteKind,
    },
};
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use std::collections::BTreeMap;

// TODO: We should use AuthorityTemporaryStore instead.
// Keeping this functionally identical to AuthorityTemporaryStore is a pain.
#[derive(Debug, Default)]
pub struct InMemoryStorage {
    persistent: BTreeMap<ObjectID, Object>,
    last_entry_for_deleted: BTreeMap<ObjectID, ObjectRef>,
    wrapped: BTreeMap<ObjectID, VersionNumber>,
}

impl BackingPackageStore for InMemoryStorage {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        Ok(self.persistent.get(package_id).cloned())
    }
}

impl ChildObjectResolver for InMemoryStorage {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        let child_object = match self.persistent.get(child).cloned() {
            None => return Ok(None),
            Some(obj) => obj,
        };
        let parent = *parent;
        if child_object.owner != Owner::ObjectOwner(parent.into()) {
            return Err(SuiError::InvalidChildObjectAccess {
                object: *child,
                given_parent: parent,
                actual_owner: child_object.owner,
            });
        }
        Ok(Some(child_object))
    }
}

impl ParentSync for InMemoryStorage {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        if let Some(obj) = self.persistent.get(&object_id) {
            return Ok(Some(obj.compute_object_reference()));
        }
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

impl ObjectStore for InMemoryStorage {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.persistent.get(object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .persistent
            .get(object_id)
            .and_then(|obj| {
                if obj.version() == version {
                    Some(obj)
                } else {
                    None
                }
            })
            .cloned())
    }
}

impl ObjectStore for &mut InMemoryStorage {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.persistent.get(object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .persistent
            .get(object_id)
            .and_then(|obj| {
                if obj.version() == version {
                    Some(obj)
                } else {
                    None
                }
            })
            .cloned())
    }
}

impl GetModule for InMemoryStorage {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        get_module_by_id(self, id)
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
            wrapped: BTreeMap::new(),
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
        self.wrapped.remove(&id);
        self.persistent.insert(id, object);
    }

    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.persistent
    }

    pub fn wrapped(&self) -> &BTreeMap<ObjectID, VersionNumber> {
        &self.wrapped
    }

    pub fn get_wrapped(&self, id: &ObjectID) -> Option<VersionNumber> {
        self.wrapped.get(id).copied()
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
        for (id, (ver, kind)) in deleted {
            if let Some(obj) = self.persistent.remove(&id) {
                self.last_entry_for_deleted
                    .insert(id, obj.compute_object_reference());
            }
            match kind {
                DeleteKind::Wrap => {
                    self.wrapped.insert(id, ver);
                }
                DeleteKind::UnwrapThenDelete => {
                    self.wrapped.remove(&id);
                }
                _ => (),
            }
        }
    }
}
