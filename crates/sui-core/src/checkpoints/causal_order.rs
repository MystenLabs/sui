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
        let effects = self.get_effects(transactions)?;

        // Ensure all transactions included are executed (static property). This should be true since we should not
        // be signing a checkpoint unless we have processed all transactions within it.
        debug_assert!(effects.iter().all(|e| e.is_some()));

        // Include in the checkpoint the computed effects,  rather than the effects provided.
        // (Which could have been provided by < f+1 byz validators and be incorrect).
        let digests = effects
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
        let mut effect_map: BTreeMap<TransactionDigest, &TransactionEffects> = effects
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

            // If nothing depends on this tx move on to the next
            let forward_deps = forward_index.get(next_transaction);
            if forward_deps.is_none() {
                continue;
            }

            // Check all transactions that depend on it, to see if all
            // dependencies are  satisfied.
            for dep in forward_deps.unwrap() {
                // If the forward dependency is not included in the current checkpoint, ignore.
                if !effect_map.contains_key(*dep) {
                    continue;
                }

                // We have already included this into the candidate set once.
                if master_set.contains(dep) {
                    continue;
                }

                if effect_map
                    .get(*dep)
                    .unwrap()
                    .dependencies
                    .iter()
                    // All dependencies sequenced in the master seq or in previous checkpoint
                    .filter(|item| tx_not_in_checkpoint.contains(*item))
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

impl EffectsStore for BTreeMap<TransactionDigest, TransactionEffects> {
    fn get_effects(
        &self,
        transactions: &[ExecutionDigests],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(transactions
            .iter()
            .map(|item| self.get(&item.transaction).cloned())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, env, fs, sync::Arc};

    use crate::checkpoints::causal_order::EffectsStore;
    use crate::checkpoints::CheckpointStore;
    use rand::{prelude::StdRng, SeedableRng};
    use sui_types::{
        base_types::{ExecutionDigests, ObjectDigest, ObjectID, SequenceNumber, TransactionDigest},
        gas::GasCostSummary,
        messages::{ExecutionStatus, TransactionEffects},
        object::Owner,
        utils::make_committee_key,
    };
    use typed_store::Map;

    fn effects_from(
        transaction_digest: TransactionDigest,
        dependencies: Vec<TransactionDigest>,
    ) -> TransactionEffects {
        TransactionEffects {
            // The only fields that matter
            transaction_digest,
            dependencies,

            // Other fields do not really matter here
            status: ExecutionStatus::Success,
            gas_used: GasCostSummary {
                computation_cost: 0,
                storage_cost: 0,
                storage_rebate: 0,
            },
            shared_objects: vec![],
            created: vec![],
            mutated: vec![],
            unwrapped: vec![],
            deleted: vec![],
            wrapped: vec![],
            // Nonsense is ok for the purposes of these tests
            gas_object: (
                (
                    ObjectID::random(),
                    SequenceNumber::from(0),
                    ObjectDigest::random(),
                ),
                Owner::Immutable,
            ),
            events: vec![],
        }
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn causal_just_reorder() {
        let mut rng = StdRng::from_seed([1; 32]);
        let (keys, committee) = make_committee_key(&mut rng);
        let k = keys[0].copy();

        // Setup

        let dir = env::temp_dir();
        let path = dir.join(format!("SC_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        // Create an authority
        // Open store first time

        let mut cps = CheckpointStore::open(
            path,
            None,
            committee.epoch,
            *k.public_key_bytes(),
            Arc::pin(k.copy()),
        )
        .unwrap();

        let result = 2 + 2;
        assert_eq!(result, 4);

        // Make some transactions
        let t0 = TransactionDigest::random();
        let t1 = TransactionDigest::random();
        let t2 = TransactionDigest::random();
        let t3 = TransactionDigest::random();

        let e0 = effects_from(t0, vec![]);
        let e1 = effects_from(t1, vec![t0]);
        let e2 = effects_from(t2, vec![t1]);
        let e3 = effects_from(t3, vec![t2]);

        let mut effect_map = BTreeMap::new();
        effect_map.extend([
            (t0, e0),
            (t1, e1.clone()),
            (t2, e2.clone()),
            (t3, e3.clone()),
        ]);

        let input: Vec<_> = vec![e2.clone(), e1.clone(), e3.clone()]
            .iter()
            .map(|item| ExecutionDigests::new(item.transaction_digest, item.digest()))
            .collect();

        // TEST 1
        // None are recorded as new transactions in the checkpoint DB so the end sequence is empty
        let x = effect_map.causal_order_from_effects(&input, &mut cps);
        assert_eq!(x.unwrap().len(), 0);

        cps.extra_transactions.insert(&input[0], &0).unwrap();
        cps.extra_transactions.insert(&input[1], &1).unwrap();
        cps.extra_transactions.insert(&input[2], &2).unwrap();

        // TEST 2
        // The two transactions are recorded as new so they are re-ordered and sequenced
        let x = effect_map.causal_order_from_effects(&input[..2], &mut cps);
        assert!(x.clone().unwrap().len() == 2);
        // Its in the correct order
        assert!(x.unwrap() == vec![input[1], input[0]]);

        // TEST3
        // Skip t2. and order [t3, t1]
        let input: Vec<_> = vec![e3, e1.clone()]
            .iter()
            .map(|item| ExecutionDigests::new(item.transaction_digest, item.digest()))
            .collect();

        let x = effect_map.causal_order_from_effects(&input[..2], &mut cps);

        assert!(x.clone().unwrap().len() == 1);
        // Its in the correct order
        assert!(x.unwrap() == vec![input[1]]);

        // Test4
        // Many dependencies
        println!("Test 4");

        // Make some transactions
        let tx = TransactionDigest::random();
        let ty = TransactionDigest::random();

        let ex = effects_from(tx, vec![t0, t1]);
        let ey = effects_from(ty, vec![tx, t2]);

        effect_map.extend([(tx, ex.clone()), (ty, ey.clone())]);

        let input: Vec<_> = vec![e2.clone(), ex.clone(), ey.clone(), e1.clone()]
            .iter()
            .map(|item| ExecutionDigests::new(item.transaction_digest, item.digest()))
            .collect();

        cps.extra_transactions.insert(&input[1], &3).unwrap();
        cps.extra_transactions.insert(&input[2], &4).unwrap();

        assert!(input[1..].len() == 3);
        let x = effect_map.causal_order_from_effects(&input[1..], &mut cps);

        println!("result: {:?}", x);
        assert_eq!(x.clone().unwrap().len(), 2);
        // Its in the correct order
        assert_eq!(x.unwrap(), vec![input[3], input[1]]);

        // TESt 5 all

        let x = effect_map.causal_order_from_effects(&input, &mut cps);

        println!("result: {:?}", x);
        assert_eq!(x.clone().unwrap().len(), 4);
    }
}
