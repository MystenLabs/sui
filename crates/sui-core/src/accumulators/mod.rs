// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;

use sui_types::accumulator_event::AccumulatorEvent;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::transaction::Transaction;

use crate::execution_cache::TransactionCacheRead;

enum MergedValue {
    Sum(u128),
    SumTuple(u128, u128),
    Events(Vec<AccumulatorEvent>),
}

impl MergedValue {

    fn assert_bounds(value: &AccumulatorValue) {
        match &value {
            AccumulatorValue::Integer(v) => assert!(v <= u64::MAX as u128, "value out of bounds");
            AccumulatorValue::IntegerTuple(v1, v2) => {
                assert!(v1 <= u64::MAX as u128 && v2 <= u64::MAX as u128, "value out of bounds");
            }
        }
    }

    fn init(value: AccumulatorValue) -> Self {
        assert_bounds(&value);
        match value {
            AccumulatorValue::Integer(v) => Self::Sum(v),
            AccumulatorValue::IntegerTuple(v1, v2) => Self::SumTuple(v1, v2),
            AccumulatorValue::EventDigest(v) => Self::Events(vec![AccumulatorEvent::new(v)]),
        }
    }

    fn accumulate_into(&mut self, value: AccumulatorValue) {
        assert_bounds(&value);

        match (self, value) {
            (Self::Sum(v1), Self::Sum(v2)) => *v1 += v2,
            (Self::SumTuple(v1, v2), Self::SumTuple(w1, w2)) => {
                *v1 += w1;
                *v2 += w2;
            }
            (Self::Events(digests), Self::EventDigest(digest)) => {
                digests.push(digest);
            }
            _ => {
                fatal!("invalid merge {:?} and {:?}", self, value);
            }
        }
    }

}

pub fn create_accumulator_update_transactions(
    cache: impl TransactionCacheRead,
    ckpt_effects: &[TransactionEffects],
) -> Vec<Transaction> {

    for effect in ckpt_effects {
        let tx = effect.transaction_digest();
        // TransactionEffectsAPI::accumulator_events() uses a linear scan of all
        // object changes and allocates a new vector. In the common case (on validators),
        // we still have still have the original vector in the writeback cache, so
        // we can avoid the unnecessary work by just taking it from the cache.
        let events = match cache.take_accumulator_events(&tx) {
            Some(events) => events,
            None => effect.accumulator_events(),
        };

    let mut merged_accumulator_writes = HashMap::new();
    for AccumulatorEvent {
        accumulator_obj,
        write,
        } in events {

        match merged_accumulator_writes
            .entry(accumulator_obj) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().accumulate_into(write);
                }
                Entry::Vacant(entry) => {
                    entry.insert(MergedValue::init(write));
                }
            }
        }
    }
}