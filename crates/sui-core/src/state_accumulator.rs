// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::monitored_scope;
use serde::Serialize;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::committee::EpochId;
use sui_types::digests::ObjectDigest;
use sui_types::storage::ObjectKey;
use tracing::debug;
use typed_store::Map;

use std::sync::Arc;

use fastcrypto::hash::MultisetHash;
use sui_types::accumulator::Accumulator;
use sui_types::error::SuiResult;
use sui_types::messages::{TransactionEffects, TransactionEffectsAPI};
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, ECMHLiveObjectSetDigest};
use typed_store::rocks::TypedStoreError;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::AuthorityStore;

pub struct StateAccumulator {
    authority_store: Arc<AuthorityStore>,
}

/// Serializable representation of the ObjectRef of an
/// object that has been wrapped
#[derive(Serialize)]
struct WrappedObject {
    id: ObjectID,
    wrapped_at: SequenceNumber,
    digest: ObjectDigest,
}

impl WrappedObject {
    fn new(id: ObjectID, wrapped_at: SequenceNumber) -> Self {
        Self {
            id,
            wrapped_at,
            digest: ObjectDigest::OBJECT_DIGEST_WRAPPED,
        }
    }
}

impl StateAccumulator {
    pub fn new(authority_store: Arc<AuthorityStore>) -> Self {
        Self { authority_store }
    }

    /// Accumulates the effects of a single checkpoint.
    /// This function is idempotent.
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
                        .collect::<Vec<ObjectDigest>>()
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

        // get all modified_at_versions for the fx
        let modified_at_version_keys: Vec<_> = effects
            .iter()
            .flat_map(|fx| {
                fx.modified_at_versions()
                    .iter()
                    .map(|(id, seq_num)| ObjectKey(*id, *seq_num))
                    .collect::<Vec<ObjectKey>>()
            })
            .collect();

        let modified_at_digests: Vec<_> = self
            .authority_store
            .multi_get_object_by_key(&modified_at_version_keys)
            .expect("Failed to get modified_at_versions object from object table")
            .into_iter()
            .map(|obj| {
                obj.expect(
                    "Object from modified_at_versions effects does not exist in objects table",
                )
                .compute_object_reference()
                .2
            })
            .collect();

        acc.remove_all(modified_at_digests);

        epoch_store.insert_state_hash_for_checkpoint(&checkpoint_seq_num, &acc)?;
        debug!("Accumulated checkpoint {}", checkpoint_seq_num);

        epoch_store
            .checkpoint_state_notify_read
            .notify(&checkpoint_seq_num, &acc);

        Ok(acc)
    }

    /// Unions all checkpoint accumulators at the end of the epoch to generate the
    /// root state hash and saves it. This function is idempotent. Can be called on
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

    pub fn accumulate_live_object_set(&self) -> Accumulator {
        let mut acc = Accumulator::default();
        for oref in self.authority_store.iter_live_object_set() {
            if oref.2 == ObjectDigest::OBJECT_DIGEST_WRAPPED {
                acc.insert(
                    bcs::to_bytes(&WrappedObject::new(oref.0, oref.1))
                        .expect("Failed to serialize WrappedObject"),
                );
            } else {
                acc.insert(oref.2);
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
