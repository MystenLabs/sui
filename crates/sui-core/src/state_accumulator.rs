// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use mysten_metrics::monitored_scope;
use serde::Serialize;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber};
use sui_types::committee::EpochId;
use sui_types::digests::{ObjectDigest, TransactionDigest};
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::object::Object;
use sui_types::storage::{ObjectKey, ObjectStore};
use tracing::debug;
use typed_store::Map;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use fastcrypto::hash::MultisetHash;
use sui_types::accumulator::Accumulator;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, ECMHLiveObjectSetDigest};
use typed_store::rocks::TypedStoreError;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store_tables::LiveObject;
use crate::authority::AuthorityStore;

pub struct StateAccumulator {
    authority_store: Arc<AuthorityStore>,
}

pub trait AccumulatorReadStore {
    fn multi_get_object_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<Option<Object>>>;

    fn get_object_ref_prior_to_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<ObjectRef>>;
}

impl AccumulatorReadStore for AuthorityStore {
    fn multi_get_object_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<Option<Object>>> {
        self.multi_get_object_by_key(object_keys)
    }

    fn get_object_ref_prior_to_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<ObjectRef>> {
        self.get_object_ref_prior_to_key(object_id, version)
    }
}

impl AccumulatorReadStore for InMemoryStorage {
    fn multi_get_object_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<Option<Object>>> {
        let mut objects = Vec::new();
        for key in object_keys {
            objects.push(self.get_object_by_key(&key.0, key.1)?);
        }
        Ok(objects)
    }

    fn get_object_ref_prior_to_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<ObjectRef>> {
        Ok(if let Some(wrapped_version) = self.get_wrapped(object_id) {
            assert!(wrapped_version < version);
            Some((
                *object_id,
                wrapped_version,
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            ))
        } else {
            None
        })
    }
}

/// Serializable representation of the ObjectRef of an
/// object that has been wrapped
/// TODO: This can be replaced with ObjectKey.
#[derive(Serialize, Debug)]
pub struct WrappedObject {
    id: ObjectID,
    wrapped_at: SequenceNumber,
    digest: ObjectDigest,
}

impl WrappedObject {
    pub fn new(id: ObjectID, wrapped_at: SequenceNumber) -> Self {
        Self {
            id,
            wrapped_at,
            digest: ObjectDigest::OBJECT_DIGEST_WRAPPED,
        }
    }
}

