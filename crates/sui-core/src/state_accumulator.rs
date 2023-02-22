// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::monitored_scope;
use sui_types::committee::EpochId;
use typed_store::Map;

use std::sync::Arc;
use sui_types::base_types::ObjectDigest;

use fastcrypto::hash::{Digest, MultisetHash};
use sui_types::accumulator::Accumulator;
use sui_types::error::SuiResult;
use sui_types::messages::TransactionEffects;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use typed_store::rocks::TypedStoreError;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::AuthorityStore;

pub struct StateAccumulator {
    authority_store: Arc<AuthorityStore>,
}

impl StateAccumulator {
    pub fn new(authority_store: Arc<AuthorityStore>) -> Self {
        Self { authority_store }
    }

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

        acc.insert_all(
            effects
                .iter()
                .flat_map(|fx| {
                    fx.created
                        .clone()
                        .into_iter()
                        .map(|(obj_ref, _)| obj_ref.2)
                        .collect::<Vec<ObjectDigest>>()
                })
                .collect::<Vec<ObjectDigest>>(),
        );
        acc.remove_all(
            effects
                .iter()
                .flat_map(|fx| {
                    fx.deleted
                        .clone()
                        .into_iter()
                        .map(|obj_ref| obj_ref.2)
                        .collect::<Vec<ObjectDigest>>()
                })
                .collect::<Vec<ObjectDigest>>(),
        );

        // TODO almost certainly not currectly handling "mutated" effects.
        acc.insert_all(
            effects
                .iter()
                .flat_map(|fx| {
                    fx.mutated
                        .clone()
                        .into_iter()
                        .map(|(obj_ref, _)| obj_ref.2)
                        .collect::<Vec<ObjectDigest>>()
                })
                .collect::<Vec<ObjectDigest>>(),
        );

        epoch_store.insert_state_hash_for_checkpoint(&checkpoint_seq_num, &acc)?;

        epoch_store
            .checkpoint_state_notify_read
            .notify(&checkpoint_seq_num, &acc);

        Ok(acc)
    }

    /// Unions all checkpoint accumulators at the end of the epoch to generate the
    /// root state hash and saves it. This function is guaranteed to be idempotent (despite the
    /// underlying data structure not being) as long as it is not called in a multi-threaded
    /// context. Can be called on non-consecutive epochs, e.g. to accumulate epoch 3 after
    /// having last accumulated epoch 1.
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

        let (checkpoints, mut accumulators) = epoch_store
            .get_accumulators_in_checkpoint_range(next_to_accumulate, last_checkpoint_of_epoch)?
            .into_iter()
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let remaining_checkpoints: Vec<_> = (next_to_accumulate..=last_checkpoint_of_epoch)
            .filter(|seq_num| !checkpoints.contains(seq_num))
            .collect();

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

    pub async fn digest_epoch(
        &self,
        epoch: &EpochId,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Result<Digest<32>, TypedStoreError> {
        Ok(self
            .accumulate_epoch(epoch, last_checkpoint_of_epoch, epoch_store)
            .await?
            .digest())
    }
}
