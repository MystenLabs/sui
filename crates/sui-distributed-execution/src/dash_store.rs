use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber},
    digests::TransactionDigest,
    error::{SuiError, SuiResult},
    object::{Object, Owner},
    storage::{
        get_module_by_id, BackingPackageStore, ChildObjectResolver, GetSharedLocks, ObjectStore,
        ParentSync,
    },
};

use anyhow::Result;
use dashmap::DashMap;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;

use crate::types::WritableObjectStore;

#[derive(Debug)]
pub struct DashMemoryBackedStore {
    pub objects: DashMap<ObjectID, Object>,
}

impl DashMemoryBackedStore {
    pub fn new() -> DashMemoryBackedStore {
        DashMemoryBackedStore {
            objects: DashMap::new(),
        }
    }
}

impl ObjectStore for DashMemoryBackedStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.objects.get(object_id).map(|v| v.clone()))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects
            .get(object_id)
            .and_then(|obj| {
                if obj.version() == version {
                    Some(obj.clone())
                } else {
                    None
                }
            })
            .clone())
    }
}

impl WritableObjectStore for DashMemoryBackedStore {
    fn insert(&self, k: ObjectID, v: (ObjectRef, Object)) -> Option<(ObjectRef, Object)> {
        self.objects
            .insert(k, v.1)
            .map(|v| (v.compute_object_reference(), v))
    }

    fn remove(&self, k: ObjectID) -> Option<(ObjectRef, Object)> {
        let (_, obj) = self.objects.remove(&k).unwrap();
        Some((obj.compute_object_reference(), obj))
    }
}

impl ParentSync for DashMemoryBackedStore {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        Ok(self
            .objects
            .get(&object_id)
            .map(|v| v.compute_object_reference()))
    }
}

impl BackingPackageStore for DashMemoryBackedStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        Ok(self.get_object(package_id).unwrap().and_then(|o| {
            if o.is_package() {
                Some(o.clone())
            } else {
                None
            }
        }))
    }
}

impl ChildObjectResolver for DashMemoryBackedStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object(child).unwrap().and_then(|o| {
            if o.version() <= child_version_upper_bound
                && o.owner == Owner::ObjectOwner((*parent).into())
            {
                Some(o.clone())
            } else {
                None
            }
        }))
    }
}

impl ObjectStore for &DashMemoryBackedStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.objects.get(object_id).map(|v| v.clone()))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects
            .get(object_id)
            .and_then(|obj| {
                if obj.version() == version {
                    Some(obj.clone())
                } else {
                    None
                }
            })
            .clone())
    }
}

impl GetModule for DashMemoryBackedStore {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        get_module_by_id(self, id)
    }
}

impl GetSharedLocks for DashMemoryBackedStore {
    fn get_shared_locks(
        &self,
        _transaction_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        unreachable!()
    }
}
