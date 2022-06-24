// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::{base_types::ExecutionDigests, error::SuiResult};
use typed_store::Map;

use crate::authority::AuthorityStore;

use super::CheckpointStore;

/// The interfaces here allow a separation between checkpoint store, that knows about digests largely,
/// and other parts of the system that know about the transaction semantics, hold and can interpret the
/// transaction effects.
///
/// A point of interaction between these two worlds is necessary when we need to order the execution digests
/// within a checkpoint, as well as detect digest already in checkpoints of missing to have a full causal history.
/// The interface here allows these computations to be implemented without passing in a full authority / authority store
/// for the sake of keeping components separate enough to be tested without one another.

pub trait CausalOrder {
    fn get_complete_causal_order(
        self: Self,
        transactions: &[ExecutionDigests],
        ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>>;
}

/// An identity causal order that returns just the same order. For testing.
pub struct TestCausalOrderNoop;

impl CausalOrder for TestCausalOrderNoop {
    fn get_complete_causal_order(
        self: Self,
        transactions: &[ExecutionDigests],
        _ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        Ok(transactions.iter().cloned().collect())
    }
}

/// Now this is a real causal orderer based on having an Arc<AuthorityStore> handy.
impl CausalOrder for Arc<AuthorityStore> {
    fn get_complete_causal_order(
        self: Self,
        transactions: &[ExecutionDigests],
        _ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        let effetcs = self
            .effects
            .multi_get(transactions.iter().map(|d| d.transaction))?;

        // Ensure all transactions included are executed (static property). This should be true since we should not
        // be signing a checkpoint unless we have processed all transactions within it.
        debug_assert!(effetcs.iter().all(|e| e.is_some()));

        // Include in the checkpoint the computed effects,  rather than the effects provided.
        // (Which could have been provided by < f+1 byz validators and be incorrect).
        let digests = effetcs
            .into_iter()
            .map(|e| {
                let e = e.unwrap().effects;
                ExecutionDigests::new(e.transaction_digest, e.digest())
            })
            .collect::<Vec<ExecutionDigests>>();

        // Only include in the checkpoint transactions that have not been checkpointed before.
        // Due to the construction of the checkpoint table `extra_transactions` and given that
        // we must have processed all transactions in the proposed checkpoint, this check is
        // reduced to ensuring that the transactions are in the table `extra_transactions`
        // (that lists transactions executed but not yet checkpointed).
        let in_store =_ckpt_store.extra_transactions.multi_get(&digests)?;
        let digests : Vec<_> = digests.iter().zip(in_store).filter_map(|(d, instore)| {
            if instore.is_some() {
                Some(d)
            }
            else {
                None
            }
        }).collect();

        Ok(transactions.iter().cloned().collect())
    }
}
