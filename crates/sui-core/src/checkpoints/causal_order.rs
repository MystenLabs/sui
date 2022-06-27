// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::Arc,
};

use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest},
    error::SuiResult,
    messages::TransactionEffects,
};
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
        &self,
        transactions: &[ExecutionDigests],
        ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>>;
}

pub trait EffectsStore {
    fn get_effects(
        &self,
        transactions: &[ExecutionDigests],
    ) -> SuiResult<Vec<Option<TransactionEffects>>>;

    fn causal_order_from_effects(
        &self,
        transactions: &[ExecutionDigests],
        ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        let effetcs = self.get_effects(transactions)?;

        // Ensure all transactions included are executed (static property). This should be true since we should not
        // be signing a checkpoint unless we have processed all transactions within it.
        debug_assert!(effetcs.iter().all(|e| e.is_some()));

        // Include in the checkpoint the computed effects,  rather than the effects provided.
        // (Which could have been provided by < f+1 byz validators and be incorrect).
        let digests = effetcs
            .iter()
            .map(|e| {
                let e = &e.as_ref().unwrap();
                ExecutionDigests::new(e.transaction_digest, e.digest())
            })
            .collect::<Vec<ExecutionDigests>>();

        // Load the extra transactions in memory, we will use them quite a bit.
        // TODO: monitor memory use here.
        let tx_not_in_checkpoint: HashSet<_> = ckpt_store
            .extra_transactions
            .keys()
            .map(|e| e.transaction)
            .collect();

        // Index the effects by transaction digest, as we will need to look them up.
        let mut effect_map: BTreeMap<TransactionDigest, &TransactionEffects> = effetcs
            .iter()
            .map(|e| {
                let e = e.as_ref().unwrap();
                (e.transaction_digest, e)
            })
            .collect();

        // Only include in the checkpoint transactions that have not been checkpointed before.
        // Due to the construction of the checkpoint table `extra_transactions` and given that
        // we must have processed all transactions in the proposed checkpoint, this check is
        // reduced to ensuring that the transactions are in the table `extra_transactions`
        // (that lists transactions executed but not yet checkpointed).
        let digest_map: BTreeMap<TransactionDigest, &ExecutionDigests> = digests
            .iter()
            .filter_map(|d| {
                if tx_not_in_checkpoint.contains(&d.transaction) {
                    Some((d.transaction, d))
                } else {
                    // We remove the effects map entries for transactions
                    // that are already checkpointed.
                    effect_map.remove(&d.transaction);
                    None
                }
            })
            .collect();

        // Set of starting transactions that depend only on previously
        // checkpointed objects.
        let initial_transactions: BTreeSet<_> = effect_map
            .iter()
            .filter_map(|(d, e)| {
                // All dependencies must be in checkpoint.
                if e.dependencies
                    .iter()
                    .all(|d| !tx_not_in_checkpoint.contains(d))
                {
                    Some(d)
                } else {
                    None
                }
            })
            .collect();

        // Build a forward index of transactions. This will allow us to start with the initial
        // and then sequenced trasnactions and efficiently determine which other transactions
        // become candidates for sequencing.
        let mut forward_index: BTreeMap<&TransactionDigest, Vec<&TransactionDigest>> =
            BTreeMap::new();
        for (d, effect) in &effect_map {
            for dep in &effect.dependencies {
                // We only record the dependencies not in a checkpoint, as the ones
                // in a checkpoint are already satisfied presumably.
                if tx_not_in_checkpoint.contains(dep) {
                    forward_index.entry(dep).or_default().push(d);
                }
            }
        }

        // Define the master sequence, to contain the initial transactions
        // by transaction digest order.
        let mut master_sequence: Vec<&TransactionDigest> =
            initial_transactions.iter().cloned().collect();
        // A set mirroring the contents of the mater sequence for quick lookup
        let mut master_set = initial_transactions.clone();
        // The transactions that just became executed.
        let mut candidates = initial_transactions;

        // Trace forward the executed transactions, starting from the initial set, and adding more
        // trasnactions as all their dependencies become executed. The candidates represent executed
        // transactions that need to have subsequent transactions depending on them examined to
        // determine if all their dependencies are executed. If so they are sequenced, and also added
        // to the candidate set to be examiner once.
        while !candidates.is_empty() {
            // we continue while we can make progress

            // Take a transaction
            let next_transaction = *candidates.iter().next().unwrap();
            candidates.remove(next_transaction);

            // Check all transactions that depend on it, to see if all
            // dependencies are  satisfied.
            for dep in forward_index.get(next_transaction).unwrap() {
                // The candidate is its parent. If it is the last parent the above will be true
                // but only once, so we should not already have sequenced it.
                debug_assert!(!master_set.contains(dep));

                if effect_map
                    .get(*dep)
                    .unwrap()
                    .dependencies
                    .iter()
                    .all(|item| master_set.contains(item))
                {
                    // It seems like all dependencies are satisfied for dep, so sequence it.
                    master_sequence.push(dep);
                    master_set.insert(dep);
                    candidates.insert(dep);
                }
            }
        }

        // NOTE: not all transactions have to be seqeunced into the checkpoint. In particular if a
        // byzantine node includes some transaction into their proposal but not its previous dependencies
        // they may not be checkpointed. That is ok, since we promise finality only if >2/3 honest
        // eventually include in proposal, which means that at leats 1 honest will include in a checkpoint
        // and honest nodes include full causal sequences in proposals.

        // Map transaction digest back to correct execution digest.
        Ok(master_sequence
            .iter()
            .map(|d| **digest_map.get(*d).unwrap())
            .collect())
    }
}

impl EffectsStore for Arc<AuthorityStore> {
    fn get_effects(
        &self,
        transactions: &[ExecutionDigests],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(self
            .effects
            .multi_get(transactions.iter().map(|d| d.transaction))?
            .into_iter()
            .map(|item| item.map(|x| x.effects))
            .collect())
    }
}

/// An identity causal order that returns just the same order. For testing.
pub struct TestCausalOrderNoop;

impl CausalOrder for TestCausalOrderNoop {
    fn get_complete_causal_order(
        &self,
        transactions: &[ExecutionDigests],
        _ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        Ok(transactions.to_vec())
    }
}

/// Now this is a real causal orderer based on having an Arc<AuthorityStore> handy.
impl CausalOrder for Arc<AuthorityStore> {
    fn get_complete_causal_order(
        &self,
        transactions: &[ExecutionDigests],
        _ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        self.causal_order_from_effects(transactions, _ckpt_store)
    }
}
