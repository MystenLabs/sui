use std::collections::HashMap;
use std::sync::Mutex;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber},
    error::{SuiError, SuiResult},
    object::{Object, Owner},
    storage::{
        get_module_by_id, BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync,
    },
};

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};

use crate::storage::WritableObjectStore;

#[derive(Debug)]
pub struct MutexedMemoryBackedStore {
    pub objects: Mutex<HashMap<ObjectID, (ObjectRef, Object)>>,
}

impl MutexedMemoryBackedStore {
    pub fn new() -> MutexedMemoryBackedStore {
        MutexedMemoryBackedStore {
            objects: Mutex::new(HashMap::new()),
        }
    }
}

impl ObjectStore for MutexedMemoryBackedStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects
            .lock()
            .unwrap()
            .get(object_id)
            .map(|v| v.1.clone()))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects
            .lock()
            .unwrap()
            .get(object_id)
            .and_then(|obj| {
                if obj.1.version() == version {
                    Some(obj.1.clone())
                } else {
                    None
                }
            })
            .clone())
    }
}

impl WritableObjectStore for MutexedMemoryBackedStore {
    fn insert(&self, k: ObjectID, v: (ObjectRef, Object)) -> Option<(ObjectRef, Object)> {
        self.objects.lock().unwrap().insert(k, v)
    }

    fn remove(&self, k: ObjectID) -> Option<(ObjectRef, Object)> {
        self.objects.lock().unwrap().remove(&k)
    }
}

impl ParentSync for MutexedMemoryBackedStore {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        // println!("Parent: {:?}", object_id);
        Ok(self.objects.lock().unwrap().get(&object_id).map(|v| v.0))
    }
}

impl BackingPackageStore for MutexedMemoryBackedStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        // println!("Package: {:?}", package_id);
        Ok(self
            .objects
            .lock()
            .unwrap()
            .get(package_id)
            .map(|v| v.1.clone()))
    }
}

impl ChildObjectResolver for MutexedMemoryBackedStore {
    // fn read_child_object(&self, _parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
    //     Ok(self.objects
    //         .lock()
    //         .unwrap()
    //         .get(child).map(|v| v.1.clone()))
    // }

    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let child_object = match self.objects.lock().unwrap().get(child).map(|v| v.1.clone()) {
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
        if child_object.version() > child_version_upper_bound {
            return Err(SuiError::UnsupportedFeatureError {
                error: "TODO InMemoryStorage::read_child_object does not yet support bounded reads"
                    .to_owned(),
            });
        }
        Ok(Some(child_object))
    }
}

impl ObjectStore for &MutexedMemoryBackedStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects
            .lock()
            .unwrap()
            .get(object_id)
            .map(|v| v.1.clone()))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects
            .lock()
            .unwrap()
            .get(object_id)
            .and_then(|obj| {
                if obj.1.version() == version {
                    Some(obj.1.clone())
                } else {
                    None
                }
            })
            .clone())
    }
}

impl ModuleResolver for MutexedMemoryBackedStore {
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

impl GetModule for MutexedMemoryBackedStore {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        get_module_by_id(self, id)
    }
}
