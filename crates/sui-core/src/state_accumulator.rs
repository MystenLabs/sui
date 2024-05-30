// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use mysten_metrics::monitored_scope;
use parking_lot::Mutex;
use serde::Serialize;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber};
use sui_types::committee::EpochId;
use sui_types::digests::{ObjectDigest, TransactionDigest};
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::storage::{ObjectKey, ObjectStore};
use tracing::debug;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use fastcrypto::hash::MultisetHash;
use sui_types::accumulator::Accumulator;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::{
    CheckpointSequenceNumber, ECMHLiveObjectSetDigest, VerifiedCheckpoint,
};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store_tables::LiveObject;

pub struct StateAccumulator {
    store: Arc<dyn AccumulatorStore>,
    /// Running root accumulator for the epoch. This is used to amortize the
    /// cost of accumulating the root state hash across multiple checkpoints.
    /// Note that, as it is only representative of all checkpoints in the current
    /// epoch, it must be unioned with the previous epoch's root state accumulator
    /// at end of epoch.
    running_root_accumulator: Mutex<Option<Accumulator>>,
}

pub trait AccumulatorStore: ObjectStore + Send + Sync {
    /// This function is only called in older protocol versions, and should no longer be used.
    /// It creates an explicit dependency to tombstones which is not desired.
    fn get_object_ref_prior_to_key_deprecated(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<ObjectRef>>;

    fn get_root_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>>;

    fn get_root_state_accumulator_for_highest_epoch(
        &self,
    ) -> SuiResult<Option<(EpochId, (CheckpointSequenceNumber, Accumulator))>>;

    fn insert_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
        checkpoint_seq_num: &CheckpointSequenceNumber,
        acc: &Accumulator,
    ) -> SuiResult;

    fn iter_live_object_set(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = LiveObject> + '_>;

    fn iter_cached_live_object_set_for_testing(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = LiveObject> + '_> {
        self.iter_live_object_set(include_wrapped_tombstone)
    }
}

impl AccumulatorStore for InMemoryStorage {
    fn get_object_ref_prior_to_key_deprecated(
        &self,
        _object_id: &ObjectID,
        _version: VersionNumber,
    ) -> SuiResult<Option<ObjectRef>> {
        unreachable!("get_object_ref_prior_to_key is only called by accumulate_effects_v1, while InMemoryStorage is used by testing and genesis only, which always uses latest protocol ")
    }

