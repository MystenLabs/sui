// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use mysten_common::fatal;
use sui_types::accumulator_event::AccumulatorEvent;
use sui_types::effects::{
    AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
    TransactionEffects, TransactionEffectsAPI,
};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, CallArg, ObjectArg, TransactionKind};
use sui_types::{Identifier, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use tracing::debug;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::execution_cache::TransactionCacheRead;

/// Merged value is the value stored inside accumulator objects.
/// Each mergable Move type will map to a single variant as its representation.
///
/// For instance, Balance<T> stores a single u64 value, so it will map to SumU128.
/// A clawback Balance<T> will map to SumU128U128 since it also needs to represent
/// the amount of the balance that has been frozen.
enum MergedValue {
    SumU128(u128),
    SumU128U128(u128, u128),
}

impl MergedValue {
    fn add_move_call(
        self,
        _root: Argument,
        _address: &AccumulatorAddress,
        _builder: &mut ProgrammableTransactionBuilder,
    ) {
        match self {
            MergedValue::SumU128(_v) => todo!(),
            MergedValue::SumU128U128(_v1, _v2) => todo!(),
        }
    }
}

impl From<MergedValueIntermediate> for MergedValue {
    fn from(value: MergedValueIntermediate) -> Self {
        match value {
            MergedValueIntermediate::SumU128(v) => MergedValue::SumU128(v),
            MergedValueIntermediate::SumU128U128(v1, v2) => MergedValue::SumU128U128(v1, v2),
        }
    }
}

/// MergedValueIntermediate is an intermediate / in-memory representation of the for
/// accumulators. It is used to store the merged result of all accumulator writes in a single
/// checkpoint.
///
/// This pattern is not necessary for fully commutative operations, since those could use MergedValue directly.
///
/// However, this supports the commutative-merge + non-commutative-update pattern, which will be used by event
/// streams. In this pattern, everything within a checkpoint is merged commutativley, and then a single
/// non-commutative update is applied to the accumulator at the end of the checkpoint.
#[derive(Debug)]
enum MergedValueIntermediate {
    SumU128(u128),
    SumU128U128(u128, u128),
}

impl MergedValueIntermediate {
    fn init(value: AccumulatorValue) -> Self {
        match value {
            AccumulatorValue::Integer(v) => Self::SumU128(v as u128),
            AccumulatorValue::IntegerTuple(v1, v2) => Self::SumU128U128(v1 as u128, v2 as u128),
        }
    }

    fn accumulate_into(&mut self, value: AccumulatorValue) {
        match (self, value) {
            (Self::SumU128(v1), AccumulatorValue::Integer(v2)) => *v1 += v2 as u128,
            (Self::SumU128U128(v1, v2), AccumulatorValue::IntegerTuple(w1, w2)) => {
                *v1 += w1 as u128;
                *v2 += w2 as u128;
            }
            _ => {
                fatal!("invalid merge");
            }
        }
    }
}

// TODO(address-balances): This currently only creates a single accumulator update transaction.
// To support multiple accumulator update transactions, we need to:
// - have each transaction take the accumulator root as a "non-exclusive mutable" input
// - each transaction writes out a set of fields that are disjoint from the others.
// - a barrier transaction must be added to advance the version of the accumulator root object.
//   The barrier transaction doesn't do any field writes. This is necessary in order to provide
//   a consistent view of the system accumulator state. When the version of the accumulator
//   root object is advanced, we know that all accumulator state updates prior to that version
//   have been applied.
pub fn create_accumulator_update_transactions(
    epoch_store: &AuthorityPerEpochStore,
    checkpoint_height: u64,
    cache: Option<&dyn TransactionCacheRead>,
    ckpt_effects: &[TransactionEffects],
) -> Vec<TransactionKind> {
    let epoch = epoch_store.epoch();
    let Some(accumulator_root_obj_initial_shared_version) = epoch_store
        .epoch_start_config()
        .accumulator_root_obj_initial_shared_version()
    else {
        debug!("accumulator root object does not exist, skipping accumulator update");
        return vec![];
    };

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

    let root = builder
        .input(CallArg::Object(ObjectArg::SharedObject {
            id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            initial_shared_version: accumulator_root_obj_initial_shared_version,
            mutable: true,
        }))
        .unwrap();

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

    vec![TransactionKind::ProgrammableSystemTransaction(
        builder.finish(),
    )]
}
