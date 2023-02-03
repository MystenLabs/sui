// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

#[derive(Debug, Clone)]
pub struct State {
    pub effects: Vec<TransactionEffects>,
    pub checkpoint_seq_num: CheckpointSequenceNumber,
}

pub struct StateAccumulator {
    authority_store: Arc<AuthorityStore>,
}

impl StateAccumulator {
    pub fn new(authority_store: Arc<AuthorityStore>) -> Self {
        Self { authority_store }
    }

    pub fn accumulate_checkpoint(
        &self,
        state: State,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Accumulator> {
        if let Some(acc) = epoch_store.get_state_hash_for_checkpoint(&state.checkpoint_seq_num)? {
            return Ok(acc);
        }

        let mut acc = Accumulator::default();

        acc.insert_all(
            state
                .effects
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
            state
                .effects
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
            state
                .effects
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

        epoch_store.insert_state_hash_for_checkpoint(&state.checkpoint_seq_num, &acc)?;

        epoch_store
            .checkpoint_state_notify_read
            .notify(&state.checkpoint_seq_num, &acc);

        Ok(acc)
    }

    /// Unions all checkpoint accumulators at the end of the epoch to generate the
    /// root state hash and saves it. This function is guaranteed to be idempotent (despite the
    /// underlying data structure not being) as long as it is not called in a multi-threaded
    /// context. Can be called on non-consecutive epochs, e.g. to accumulate epoch 3 after
    /// having last accumulated epoch 1.
    pub fn accumulate_epoch(
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
        let (epoch, (next_to_accumulate, mut root_state_hash)) = self
            .authority_store
            .perpetual_tables
            .root_state_hash_by_epoch
            .iter()
            .skip_to_last()
            .next()
            .map(|(epoch, (highest, hash))| (epoch, (highest.saturating_add(1), hash)))
            .unwrap_or((0, (0, Accumulator::default())));

        for i in next_to_accumulate..=last_checkpoint_of_epoch {
            let acc = epoch_store
                .get_state_hash_for_checkpoint(&i)
                .unwrap()
                .unwrap_or_else(|| {
                    panic!("Accumulator for checkpoint sequence number {i:?} not present in store")
                });
            root_state_hash.union(&acc);
        }

        self.authority_store
            .perpetual_tables
            .root_state_hash_by_epoch
            .insert(&epoch, &(last_checkpoint_of_epoch, root_state_hash.clone()))?;

        Ok(root_state_hash)
    }

    pub fn digest_epoch(
        &self,
        epoch: &EpochId,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Result<Digest<32>, TypedStoreError> {
        Ok(self
            .accumulate_epoch(epoch, last_checkpoint_of_epoch, epoch_store)?
            .digest())
    }
}
