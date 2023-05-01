// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};
use sui_types::base_types::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::storage::ObjectKey;
use tracing::trace;

pub struct CasualOrder {
    not_seen: BTreeMap<TransactionDigest, TransactionDependencies>,
    output: Vec<TransactionEffects>,
}

impl CasualOrder {
    /// Casually sort given vector of effects
    ///
    /// Returned list has effects that
    ///
    /// (a) Casually sorted
    /// (b) Have deterministic order between transactions that are not casually dependent
    ///
    /// The order of result list does not depend on order of effects in the supplied vector
    pub fn casual_sort(effects: Vec<TransactionEffects>) -> Vec<TransactionEffects> {
        let mut this = Self::from_vec(effects);
        while let Some(item) = this.pop_first() {
            this.insert(item);
        }
        this.into_list()
    }

    fn from_vec(effects: Vec<TransactionEffects>) -> Self {
        let rwlock_builder = RWLockDependencyBuilder::from_effects(&effects);
        let dependencies: Vec<_> = effects
            .into_iter()
            .map(|e| TransactionDependencies::from_effects(e, &rwlock_builder))
            .collect();
        let output = Vec::with_capacity(dependencies.len() * 2);
        let not_seen = dependencies.into_iter().map(|e| (e.digest, e)).collect();
        Self { not_seen, output }
    }

    fn pop_first(&mut self) -> Option<TransactionDependencies> {
        // Once map_first_last is stabilized this function can be rewritten as this:
        // self.not_seen.pop_first()
        let key = *self.not_seen.keys().next()?;
        Some(self.not_seen.remove(&key).unwrap())
    }

    // effect is already removed from self.not_seen at this point
    fn insert(&mut self, transaction: TransactionDependencies) {
        let initial_state = InsertState::new(transaction);
        let mut states = vec![initial_state];

        while let Some(state) = states.last_mut() {
            if let Some(new_state) = state.process(self) {
                // This is essentially a 'recursive call' but using heap instead of stack to store state
                states.push(new_state);
            } else {
                // Done with current state, remove it
                states.pop().expect("Should contain an element");
            }
        }
    }

    fn into_list(self) -> Vec<TransactionEffects> {
        self.output
    }
}

struct TransactionDependencies {
    digest: TransactionDigest,
    dependencies: BTreeSet<TransactionDigest>,
    effects: TransactionEffects,
}

impl TransactionDependencies {
    fn from_effects(effects: TransactionEffects, rwlock_builder: &RWLockDependencyBuilder) -> Self {
        let mut dependencies: BTreeSet<_> = effects.dependencies().iter().cloned().collect();
        rwlock_builder.add_dependencies_for(*effects.transaction_digest(), &mut dependencies);
        Self {
            digest: *effects.transaction_digest(),
            dependencies,
            effects,
        }
    }
}

/// Supplies TransactionDependencies tree with additional edges from transactions
/// that write shared locks object to transactions that read previous version of this object.
///
/// With RWLocks we can have multiple transaction that depend on shared object version N - many read
/// transactions and single write transaction. Those transactions depend on transaction that has written N,
/// but they do not depend on each other. And specifically, transaction that reads N and writes N+1
/// does not depend on read-only transactions that also read N.
///
/// We do not add such read transactions to TransactionEffects of shared object write transactions
/// for next version to make sure TransactionEffects are not grow too large
/// (and because you do not need read transactions to replay write transaction for next version).
///
/// However, when building checkpoints we supply transaction dependency tree with additional dependency edges to
/// make it look like write transaction for next version casually depends on transactions that read
/// previous versions, for two reasons:
///
/// (1) Without this addition we could have peculiar checkpoints where transaction reading
/// version N appears after transaction that overwritten this object with version N+1.
/// This does not affect how transaction is executed, but it is not something one would expect in
/// casually ordered list.
///
/// (2) On the practical side it will allow to simplify pruner as it can now just tail checkpoints
/// and delete objects in order they appear in TransactionEffects::modified_at_versions in checkpoint.
struct RWLockDependencyBuilder {
    read_version: HashMap<ObjectKey, Vec<TransactionDigest>>,
    overwrite_versions: HashMap<TransactionDigest, Vec<ObjectKey>>,
}

impl RWLockDependencyBuilder {
    pub fn from_effects(effects: &[TransactionEffects]) -> Self {
        let mut read_version: HashMap<ObjectKey, Vec<TransactionDigest>> = Default::default();
        let mut overwrite_versions: HashMap<TransactionDigest, Vec<ObjectKey>> = Default::default();
        for effect in effects {
            let modified_at_versions: HashMap<_, _> =
                effect.modified_at_versions().iter().cloned().collect();
            for (obj, seq, _) in effect.shared_objects().iter() {
                if let Some(modified_seq) = modified_at_versions.get(obj) {
                    // write transaction
                    overwrite_versions
                        .entry(*effect.transaction_digest())
                        .or_default()
                        .push(ObjectKey(*obj, *modified_seq));
                } else {
                    // Read only transaction
                    read_version
                        .entry(ObjectKey(*obj, *seq))
                        .or_default()
                        .push(*effect.transaction_digest());
                }
            }
        }
        Self {
            read_version,
            overwrite_versions,
        }
    }

