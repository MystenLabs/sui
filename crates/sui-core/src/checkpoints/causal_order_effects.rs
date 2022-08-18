// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{hash_map::Entry, BTreeMap, BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest},
    error::{SuiError, SuiResult},
    message_envelope::Message,
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
    fn get_complete_causal_order<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
        ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>>;
}

pub trait EffectsStore {
    fn get_effects<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
    ) -> SuiResult<Vec<Option<TransactionEffects>>>;

    fn causal_order_from_effects<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
        ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        let effects = self.get_effects(transactions)?;

        // Ensure all transactions included are executed (static property). This should be true since we should not
        // be signing a checkpoint unless we have processed all transactions within it.
        if effects.iter().any(|e| e.is_none()) {
            return Err(SuiError::from(
                "Cannot causally order checkpoint with unexecuted transactions.",
            ));
        }

        // Include in the checkpoint the computed effects,  rather than the effects provided.
        // (Which could have been provided by < f+1 byz validators and be incorrect).
        let digests = effects
            .iter()
            .map(|e| {
                // We have checked above all transactions have effects so unwrap is ok.
                let e = &e.as_ref().unwrap();
                ExecutionDigests::new(e.transaction_digest, e.digest())
            })
            .collect::<Vec<ExecutionDigests>>();

        // Load the extra transactions in memory, we will use them quite a bit.
        // TODO: monitor memory use here.
        let tx_not_in_checkpoint: HashSet<_> = ckpt_store
            .tables
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

        // Build a forward index of transactions. This will allow us to start with the initial
        // and then sequenced trasnactions and efficiently determine which other transactions
        // become candidates for sequencing.
        let mut forward_index: BTreeMap<&TransactionDigest, Vec<&TransactionDigest>> =
            BTreeMap::new();

        // Keep track of the in-degree to facilitate topological sort.
        let mut in_degree: HashMap<&TransactionDigest, usize> = HashMap::new();
        let mut to_visit: BTreeSet<_> = effect_map.keys().collect();

        for (d, effect) in &effect_map {
            let entry = in_degree.entry(d).or_default();

            for dep in &effect.dependencies {
                // We only record the dependencies not in a checkpoint, as the ones
                // in a checkpoint are already satisfied presumably.
                if tx_not_in_checkpoint.contains(dep) {
                    forward_index.entry(dep).or_default().push(d);

                    // We record a dependency from within the tx not in a checkpoint
                    *entry += 1;
                }
            }

            // If it has a dependency it cannot be a starting item for the topological
            // sort.
            if *entry > 0 {
                to_visit.remove(d);
            }
        }

        // This implements the topological sort
        // TODO: implement an order that allows for parallel execution,
        //       ie orders first items that are independent.

        let mut final_sequence = Vec::new();
        while let Some(&item) = to_visit.iter().next() {
            // simulate pop_first
            to_visit.remove(item);
            final_sequence.push(item);
            forward_index
                .entry(item)
                .or_default()
                .iter()
                .for_each(|&child| {
                    if !effect_map.contains_key(child) {
                        // The child is in the extra executed tx but not in the checkpoint.
                        // so we skip it, as it must not be included in the sequennce.
                        return;
                    }

                    if let Entry::Occupied(mut entry) = in_degree.entry(child) {
                        *entry.get_mut() -= 1;
                        if *entry.get() == 0 {
                            to_visit.insert(child);
                        }
                    }
                });
        }

        // NOTE: not all transactions have to be sequenced into the checkpoint. In particular if a
        // byzantine node includes some transaction into their proposal but not its previous dependencies
        // they may not be checkpointed. That is ok, since we promise finality only if >2/3 honest
        // eventually include a transactions in a proposal, which means that at least 1 honest will
        // include it in a proposal and honest nodes include full causal sequences in proposals.

        // Map transaction digest back to correct execution digest.
        Ok(final_sequence
            .iter()
            .map(|d| **digest_map.get(*d).unwrap())
            .collect())
    }
}

impl EffectsStore for Arc<AuthorityStore> {
    fn get_effects<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(self
            .tables
            .effects
            .multi_get(transactions.map(|d| d.transaction))?
            .into_iter()
            .map(|item| item.map(|x| x.effects().clone()))
            .collect())
    }
}

/// An identity causal order that returns just the same order. For testing.
pub struct TestCausalOrderNoop;

impl CausalOrder for TestCausalOrderNoop {
    fn get_complete_causal_order<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
        _ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        Ok(transactions.cloned().collect())
    }
}

/// Now this is a real causal orderer based on having an Arc<AuthorityStore> handy.
impl CausalOrder for Arc<AuthorityStore> {
    fn get_complete_causal_order<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
        _ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        self.causal_order_from_effects(transactions, _ckpt_store)
    }
}

impl EffectsStore for BTreeMap<TransactionDigest, TransactionEffects> {
    fn get_effects<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(transactions
            .map(|item| self.get(&item.transaction).cloned())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, env, fs, sync::Arc};

    use crate::checkpoints::causal_order_effects::EffectsStore;
    use crate::checkpoints::CheckpointStore;
    use narwhal_crypto::traits::KeyPair;
    use rand::{prelude::StdRng, SeedableRng};
    use sui_types::{
        base_types::{ExecutionDigests, ObjectDigest, ObjectID, SequenceNumber, TransactionDigest},
        gas::GasCostSummary,
        message_envelope::Message,
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
            &path,
            None,
            committee.epoch,
            k.public().into(),
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
        let x = effect_map.causal_order_from_effects(input.iter(), &mut cps);
        assert_eq!(x.unwrap().len(), 0);

        cps.tables.extra_transactions.insert(&input[0], &0).unwrap();
        cps.tables.extra_transactions.insert(&input[1], &1).unwrap();
        cps.tables.extra_transactions.insert(&input[2], &2).unwrap();

        // TEST 2
        // The two transactions are recorded as new so they are re-ordered and sequenced
        let x = effect_map.causal_order_from_effects(input[..2].iter(), &mut cps);
        assert_eq!(x.clone().unwrap().len(), 2);
        // Its in the correct order
        assert_eq!(x.unwrap(), vec![input[1], input[0]]);

        // TEST3
        // Skip t2. and order [t3, t1]
        let input: Vec<_> = vec![e3, e1.clone()]
            .iter()
            .map(|item| ExecutionDigests::new(item.transaction_digest, item.digest()))
            .collect();

        let x = effect_map.causal_order_from_effects(input[..2].iter(), &mut cps);

        assert!(x.clone().unwrap().len() == 1);
        // Its in the correct order
        assert!(x.unwrap() == vec![input[1]]);

        // Test4
        // Many dependencies

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

        cps.tables.extra_transactions.insert(&input[1], &3).unwrap();
        cps.tables.extra_transactions.insert(&input[2], &4).unwrap();

        assert!(input[1..].len() == 3);
        let x = effect_map.causal_order_from_effects(input[1..].iter(), &mut cps);

        println!("result: {:?}", x);
        assert_eq!(x.clone().unwrap().len(), 2);
        // Its in the correct order
        assert_eq!(x.unwrap(), vec![input[3], input[1]]);

        // TESt 5 all

        let x = effect_map.causal_order_from_effects(input.iter(), &mut cps);

        println!("result: {:?}", x);
        assert_eq!(x.clone().unwrap().len(), 4);
    }
}
