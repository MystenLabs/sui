use std::collections::HashMap;
use std::cell::RefCell;
use sui_types::{
    base_types::{ObjectID, ObjectRef, VersionNumber},
    error::{SuiError, SuiResult},
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync, get_module_by_id},
};

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};

use super::storage::*;


pub struct MemoryBackedStore {
    pub objects: RefCell<HashMap<ObjectID, (ObjectRef, Object)>>,
}

// To satisfy sync requirement of ew_state.execute_tx(). This is ok as simple store is
// strictly for use in single-threaded setting
unsafe impl Sync for MemoryBackedStore {}

impl MemoryBackedStore {
    pub fn new() -> MemoryBackedStore {
        MemoryBackedStore {
            objects: RefCell::new(HashMap::new()),
        }
    }
}

impl ObjectStore for MemoryBackedStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.objects.borrow().get(object_id).map(|v| v.1.clone()))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self.objects
            .borrow()
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

impl WritableObjectStore for MemoryBackedStore {
    fn insert(&self, k: ObjectID, v: (ObjectRef, Object)) -> Option<(ObjectRef, Object)> {
        self.objects.borrow_mut().insert(k, v)
    }

    fn remove(&self, k: ObjectID) -> Option<(ObjectRef, Object)> {
        self.objects.borrow_mut().remove(&k)
    }
}

impl ParentSync for MemoryBackedStore {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        // println!("Parent: {:?}", object_id);
        Ok(self.objects.borrow().get(&object_id).map(|v| v.0))
    }
}

impl BackingPackageStore for MemoryBackedStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        // println!("Package: {:?}", package_id);
        Ok(self.objects.borrow().get(package_id).map(|v| v.1.clone()))
    }
}

impl ChildObjectResolver for MemoryBackedStore {
    fn read_child_object(&self, _parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        Ok(self.objects.borrow().get(child).map(|v| v.1.clone()))
    }
}

impl ObjectStore for &MemoryBackedStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.objects.borrow().get(object_id).map(|v| v.1.clone()))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects.borrow()
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

impl ModuleResolver for MemoryBackedStore {
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

impl GetModule for MemoryBackedStore {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        get_module_by_id(self, id)
    }
}