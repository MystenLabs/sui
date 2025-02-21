// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use once_cell::unsync::OnceCell;
use prometheus::core::{Atomic, AtomicU64};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::epoch_start_configuration::EpochStartConfigTrait;
use sui_storage::package_object_cache::PackageObjectCache;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber, VersionNumber};
use sui_types::error::{SuiError, SuiResult};
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::object::{Object, Owner};
use sui_types::storage::{
    get_module_by_id, BackingPackageStore, ChildObjectResolver, ObjectStore, PackageObject,
    ParentSync,
};
use sui_types::transaction::{InputObjectKind, InputObjects, ObjectReadResult, TransactionKey};

#[derive(Clone)]
pub(crate) struct InMemoryObjectStore {
    objects: Arc<RwLock<HashMap<ObjectID, Object>>>,
    package_cache: Arc<PackageObjectCache>,
    num_object_reads: Arc<AtomicU64>,
}

impl InMemoryObjectStore {
    pub(crate) fn new(objects: HashMap<ObjectID, Object>) -> Self {
        Self {
            objects: Arc::new(RwLock::new(objects)),
            package_cache: PackageObjectCache::new(),
            num_object_reads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn get_num_object_reads(&self) -> u64 {
        self.num_object_reads.get()
    }

    // TODO: This function is out-of-sync with read_objects_for_execution from transaction_input_loader.rs.
    // For instance, it does not support the use of deleted shared objects.
    // We will need a trait to unify the these functions. (similarly the one in simulacrum)
    pub(crate) fn read_objects_for_execution(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx_key: &TransactionKey,
        input_object_kinds: &[InputObjectKind],
    ) -> SuiResult<InputObjects> {
        let shared_version_assignments_cell: OnceCell<Option<HashMap<_, _>>> = OnceCell::new();
        let mut input_objects = Vec::new();
        for kind in input_object_kinds {
            let obj: Option<Object> = match kind {
                InputObjectKind::MovePackage(id) => self.get_package_object(id)?.map(|o| o.into()),
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)
                }

                InputObjectKind::SharedMoveObject {
                    id,
                    initial_shared_version,
                    ..
                } => {
                    let shared_version_assignments = shared_version_assignments_cell
                        .get_or_init(|| {
                            epoch_store
                                .get_assigned_shared_object_versions(tx_key)
                                .map(|l| l.into_iter().collect())
                        })
                        .as_ref()
                        .ok_or_else(|| SuiError::GenericAuthorityError {
                            error: "Shared object versions should have been assigned.".to_string(),
                        })?;
                    let initial_shared_version = if epoch_store
                        .epoch_start_config()
                        .use_version_assignment_tables_v3()
                    {
                        *initial_shared_version
                    } else {
                        // (before ConsensusV2 objects, we didn't track initial shared
                        // version for shared object locks)
                        SequenceNumber::UNKNOWN
                    };
                    let version = shared_version_assignments.get(&(*id, initial_shared_version)).unwrap_or_else(|| {
                        panic!("Shared object version should have been assigned. key: {tx_key:?}, obj id: {id:?}")
                    });

                    self.get_object_by_key(id, *version)
                }
            };

            input_objects.push(ObjectReadResult::new(
                *kind,
                obj.ok_or_else(|| kind.object_not_found_error())?.into(),
            ));
        }

        Ok(input_objects.into())
    }

    pub(crate) fn commit_objects(&self, inner_temp_store: InnerTemporaryStore) {
        let mut objects = self.objects.write().unwrap();
        for (object_id, _) in inner_temp_store.mutable_inputs {
            if !inner_temp_store.written.contains_key(&object_id) {
                objects.remove(&object_id);
            }
        }
        for (object_id, object) in inner_temp_store.written {
            objects.insert(object_id, object);
        }
    }
}

impl ObjectStore for InMemoryObjectStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.num_object_reads.inc_by(1);
        self.objects.read().unwrap().get(object_id).cloned()
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
        // TODO: Delete this parameter once table migration is complete.
        _use_object_per_epoch_marker_table_v2: bool,
    ) -> SuiResult<Option<Object>> {
        unimplemented!()
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
        unreachable!()
    }
}