    pub fn add_dependencies_for(
        &self,
        digest: TransactionDigest,
        v: &mut BTreeSet<TransactionDigest>,
    ) {
        let Some(overwrites) = self.overwrite_versions.get(&digest) else {return;};
        for obj_ver in overwrites {
            let Some(reads) = self.read_version.get(obj_ver) else {continue;};
            for dep in reads {
                trace!(
                    "Assuming additional dependency when constructing checkpoint {:?} -> {:?}",
                    digest,
                    *dep
                );
                v.insert(*dep);
            }
        }
    }
}

struct InsertState {
    dependencies: Vec<TransactionDigest>,
    transaction: Option<TransactionDependencies>,
}

impl InsertState {
    pub fn new(transaction: TransactionDependencies) -> Self {
        Self {
            dependencies: transaction.dependencies.iter().cloned().collect(),
            transaction: Some(transaction),
        }
    }

    pub fn process(&mut self, casual_order: &mut CasualOrder) -> Option<InsertState> {
        while let Some(dep) = self.dependencies.pop() {
            if let Some(dep_transaction) = casual_order.not_seen.remove(&dep) {
                return Some(InsertState::new(dep_transaction));
            }
        }
        let transaction = self
            .transaction
            .take()
            .expect("Can't use InsertState after it is finished");
        casual_order.output.push(transaction.effects);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::ObjectDigest;
    use sui_types::base_types::{ObjectID, SequenceNumber};
    use sui_types::effects::TransactionEffects;

    #[test]
    pub fn test_casual_order() {
        let e1 = e(d(1), vec![d(2), d(3)]);
        let e2 = e(d(2), vec![d(3), d(4)]);
        let e3 = e(d(3), vec![]);
        let e4 = e(d(4), vec![]);

        let r = extract(CasualOrder::casual_sort(vec![
            e1.clone(),
            e2,
            e3,
            e4.clone(),
        ]));
        assert_eq!(r, vec![3, 4, 2, 1]);

        // e1 and e4 are not (directly) casually dependent - ordered lexicographically
        let r = extract(CasualOrder::casual_sort(vec![e1.clone(), e4.clone()]));
        assert_eq!(r, vec![1, 4]);
        let r = extract(CasualOrder::casual_sort(vec![e4, e1]));
        assert_eq!(r, vec![1, 4]);
    }

    #[test]
    pub fn test_casual_order_rw_locks() {
        let mut e5 = e(d(5), vec![]);
        let mut e2 = e(d(2), vec![]);
        let mut e3 = e(d(3), vec![]);
        let obj_digest = ObjectDigest::new(Default::default());
        e5.shared_objects_mut_for_testing()
            .push((o(1), SequenceNumber::from_u64(1), obj_digest));
        e2.shared_objects_mut_for_testing()
            .push((o(1), SequenceNumber::from_u64(1), obj_digest));
        e3.shared_objects_mut_for_testing()
            .push((o(1), SequenceNumber::from_u64(1), obj_digest));

        e3.modified_at_versions_mut_for_testing()
            .push((o(1), SequenceNumber::from_u64(1)));
        let r = extract(CasualOrder::casual_sort(vec![e5, e2, e3]));
        assert_eq!(r.len(), 3);
        assert_eq!(*r.get(2).unwrap(), 3); // [3] is the last
                                           // both [5] and [2] are present (but order is not fixed)
        assert!(r.contains(&5));
        assert!(r.contains(&2));
    }

    fn extract(e: Vec<TransactionEffects>) -> Vec<u8> {
        e.into_iter()
            .map(|e| e.transaction_digest().inner()[0])
            .collect()
    }

    fn d(i: u8) -> TransactionDigest {
        let mut bytes: [u8; 32] = Default::default();
        bytes[0] = i;
        TransactionDigest::new(bytes)
    }

    fn o(i: u8) -> ObjectID {
        let mut bytes: [u8; ObjectID::LENGTH] = Default::default();
        bytes[0] = i;
        ObjectID::new(bytes)
    }

    fn e(
        transaction_digest: TransactionDigest,
        dependencies: Vec<TransactionDigest>,
    ) -> TransactionEffects {
        let mut effects = TransactionEffects::default();
        *effects.transaction_digest_mut_for_testing() = transaction_digest;
        *effects.dependencies_mut_for_testing() = dependencies;
        effects
    }
}
