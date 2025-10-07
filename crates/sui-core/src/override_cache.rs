use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store;
use crate::execution_cache::ObjectCacheRead;
use anemo::codegen::BoxFuture;
use std::collections::HashSet;
use std::sync::Arc;
use sui_types::base_types::{FullObjectID, ObjectID, ObjectRef, SequenceNumber, VersionNumber};
use sui_types::committee::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::{
    BackingPackageStore, BackingStore, ChildObjectResolver, FullObjectKey, MarkerValue, ObjectKey,
    ObjectOrTombstone, ObjectStore, PackageObject, ParentSync,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::{
    InputObjectKind, InputObjects, ObjectReadResult, ObjectReadResultKind,
    ReceivingObjectReadResult, ReceivingObjectReadResultKind, ReceivingObjects,
};

/// A cache wrapper for the TransactionInputLoader that allows overriding specific objects.
/// Use only for dry-run style simulations.
pub struct InputLoaderCache {
    pub base: Arc<dyn ObjectCacheRead>,
    pub cache: Vec<(ObjectID, Object)>,
}

impl InputLoaderCache {
    pub fn read_objects_for_signing(
        &self,
        _tx_digest_for_caching: Option<&TransactionDigest>,
        input_object_kinds: &[InputObjectKind],
        receiving_objects: &[ObjectRef],
        epoch_id: EpochId,
    ) -> SuiResult<(InputObjects, ReceivingObjects)> {
        // Mirrors TransactionInputLoader::read_objects_for_signing but consults overrides first.
        let mut input_results = vec![None; input_object_kinds.len()];
        let mut object_refs = Vec::with_capacity(input_object_kinds.len());
        let mut fetch_indices = Vec::with_capacity(input_object_kinds.len());

        for (i, kind) in input_object_kinds.iter().enumerate() {
            match kind {
                InputObjectKind::MovePackage(id) => {
                    let Some(package) = self.base.get_package_object(id)?.map(|o| o.into()) else {
                        return Err(SuiError::from(kind.object_not_found_error()));
                    };
                    input_results[i] = Some(ObjectReadResult {
                        input_object_kind: *kind,
                        object: ObjectReadResultKind::Object(package),
                    });
                }
                InputObjectKind::SharedMoveObject { .. } => {
                    let input_full_id = kind.full_object_id();
                    match self.base.get_object(&kind.object_id()) {
                        Some(object) if object.full_id() == input_full_id => {
                            input_results[i] = Some(ObjectReadResult::new(*kind, object.into()))
                        }
                        _ => {
                            if let Some((version, digest)) = self
                                .base
                                .get_last_consensus_stream_end_info(input_full_id, epoch_id)
                            {
                                input_results[i] = Some(ObjectReadResult {
                                    input_object_kind: *kind,
                                    object: ObjectReadResultKind::ObjectConsensusStreamEnded(
                                        version, digest,
                                    ),
                                });
                            } else {
                                return Err(SuiError::from(kind.object_not_found_error()));
                            }
                        }
                    }
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    object_refs.push(*objref);
                    fetch_indices.push(i);
                }
            }
        }

        let objects = self
            .base
            .multi_get_objects_with_more_accurate_error_return(&object_refs)?;
        assert_eq!(objects.len(), object_refs.len());
        for (index, object) in fetch_indices.into_iter().zip(objects.into_iter()) {
            input_results[index] = Some(ObjectReadResult {
                input_object_kind: input_object_kinds[index],
                object: ObjectReadResultKind::Object(object),
            });
        }

        let receiving_results =
            self.read_receiving_objects_for_signing(receiving_objects, epoch_id)?;
        Ok((
            input_results
                .into_iter()
                .map(Option::unwrap)
                .collect::<Vec<_>>()
                .into(),
            receiving_results,
        ))
    }

    fn read_receiving_objects_for_signing(
        &self,
        receiving_objects: &[ObjectRef],
        epoch_id: EpochId,
    ) -> SuiResult<ReceivingObjects> {
        let mut receiving_results = Vec::with_capacity(receiving_objects.len());
        for objref in receiving_objects {
            let (object_id, version, _) = objref;
            let full_object_id = FullObjectID::new(*object_id, Some(*version));
            if self.base.have_received_object_at_version(
                FullObjectKey::new(full_object_id, *version),
                epoch_id,
            ) {
                receiving_results.push(ReceivingObjectReadResult::new(
                    *objref,
                    ReceivingObjectReadResultKind::PreviouslyReceivedObject,
                ));
                continue;
            }

            let Some(object) = self.base.get_object(object_id) else {
                return Err(UserInputError::ObjectNotFound {
                    object_id: *object_id,
                    version: Some(*version),
                }
                .into());
            };

            receiving_results.push(ReceivingObjectReadResult::new(*objref, object.into()));
        }
        Ok(receiving_results.into())
    }
}

impl ObjectCacheRead for InputLoaderCache {
    fn get_package_object(&self, id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        // check overrides first
        for (cache_id, obj) in &self.cache {
            if obj.is_package() && cache_id == id {
                return Ok(Some(PackageObject::new(obj.clone())));
            }
        }
        self.base.get_package_object(id)
    }

    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]) {
        self.base.force_reload_system_packages(system_package_ids);
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        for (cache_id, obj) in &self.cache {
            if cache_id == id {
                return Some(obj.clone());
            }
        }
        self.base.get_object(id)
    }

    fn get_latest_object_ref_or_tombstone(&self, object_id: ObjectID) -> Option<ObjectRef> {
        self.base.get_latest_object_ref_or_tombstone(object_id)
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Option<(ObjectKey, ObjectOrTombstone)> {
        self.base.get_latest_object_or_tombstone(object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        for (cache_id, obj) in &self.cache {
            if cache_id == object_id && obj.version() == version {
                return Some(obj.clone());
            }
        }
        self.base.get_object_by_key(object_id, version)
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>> {
        let mut results = Vec::with_capacity(object_keys.len());
        for object_key in object_keys {
            let object_id = &object_key.0;
            let version = object_key.1;
            let mut found = false;
            for (cache_id, obj) in &self.cache {
                if cache_id == object_id && obj.version() == version {
                    results.push(Some(obj.clone()));
                    found = true;
                    break;
                }
            }
            if !found {
                results.push(self.base.get_object_by_key(object_id, version));
            }
        }
        results
    }

    fn object_exists_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> bool {
        for (cache_id, obj) in &self.cache {
            if cache_id == object_id && obj.version() == version {
                return true;
            }
        }
        self.base.object_exists_by_key(object_id, version)
    }

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> Vec<bool> {
        self.base.multi_object_exists_by_key(object_keys)
    }

    fn multi_input_objects_available_cache_only(
        &self,
        keys: &[sui_types::storage::InputKey],
    ) -> Vec<bool> {
        self.base.multi_input_objects_available_cache_only(keys)
    }

    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        self.base.find_object_lt_or_eq_version(object_id, version)
    }

    fn get_lock(
        &self,
        obj_ref: ObjectRef,
        epoch_store: &AuthorityPerEpochStore,
    ) -> authority_store::SuiLockResult {
        self.base.get_lock(obj_ref, epoch_store)
    }

    fn _get_live_objref(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        for (cache_id, obj) in &self.cache {
            if cache_id == &object_id {
                return Ok((object_id, obj.version(), obj.digest()));
            }
        }
        self.base._get_live_objref(object_id)
    }

    fn check_owned_objects_are_live(&self, owned_object_refs: &[ObjectRef]) -> SuiResult {
        self.base.check_owned_objects_are_live(owned_object_refs)
    }

    fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState> {
        self.base.get_sui_system_state_object_unsafe()
    }

    fn get_bridge_object_unsafe(&self) -> SuiResult<sui_types::bridge::Bridge> {
        self.base.get_bridge_object_unsafe()
    }

    fn get_marker_value(
        &self,
        object_key: FullObjectKey,
        epoch_id: EpochId,
    ) -> Option<MarkerValue> {
        self.base.get_marker_value(object_key, epoch_id)
    }

    fn get_latest_marker(
        &self,
        object_id: FullObjectID,
        epoch_id: EpochId,
    ) -> Option<(SequenceNumber, MarkerValue)> {
        self.base.get_latest_marker(object_id, epoch_id)
    }

    fn get_highest_pruned_checkpoint(&self) -> Option<CheckpointSequenceNumber> {
        self.base.get_highest_pruned_checkpoint()
    }

    fn notify_read_input_objects<'a>(
        &'a self,
        input_and_receiving_keys: &'a [sui_types::storage::InputKey],
        receiving_keys: &'a HashSet<sui_types::storage::InputKey>,
        epoch: EpochId,
    ) -> BoxFuture<'a, ()> {
        self.base
            .notify_read_input_objects(input_and_receiving_keys, receiving_keys, epoch)
    }
}

