// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use parking_lot::RwLock;
use prometheus::core::{Atomic, AtomicU64};
use std::collections::HashMap;
use std::sync::Arc;
use sui_storage::package_object_cache::PackageObjectCache;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber, VersionNumber};
use sui_types::error::{SuiError, SuiResult};
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::object::{Object, Owner};
use sui_types::storage::{
    get_module_by_id, BackingPackageStore, ChildObjectResolver, ObjectStore, PackageObject,
    ParentSync,
};
use sui_types::transaction::{InputObjectKind, InputObjects, ObjectReadResult};

#[derive(Clone)]
pub struct InMemoryObjectStore {
    objects: Arc<RwLock<HashMap<ObjectID, Object>>>,
    package_cache: Arc<PackageObjectCache>,
    num_object_reads: Arc<AtomicU64>,
}

impl InMemoryObjectStore {
    pub fn new(objects: HashMap<ObjectID, Object>) -> Self {
        Self {
            objects: Arc::new(RwLock::new(objects)),
            package_cache: PackageObjectCache::new(),
            num_object_reads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn empty() -> Self {
        Self::new(HashMap::new())
    }

    pub fn new_with_genesis_packages(objects: HashMap<ObjectID, Object>) -> Self {
        let mut all_objects = objects;
        for genesis_object in crate::genesis_packages() {
            all_objects.insert(genesis_object.id(), genesis_object);
        }
        Self::new(all_objects)
    }

    pub fn new_genesis() -> Self {
        Self::new_with_genesis_packages(HashMap::new())
    }

    pub fn get_num_object_reads(&self) -> u64 {
        self.num_object_reads.get()
    }

    pub fn commit_objects(&self, inner_temp_store: InnerTemporaryStore) {
        let mut objects = self.objects.write();
        for (object_id, _) in inner_temp_store.mutable_inputs {
            if !inner_temp_store.written.contains_key(&object_id) {
                objects.remove(&object_id);
            }
        }
        for (object_id, object) in inner_temp_store.written {
            objects.insert(object_id, object);
        }
    }

    pub fn insert_object(&self, object: Object) {
        let mut objects = self.objects.write();
        objects.insert(object.id(), object);
    }

    pub fn insert_objects(&self, new_objects: impl IntoIterator<Item = Object>) {
        let mut objects = self.objects.write();
        for object in new_objects {
            objects.insert(object.id(), object);
        }
    }

    pub fn read_input_objects(&self, input_object_kinds: &[InputObjectKind]) -> SuiResult<InputObjects> {
        let mut input_objects = Vec::new();
        for kind in input_object_kinds {
            let obj: Option<Object> = match kind {
                InputObjectKind::MovePackage(id) => {
                    self.get_package_object(id)?.map(|o| o.into())
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)
                }
                InputObjectKind::SharedMoveObject { .. } => {
                    return Err(SuiError::UserInputError {
                        error: sui_types::error::UserInputError::Unsupported(
                            "Shared objects not supported in minimal executor yet".to_string()
                        ),
                    });
                }
            };

            input_objects.push(ObjectReadResult::new(
                *kind,
                obj.ok_or_else(|| kind.object_not_found_error())?.into(),
            ));
        }

        Ok(input_objects.into())
    }
}

impl ObjectStore for InMemoryObjectStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.num_object_reads.inc_by(1);
        self.objects.read().get(object_id).cloned()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.get_object(object_id).and_then(|o| {
            if o.version() == version {
                Some(o.clone())
            } else {
                None
            }
        })
    }
}

impl BackingPackageStore for InMemoryObjectStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        self.package_cache.get_package_object(package_id, self)
    }
}

impl ChildObjectResolver for InMemoryObjectStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object(child).and_then(|o| {
            if o.version() <= child_version_upper_bound
                && o.owner == Owner::ObjectOwner((*parent).into())
            {
                Some(o.clone())
            } else {
                None
            }
        }))
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        unimplemented!("get_object_received_at_version not needed for basic execution")
    }
}

impl GetModule for InMemoryObjectStore {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        get_module_by_id(self, id)
    }
}

impl ParentSync for InMemoryObjectStore {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        unimplemented!("get_latest_parent_entry_ref_deprecated not needed for basic execution")
    }
}