pub fn accumulate_effects<T, S>(
    store: S,
    effects: Vec<TransactionEffects>,
    protocol_config: &ProtocolConfig,
) -> Accumulator
where
    S: std::ops::Deref<Target = T>,
    T: AccumulatorReadStore,
{
    let mut acc = Accumulator::default();

    // process insertions to the set
    acc.insert_all(
        effects
            .iter()
            .flat_map(|fx| {
                fx.created()
                    .iter()
                    .map(|(oref, _)| oref.2)
                    .chain(fx.unwrapped().iter().map(|(oref, _)| oref.2))
                    .chain(fx.mutated().iter().map(|(oref, _)| oref.2))
            })
            .collect::<Vec<ObjectDigest>>(),
    );

    // insert wrapped tombstones. We use a custom struct in order to contain the tombstone
    // against the object id and sequence number, as the tombstone by itself is not unique.
    acc.insert_all(
        effects
            .iter()
            .flat_map(|fx| {
                fx.wrapped()
                    .iter()
                    .map(|oref| {
                        bcs::to_bytes(&WrappedObject::new(oref.0, oref.1))
                            .unwrap()
                            .to_vec()
                    })
                    .collect::<Vec<Vec<u8>>>()
            })
            .collect::<Vec<Vec<u8>>>(),
    );

    let all_unwrapped = effects
        .iter()
        .flat_map(|fx| {
            fx.unwrapped()
                .iter()
                .map(|(oref, _owner)| (*fx.transaction_digest(), oref.0, oref.1))
        })
        .chain(effects.iter().flat_map(|fx| {
            fx.unwrapped_then_deleted()
                .iter()
                .map(|oref| (*fx.transaction_digest(), oref.0, oref.1))
        }))
        .collect::<Vec<(TransactionDigest, ObjectID, SequenceNumber)>>();

    let unwrapped_ids: HashMap<TransactionDigest, HashSet<ObjectID>> = all_unwrapped
        .iter()
        .map(|(digest, id, _)| (*digest, *id))
        .into_group_map()
        .iter()
        .map(|(digest, ids)| (*digest, HashSet::from_iter(ids.iter().cloned())))
        .collect();

    // Collect keys from modified_at_versions to remove from the accumulator.
    // Filter all unwrapped objects (from unwrapped or unwrapped_then_deleted effects)
    // as these were inserted into the accumulator as a WrappedObject. Will handle these
    // separately.
    let modified_at_version_keys: Vec<ObjectKey> = effects
        .iter()
        .flat_map(|fx| {
            fx.modified_at_versions()
                .iter()
                .map(|(id, seq_num)| (*fx.transaction_digest(), *id, *seq_num))
        })
        .filter_map(|(tx_digest, id, seq_num)| {
            // unwrapped tx
            if let Some(ids) = unwrapped_ids.get(&tx_digest) {
                // object unwrapped in this tx. We handle it later
                if ids.contains(&id) {
                    return None;
                }
            }
            Some(ObjectKey(id, seq_num))
        })
        .collect();

    let modified_at_digests: Vec<_> = store
        .multi_get_object_by_key(&modified_at_version_keys.clone())
        .expect("Failed to get modified_at_versions object from object table")
        .into_iter()
        .zip(modified_at_version_keys)
        .map(|(obj, key)| {
            obj.unwrap_or_else(|| panic!("Object for key {:?} from modified_at_versions effects does not exist in objects table", key))
            .compute_object_reference()
            .2
        })
        .collect();
    acc.remove_all(modified_at_digests);

    // Process unwrapped and unwrapped_then_deleted effects, which need to be
    // removed as WrappedObject using the last sequence number it was tombstoned
    // against. Since this happened in a past transaction, and the child object may
    // have been modified since (and hence its sequence number incremented), we
    // seek the version prior to the unwrapped version from the objects table directly.
    // If the tombstone is not found, then assume this is a newly created wrapped object hence
    // we don't expect to find it in the table.
    let wrapped_objects_to_remove: Vec<WrappedObject> = all_unwrapped
        .iter()
        .filter_map(|(_tx_digest, id, seq_num)| {
            let objref = store
                .get_object_ref_prior_to_key(id, *seq_num)
                .expect("read cannot fail");

            objref.map(|(id, version, digest)| {
                assert!(
                    !protocol_config.loaded_child_objects_fixed() || digest.is_wrapped(),
                    "{:?}",
                    id
                );
                WrappedObject::new(id, version)
            })
        })
        .collect();

    acc.remove_all(
        wrapped_objects_to_remove
            .iter()
            .map(|wrapped| bcs::to_bytes(wrapped).unwrap().to_vec())
            .collect::<Vec<Vec<u8>>>(),
    );

    acc
}

impl StateAccumulator {
    pub fn new(authority_store: Arc<AuthorityStore>) -> Self {
        Self { authority_store }
    }

