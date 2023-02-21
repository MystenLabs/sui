use std::collections::HashMap;

use sui_types::{base_types::{ObjectID, ObjectRef}, object::Object, storage::{ParentSync, BackingPackageStore, ChildObjectResolver}, error::SuiResult};

pub struct MemoryBackedStore {
    pub objects : HashMap<ObjectID, (ObjectRef, Object)>    
}

impl MemoryBackedStore {
    pub fn new() -> MemoryBackedStore {
        MemoryBackedStore {
            objects : HashMap::new()
        }

    }
}

impl ParentSync for MemoryBackedStore {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        // println!("Parent: {:?}", object_id);
        Ok(self.objects.get(&object_id).map(|v| v.0))
    }
}

impl BackingPackageStore for MemoryBackedStore {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        // println!("Package: {:?}", package_id);
        Ok(self.objects.get(package_id).map(|v| v.1.clone()))
    }
}

impl ChildObjectResolver for MemoryBackedStore { 
    fn read_child_object(&self, _parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        Ok(self.objects.get(child).map(|v| v.1.clone()))
    }
}