use std::collections::HashMap;

use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::ExecutionDigests;
use sui_types::effects::TransactionEffects;
use sui_types::epoch_data::EpochData;
use sui_types::messages::VerifiedTransaction;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use sui_types::storage::get_module_by_id;
use sui_types::{
    base_types::{ObjectID, ObjectRef, VersionNumber},
    error::{SuiError, SuiResult},
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync},
};

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};


#[derive(Debug)]
pub enum SailfishMessage {
    EpochStart{conf: ProtocolConfig, data: EpochData, ref_gas_price: u64},
    EpochEnd{new_epoch_start_state: EpochStartSystemState},
    Transaction{
        tx: VerifiedTransaction, 
        exec_digest: ExecutionDigests, 
        tx_effects: TransactionEffects,
        checkpoint_seq: u64,
    }
}


pub struct MemoryBackedStore {
    pub objects: HashMap<ObjectID, (ObjectRef, Object)>,
}

impl MemoryBackedStore {
    pub fn new() -> MemoryBackedStore {
        MemoryBackedStore {
            objects: HashMap::new(),
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
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        // println!("Package: {:?}", package_id);
        Ok(self.objects.get(package_id).map(|v| v.1.clone()))
    }
}

impl ChildObjectResolver for MemoryBackedStore {
    fn read_child_object(&self, _parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        Ok(self.objects.get(child).map(|v| v.1.clone()))
    }
}

impl ObjectStore for &MemoryBackedStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.objects.get(object_id).map(|v| v.1.clone()))
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