    /// Accumulates the effects of a single checkpoint and persists the accumulator.
    pub fn accumulate_checkpoint(
        &self,
        effects: Vec<TransactionEffects>,
        checkpoint_seq_num: CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Accumulator> {
        let _scope = monitored_scope("AccumulateCheckpoint");
        if let Some(acc) = epoch_store.get_state_hash_for_checkpoint(&checkpoint_seq_num)? {
            return Ok(acc);
        }

        let acc = self.accumulate_effects(effects, epoch_store.protocol_config());

        epoch_store.insert_state_hash_for_checkpoint(&checkpoint_seq_num, &acc)?;
        debug!("Accumulated checkpoint {}", checkpoint_seq_num);

        epoch_store
            .checkpoint_state_notify_read
            .notify(&checkpoint_seq_num, &acc);

        Ok(acc)
    }

    /// Accumulates given effects and returns the accumulator without side effects.
    pub fn accumulate_effects(
        &self,
        effects: Vec<TransactionEffects>,
        protocol_config: &ProtocolConfig,
    ) -> Accumulator {
        accumulate_effects(&*self.authority_store, effects, protocol_config)
    }

    /// Unions all checkpoint accumulators at the end of the epoch to generate the
    /// root state hash and persists it to db. This function is idempotent. Can be called on
    /// non-consecutive epochs, e.g. to accumulate epoch 3 after having last
    /// accumulated epoch 1.
    pub async fn accumulate_epoch(
        &self,
        epoch: &EpochId,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Result<Accumulator, TypedStoreError> {
        if let Some((_checkpoint, acc)) = self
            .authority_store
            .perpetual_tables
            .root_state_hash_by_epoch
            .get(epoch)?
        {
            return Ok(acc);
        }

        // Get the next checkpoint to accumulate (first checkpoint of the epoch)
        // by adding 1 to the highest checkpoint of the previous epoch
        let (_, (next_to_accumulate, mut root_state_hash)) = self
            .authority_store
            .perpetual_tables
            .root_state_hash_by_epoch
            .iter()
            .skip_to_last()
            .next()
            .map(|(epoch, (highest, hash))| {
                (
                    epoch,
                    (
                        highest.checked_add(1).expect("Overflowed u64 for epoch ID"),
                        hash,
                    ),
                )
            })
            .unwrap_or((0, (0, Accumulator::default())));

        debug!(
            "Accumulating epoch {} from checkpoint {} to checkpoint {} (inclusive)",
            epoch, next_to_accumulate, last_checkpoint_of_epoch
        );

        let (checkpoints, mut accumulators) = epoch_store
            .get_accumulators_in_checkpoint_range(next_to_accumulate, last_checkpoint_of_epoch)?
            .into_iter()
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let remaining_checkpoints: Vec<_> = (next_to_accumulate..=last_checkpoint_of_epoch)
            .filter(|seq_num| !checkpoints.contains(seq_num))
            .collect();

        if !remaining_checkpoints.is_empty() {
            debug!(
                "Awaiting accumulation of checkpoints {:?} for epoch {} accumulation",
                remaining_checkpoints, epoch
            );
        }

        let mut remaining_accumulators = epoch_store
            .notify_read_checkpoint_state_digests(remaining_checkpoints)
            .await
            .expect("Failed to notify read checkpoint state digests");

        accumulators.append(&mut remaining_accumulators);

        assert!(accumulators.len() == (last_checkpoint_of_epoch - next_to_accumulate + 1) as usize);

        for acc in accumulators {
            root_state_hash.union(&acc);
        }

        self.authority_store
            .perpetual_tables
            .root_state_hash_by_epoch
            .insert(epoch, &(last_checkpoint_of_epoch, root_state_hash.clone()))?;

        self.authority_store
            .root_state_notify_read
            .notify(epoch, &(last_checkpoint_of_epoch, root_state_hash.clone()));

        Ok(root_state_hash)
    }

    /// Returns the result of accumulatng the live object set, without side effects
    pub fn accumulate_live_object_set(&self) -> Accumulator {
        let mut acc = Accumulator::default();
        for live_object in self.authority_store.iter_live_object_set() {
            match live_object {
                LiveObject::Normal(object) => {
                    acc.insert(object.compute_object_reference().2);
                }
                LiveObject::Wrapped(key) => {
                    acc.insert(
                        bcs::to_bytes(&WrappedObject::new(key.0, key.1))
                            .expect("Failed to serialize WrappedObject"),
                    );
                }
            }
        }
        acc
    }

    pub fn digest_live_object_set(&self) -> ECMHLiveObjectSetDigest {
        let acc = self.accumulate_live_object_set();
        acc.digest().into()
    }

    pub async fn digest_epoch(
        &self,
        epoch: &EpochId,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Result<ECMHLiveObjectSetDigest, TypedStoreError> {
        Ok(self
            .accumulate_epoch(epoch, last_checkpoint_of_epoch, epoch_store)
            .await?
            .digest()
            .into())
    }
}
