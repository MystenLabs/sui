// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::accumulator_event::AccumulatorEvent;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::transaction::Transaction;

use crate::execution_cache::TransactionCacheRead;

pub fn create_accumulator_update_transactions(
    cache: impl TransactionCacheRead,
    ckpt_effects: &[TransactionEffects],
) -> Vec<Transaction> {
    let mut txs = Vec::new();
    for effect in ckpt_effects {
        let tx = effect.transaction_digest();

        txs.push(tx);
    }
    txs
}
