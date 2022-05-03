// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    error::SuiResult,
    object::Object,
    storage::{BackingPackageStore, DeleteKind},
};

// TODO: We should use AuthorityTemporaryStore instead.
// Keeping this functionally identical to AuthorityTemporaryStore is a pain.
#[derive(Default, Debug)]
pub struct InMemoryStorage {
    persistent: BTreeMap<ObjectID, Object>,
}

impl BackingPackageStore for InMemoryStorage {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        Ok(self.persistent.get(package_id).cloned())
    }
}

impl InMemoryStorage {
    pub fn new(objects: Vec<Object>) -> Self {
        let mut persistent = BTreeMap::new();
        for o in objects {
            persistent.insert(o.id(), o);
        }
        Self { persistent }
    }

    pub fn get_object(&self, id: &ObjectID) -> Option<&Object> {
        self.persistent.get(id)
    }

    pub fn insert_object(&mut self, object: Object) {
        self.persistent.insert(object.id(), object);
    }

    pub fn finish(
        &mut self,
        written: BTreeMap<ObjectID, (ObjectRef, Object)>,
        deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    ) {
        debug_assert!(written.keys().all(|id| !deleted.contains_key(id)));
        for (_id, (_, new_object)) in written {
            debug_assert!(new_object.id() == _id);
            self.insert_object(new_object);
        }
        for (id, _) in deleted {
            let obj_opt = self.persistent.remove(&id);
            assert!(obj_opt.is_none())
        }
    }
}