    fn get_root_state_accumulator_for_epoch(
        &self,
        _epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>> {
        unreachable!("not used for testing")
    }

    fn get_root_state_accumulator_for_highest_epoch(
        &self,
    ) -> SuiResult<Option<(EpochId, (CheckpointSequenceNumber, Accumulator))>> {
        unreachable!("not used for testing")
    }

    fn insert_state_accumulator_for_epoch(
        &self,
        _epoch: EpochId,
        _checkpoint_seq_num: &CheckpointSequenceNumber,
        _acc: &Accumulator,
    ) -> SuiResult {
        unreachable!("not used for testing")
    }

    fn iter_live_object_set(
        &self,
        _include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = LiveObject> + '_> {
        unreachable!("not used for testing")
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
    T: AccumulatorStore + ?Sized,
{
    if protocol_config.enable_effects_v2() {
        accumulate_effects_v3(effects)
    } else if protocol_config.simplified_unwrap_then_delete() {
        accumulate_effects_v2(store, effects)
    } else {
        accumulate_effects_v1(store, effects, protocol_config)
    }
}

fn accumulate_effects_v1<T, S>(
    store: S,
    effects: Vec<TransactionEffects>,
    protocol_config: &ProtocolConfig,
) -> Accumulator
where
    S: std::ops::Deref<Target = T>,
    T: AccumulatorStore + ?Sized,
{
    let mut acc = Accumulator::default();

    // process insertions to the set
    acc.insert_all(
        effects
            .iter()
            .flat_map(|fx| {
                fx.all_changed_objects()
                    .into_iter()
                    .map(|(oref, _, _)| oref.2)
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
                .into_iter()
                .map(|(oref, _owner)| (*fx.transaction_digest(), oref.0, oref.1))
        })
        .chain(effects.iter().flat_map(|fx| {
            fx.unwrapped_then_deleted()
                .into_iter()
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
                .into_iter()
                .map(|(id, seq_num)| (*fx.transaction_digest(), id, seq_num))
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
        .multi_get_objects_by_key(&modified_at_version_keys.clone())
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
                .get_object_ref_prior_to_key_deprecated(id, *seq_num)
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

fn accumulate_effects_v2<T, S>(store: S, effects: Vec<TransactionEffects>) -> Accumulator
where
    S: std::ops::Deref<Target = T>,
    T: AccumulatorStore + ?Sized,
{
    let mut acc = Accumulator::default();

    // process insertions to the set
    acc.insert_all(
        effects
            .iter()
            .flat_map(|fx| {
                fx.all_changed_objects()
                    .into_iter()
                    .map(|(oref, _, _)| oref.2)
            })
            .collect::<Vec<ObjectDigest>>(),
    );

    // Collect keys from modified_at_versions to remove from the accumulator.
    let modified_at_version_keys: Vec<_> = effects
        .iter()
        .flat_map(|fx| {
            fx.modified_at_versions()
                .into_iter()
                .map(|(id, version)| ObjectKey(id, version))
        })
        .collect();

    let modified_at_digests: Vec<_> = store
        .multi_get_objects_by_key(&modified_at_version_keys.clone())
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

    acc
}

fn accumulate_effects_v3(effects: Vec<TransactionEffects>) -> Accumulator {
    let mut acc = Accumulator::default();

    // process insertions to the set
    acc.insert_all(
        effects
            .iter()
            .flat_map(|fx| {
                fx.all_changed_objects()
                    .into_iter()
                    .map(|(object_ref, _, _)| object_ref.2)
            })
            .collect::<Vec<ObjectDigest>>(),
    );

    // process modified objects to the set
    acc.remove_all(
        effects
            .iter()
            .flat_map(|fx| {
                fx.old_object_metadata()
                    .into_iter()
                    .map(|(object_ref, _owner)| object_ref.2)
            })
            .collect::<Vec<ObjectDigest>>(),
    );

    acc
}

impl StateAccumulator {
    pub fn new(
        store: Arc<dyn AccumulatorStore>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Self {
        Self {
            store,
            running_root_accumulator: Mutex::new(
                epoch_store
                    .get_running_root_accumulator()
                    .unwrap_or_default(),
            ),
        }
    }

    /// Accumulates the effects of a single checkpoint and persists the accumulator.
    /// Requires a VerifiedCheckpoint as a means to enforce that it is never called
    /// by checkpoint builder (for end of epoch checkpoint, use accumulate_final_checkpoint).
    pub fn accumulate_checkpoint(
        &self,
        effects: Vec<TransactionEffects>,
        checkpoint: &VerifiedCheckpoint,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Accumulator> {
        let _scope = monitored_scope("AccumulateCheckpoint");
        let checkpoint_seq_num = checkpoint.sequence_number();
        self.accumulate_checkpoint_impl(effects, checkpoint_seq_num, epoch_store)
    }

    /// Accumulates the effects of a final checkpoint and persists the accumulator.
    /// We separate this from all other checkpoints as it is the only checkpoint
    /// that can be accumulated from two different callsites. As a result of this
    /// property, it is possible that it is called nonsequentially (i.e. there can
    /// be a gap between the checkpoint accumulated here and the next highest accumulated
    /// checkpoint). Therefore this function, unlike accumulate_checkpoint, may need to wait
    /// for others to finish being accumulated. This also allows accumulate_checkpoint to
    /// make guarantees on sequentiality, thereby avoiding disk writes or in-memory bookkeeping.
    pub async fn accumulate_final_checkpoint(
        &self,
        effects: Vec<TransactionEffects>,
        checkpoint_seq_num: CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Accumulator> {
        let _scope = monitored_scope("AccumulateFinalCheckpoint");
        debug!("Accumulating final checkpoint {}", checkpoint_seq_num);

        let last_accumulated = epoch_store.get_last_accumulated_checkpoint()?;
        let next_to_accumulate = last_accumulated.map(|last| last + 1).unwrap_or(0);

        // Check to see if we need to wait for other checkpoints to be
        // accumulated first in order to maintain sequentiality
        if next_to_accumulate < checkpoint_seq_num {
            debug!(
                "Awaiting accumulation of checkpoints in range {:?} to {:?} (inclusive) for epoch {} accumulation",
                next_to_accumulate,
                checkpoint_seq_num - 1,
                epoch_store.epoch(),
            );
            let remaining_checkpoints = (next_to_accumulate..=checkpoint_seq_num - 1).collect_vec();
            epoch_store
                .notify_read_checkpoint_state_digests(remaining_checkpoints)
                .await
                .expect("Failed to notify read checkpoint state digests");
        }

        self.accumulate_checkpoint_impl(effects, &checkpoint_seq_num, epoch_store)
    }

    fn accumulate_checkpoint_impl(
        &self,
        effects: Vec<TransactionEffects>,
        checkpoint_seq_num: &CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Accumulator> {
        // NB: it's important that we aquire this lock for the entire function call, as
        // two callers can race to accumulate the final checkpoint, and we may otherwise
        // re-accumulate the same checkpoint twice against the running_root_accumulator,
        // leading to a different root state hash. In the worst case, this could result
        // in a split brain fork.
        let mut running_root = self.running_root_accumulator.lock();
        if let Some(acc) = epoch_store.get_state_hash_for_checkpoint(checkpoint_seq_num)? {
            return Ok(acc);
        }

        let acc = self.accumulate_effects(effects, epoch_store.protocol_config());
        if let Some(running_root) = running_root.as_mut() {
            running_root.union(&acc);
        } else {
            *running_root = Some(acc.clone());
        }

        let mut batch = epoch_store.tables()?.state_hash_by_checkpoint.batch();
        batch.insert_batch(
            &epoch_store.tables()?.state_hash_by_checkpoint,
            std::iter::once((checkpoint_seq_num, running_root.clone().unwrap())),
        )?;
        batch.insert_batch(
            &epoch_store.tables()?.running_root_accumulator,
            std::iter::once(((), running_root.clone().unwrap())),
        )?;
        batch.write()?;
        debug!("Accumulated checkpoint {}", checkpoint_seq_num);

        epoch_store
            .checkpoint_state_notify_read
            .notify(checkpoint_seq_num, &acc);

        Ok(acc)
    }

    /// Accumulates given effects and returns the accumulator without side effects.
    pub fn accumulate_effects(
        &self,
        effects: Vec<TransactionEffects>,
        protocol_config: &ProtocolConfig,
    ) -> Accumulator {
        accumulate_effects(&*self.store, effects, protocol_config)
    }

    /// Must be called after all checkpoints have been accumulated for the epoch.
    /// Will force failure otherwise
    pub fn write_root_accumulator(
        &self,
        epoch_store: Arc<AuthorityPerEpochStore>,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
    ) -> SuiResult<Accumulator> {
        let _scope = monitored_scope("WriteRootAccumulator");
        // Important that we lock for the entire function as two callers
        // may race to read or write the root accumulator for the epoch.
        // We need to check for prior existence while we have the lock
        let mut running_root = self.running_root_accumulator.lock();
        let epoch = epoch_store.epoch();

        if let Some((_checkpoint, acc)) = self.store.get_root_state_accumulator_for_epoch(epoch)? {
            return Ok(acc);
        }
        let last_accumulated = epoch_store.get_last_accumulated_checkpoint()?;
        // If this fails then we have broken the invariant that checkpoints are
        // accumulated sequentially
        assert!(
            last_accumulated == Some(last_checkpoint_of_epoch),
            "Last accumulated checkpoint {} does not match last checkpoint of epoch {}",
            last_accumulated.unwrap_or(0),
            last_checkpoint_of_epoch,
        );

        let (_, (_, mut root_state_accumulator)) = self
            .store
            .get_root_state_accumulator_for_highest_epoch()?
            .unwrap_or((0, (0, Accumulator::default())));

        root_state_accumulator.union(&running_root.clone().unwrap_or_default());
        // Important! Do this to reset for the next epoch
        // TODO: instead we could make StateAccumulator a per-epoch component
        *running_root = None;

        debug!("Writing root accumulator for epoch {:?}", epoch);
        self.store.insert_state_accumulator_for_epoch(
            epoch,
            &last_checkpoint_of_epoch,
            &root_state_accumulator,
        )?;

        Ok(root_state_accumulator)
    }

    pub fn accumulate_cached_live_object_set_for_testing(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Accumulator {
        Self::accumulate_live_object_set_impl(
            self.store
                .iter_cached_live_object_set_for_testing(include_wrapped_tombstone),
        )
    }

    /// Returns the result of accumulating the live object set, without side effects
    pub fn accumulate_live_object_set(&self, include_wrapped_tombstone: bool) -> Accumulator {
        Self::accumulate_live_object_set_impl(
            self.store.iter_live_object_set(include_wrapped_tombstone),
        )
    }

    fn accumulate_live_object_set_impl(iter: impl Iterator<Item = LiveObject>) -> Accumulator {
        let mut acc = Accumulator::default();
        iter.for_each(|live_object| {
            Self::accumulate_live_object(&mut acc, &live_object);
        });
        acc
    }

    pub fn accumulate_live_object(acc: &mut Accumulator, live_object: &LiveObject) {
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

    pub fn digest_live_object_set(
        &self,
        include_wrapped_tombstone: bool,
    ) -> ECMHLiveObjectSetDigest {
        let acc = self.accumulate_live_object_set(include_wrapped_tombstone);
        acc.digest().into()
    }

    pub fn digest_epoch(
        &self,
        epoch_store: Arc<AuthorityPerEpochStore>,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
    ) -> SuiResult<ECMHLiveObjectSetDigest> {
        Ok(self
            .write_root_accumulator(epoch_store, last_checkpoint_of_epoch)?
            .digest()
            .into())
    }
}
