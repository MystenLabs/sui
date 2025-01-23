// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::VersionNumber;
use crate::committee::EpochId;
use crate::inner_temporary_store::WrittenObjects;
use crate::storage::{
    get_module, get_module_by_id, load_package_object_from_object_store, PackageObject,
};
use crate::transaction::TransactionDataAPI;
use crate::transaction::{InputObjectKind, InputObjects, ObjectReadResult, Transaction};
use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    error::{SuiError, SuiResult},
    object::{Object, Owner},
    storage::{BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync},
};
use better_any::{Tid, TidAble};
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use std::collections::BTreeMap;

// TODO: We should use AuthorityTemporaryStore instead.
// Keeping this functionally identical to AuthorityTemporaryStore is a pain.
#[derive(Debug, Default, Tid)]
pub struct InMemoryStorage {
    persistent: BTreeMap<ObjectID, Object>,
}

impl BackingPackageStore for InMemoryStorage {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for InMemoryStorage {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let child_object = match self.persistent.get(child).cloned() {
            None => return Ok(None),
            Some(obj) => obj,
        };
        let parent = *parent;
        if child_object.owner != Owner::ObjectOwner(parent.into()) {
            return Err(SuiError::InvalidChildObjectAccess {
                object: *child,
                given_parent: parent,
                actual_owner: child_object.owner.clone(),
            });
        }
        if child_object.version() > child_version_upper_bound {
            return Err(SuiError::UnsupportedFeatureError {
                error: "TODO InMemoryStorage::read_child_object does not yet support bounded reads"
                    .to_owned(),
            });
        }
        Ok(Some(child_object))
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        _use_object_per_epoch_marker_table_v2: bool,
    ) -> SuiResult<Option<Object>> {
        let recv_object = match self.persistent.get(receiving_object_id).cloned() {
            None => return Ok(None),
            Some(obj) => obj,
        };
        if recv_object.owner != Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }

        if recv_object.version() != receive_object_at_version {
            return Ok(None);
        }
        Ok(Some(recv_object))
    }
}

impl ParentSync for InMemoryStorage {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        unreachable!("Should not be called for InMemoryStorage as it's deprecated.")
    }
}

impl ModuleResolver for InMemoryStorage {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        get_module(self, module_id)
    }
}

impl ModuleResolver for &mut InMemoryStorage {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        (**self).get_module(module_id)
    }
}

impl ObjectStore for InMemoryStorage {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.persistent.get(object_id).cloned()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.persistent
            .get(object_id)
            .and_then(|obj| {
                if obj.version() == version {
                    Some(obj)
                } else {
                    None
                }
            })
            .cloned()
    }
}

impl ObjectStore for &mut InMemoryStorage {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.persistent.get(object_id).cloned()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.persistent
            .get(object_id)
            .and_then(|obj| {
                if obj.version() == version {
                    Some(obj)
                } else {
                    None
                }
            })
            .cloned()
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
        Self { persistent }
    }

    pub fn read_input_objects_for_transaction(&self, transaction: &Transaction) -> InputObjects {
        let mut input_objects = Vec::new();
        for kind in transaction.transaction_data().input_objects().unwrap() {
            let obj: Object = match kind {
                InputObjectKind::MovePackage(id)
                | InputObjectKind::ImmOrOwnedMoveObject((id, _, _))
                | InputObjectKind::SharedMoveObject { id, .. } => {
                    self.get_object(&id).unwrap().clone()
                }
            };

            input_objects.push(ObjectReadResult::new(kind, obj.into()));
        }
        input_objects.into()
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
        self.persistent.insert(id, object);
    }

    pub fn remove_object(&mut self, object_id: ObjectID) -> Option<Object> {
        self.persistent.remove(&object_id)
    }

    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.persistent
    }

    pub fn into_inner(self) -> BTreeMap<ObjectID, Object> {
        self.persistent
    }

    pub fn finish(&mut self, written: WrittenObjects) {
        for (_id, new_object) in written {
            debug_assert!(new_object.id() == _id);
            self.insert_object(new_object);
        }
    }
}
