// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use consensus_config::Epoch;
use fastcrypto::hash::{Blake2b256, HashFunction};
use mysten_common::fatal;
use sui_types::accumulator_event::AccumulatorEvent;
use sui_types::committee::EpochId;
use sui_types::digests::Digest;
use sui_types::effects::{
    AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
    TransactionEffects, TransactionEffectsAPI,
};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, CallArg, TransactionKind};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID};

use crate::execution_cache::TransactionCacheRead;

enum MergedValue {
    Sum(u128),
    SumTuple(u128, u128),

    // TODO: This should be a merkle root instead of a linear hash
    EventDigest(Digest),
}

impl MergedValue {
    fn add_move_call(
        self,
        root: Argument,
        address: &AccumulatorAddress,
        builder: &mut ProgrammableTransactionBuilder,
    ) {
        match self {
            MergedValue::Sum(_v) => todo!(),
            MergedValue::SumTuple(_v1, _v2) => todo!(),
            MergedValue::EventDigest(digest) => {
                // Note: for event streams, the type of the accumulator is fixed
                // to be EventStreamHead.
                let args = vec![
                    root,
                    builder.pure(address.address).unwrap(),
                    builder.pure(digest).unwrap(),
                ];
                builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("event").unwrap(),
                    Identifier::new("update_head").unwrap(),
                    vec![],
                    args,
                );
            }
        }
    }
}

impl From<MergedValueIntermediate> for MergedValue {
    fn from(value: MergedValueIntermediate) -> Self {
        match value {
            MergedValueIntermediate::Sum(v) => MergedValue::Sum(v),
            MergedValueIntermediate::SumTuple(v1, v2) => MergedValue::SumTuple(v1, v2),
            MergedValueIntermediate::Events(events) => {
                // TODO(deepak): merkle tree?
                let mut h = Blake2b256::new();
                bcs::serialize_into(&mut h, &events).unwrap();
                MergedValue::EventDigest(Digest::new(h.finalize().digest))
            }
        }
    }
}

#[derive(Debug)]
enum MergedValueIntermediate {
    Sum(u128),
    SumTuple(u128, u128),
    Events(Vec<Digest>),
}

impl MergedValueIntermediate {
    fn assert_bounds(value: &AccumulatorValue) {
        match &value {
            AccumulatorValue::Integer(v) => assert!(*v <= u64::MAX as u128, "value out of bounds"),
            AccumulatorValue::IntegerTuple(v1, v2) => {
                assert!(
                    *v1 <= u64::MAX as u128 && *v2 <= u64::MAX as u128,
                    "value out of bounds"
                );
            }
            AccumulatorValue::EventDigest(_v) => (),
        }
    }

    fn init(value: AccumulatorValue) -> Self {
        Self::assert_bounds(&value);
        match value {
            AccumulatorValue::Integer(v) => Self::Sum(v),
            AccumulatorValue::IntegerTuple(v1, v2) => Self::SumTuple(v1, v2),
            AccumulatorValue::EventDigest(v) => Self::Events(vec![v]),
        }
    }

    fn accumulate_into(&mut self, value: AccumulatorValue) {
        Self::assert_bounds(&value);

        match (self, value) {
            (Self::Sum(v1), AccumulatorValue::Integer(v2)) => *v1 += v2,
            (Self::SumTuple(v1, v2), AccumulatorValue::IntegerTuple(w1, w2)) => {
                *v1 += w1;
                *v2 += w2;
            }
            (Self::Events(digests), AccumulatorValue::EventDigest(digest)) => {
                digests.push(digest);
            }
            _ => {
                fatal!("invalid merge");
            }
        }
    }
}

pub fn create_accumulator_update_transactions(
    epoch: EpochId,
    checkpoint_height: u64,
    cache: Option<&dyn TransactionCacheRead>,
    ckpt_effects: &[TransactionEffects],
) -> Vec<TransactionKind> {
    let mut merges = HashMap::<_, MergedValueIntermediate>::new();
    let mut splits = HashMap::<_, MergedValueIntermediate>::new();
    let mut addresses = HashMap::<_, AccumulatorAddress>::new();

    for effect in ckpt_effects {
        let tx = effect.transaction_digest();
        // TransactionEffectsAPI::accumulator_events() uses a linear scan of all
        // object changes and allocates a new vector. In the common case (on validators),
        // we still have still have the original vector in the writeback cache, so
        // we can avoid the unnecessary work by just taking it from the cache.
        let events = match cache.and_then(|c| c.take_accumulator_events(tx)) {
            Some(events) => events,
            None => effect.accumulator_events(),
        };

        for AccumulatorEvent {
            accumulator_obj,
            write:
                AccumulatorWriteV1 {
                    operation,
                    value,
                    address,
                },
        } in events
        {
            if let Some(prev) = addresses.insert(accumulator_obj, address.clone()) {
                debug_assert_eq!(prev, address);
            }

            let entry = match operation {
                AccumulatorOperation::Merge => merges.entry(accumulator_obj),
                AccumulatorOperation::Split => splits.entry(accumulator_obj),
            };

            match entry {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().accumulate_into(value);
                }
                Entry::Vacant(entry) => {
                    entry.insert(MergedValueIntermediate::init(value));
                }
            }
        }
    }

    if merges.is_empty() && splits.is_empty() {
        return vec![];
    }

    let mut builder = ProgrammableTransactionBuilder::new();

    let root = builder.input(CallArg::ACCUMULATOR_ROOT_MUT).unwrap();

    for (accumulator_obj, merged_value) in merges {
        let address = addresses.get(&accumulator_obj).unwrap();
        let merged_value = MergedValue::from(merged_value);
        merged_value.add_move_call(root, address, &mut builder);
    }

    for (_accumulator_obj, _merged_value) in splits {
        todo!();
    }

    let epoch_arg = builder.pure(epoch).unwrap();
    let checkpoint_height_arg = builder.pure(checkpoint_height).unwrap();
    let idx_arg = builder.pure(0).unwrap();

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("accumulator").unwrap(),
        Identifier::new("commit_to_checkpoint").unwrap(),
        vec![],
        vec![epoch_arg, checkpoint_height_arg, idx_arg],
    );

    vec![TransactionKind::ProgrammableTransaction(builder.finish())]
}