/// A BackingStore wrapper that overlays specific objects for read paths.
pub struct ObjectCache {
    pub inner: Arc<dyn BackingStore + Send + Sync>,
    pub cache: Vec<(ObjectID, Object)>,
}

impl ObjectCache {
    pub fn new(inner: Arc<dyn BackingStore + Send + Sync>, cache: Vec<(ObjectID, Object)>) -> Self {
        Self { inner, cache }
    }
}

impl BackingPackageStore for ObjectCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        for (id, obj) in &self.cache {
            if id == package_id {
                return Ok(Some(PackageObject::new(obj.clone())));
            }
        }
        self.inner.get_package_object(package_id)
    }
}

impl ChildObjectResolver for ObjectCache {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        self.inner
            .read_child_object(parent, child, child_version_upper_bound)
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        self.inner.get_object_received_at_version(
            owner,
            receiving_object_id,
            receive_object_at_version,
            epoch_id,
        )
    }
}

impl ObjectStore for ObjectCache {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        for (id, obj) in &self.cache {
            if id == object_id {
                return Some(obj.clone());
            }
        }
        self.inner.get_object(object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        for (id, obj) in &self.cache {
            if id == object_id && obj.version() == version {
                return Some(obj.clone());
            }
        }
        self.inner.get_object_by_key(object_id, version)
    }
}

impl ParentSync for ObjectCache {
    fn get_latest_parent_entry_ref_deprecated(&self, object_id: ObjectID) -> Option<ObjectRef> {
        self.inner.get_latest_parent_entry_ref_deprecated(object_id)
    }
}
