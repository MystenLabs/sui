// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};
use sui_types::base_types::{ObjectID, SequenceNumber, TransactionDigest};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::{InputConsensusObject, TransactionEffects};
use sui_types::storage::ObjectKey;
use tracing::trace;

pub struct CausalOrder {
    not_seen: BTreeMap<TransactionDigest, TransactionDependencies>,
    output: Vec<TransactionEffects>,
}

impl CausalOrder {
    /// Causally sort effects, extracting the consensus commit prologue (if present at index 0)
    /// and placing it first in the result. The CCP is identified by its digest matching
    /// `ccp_digest`. All other effects are causally sorted after the CCP.
    pub fn causal_sort_with_ccp(
        effects: Vec<TransactionEffects>,
        ccp_digest: Option<TransactionDigest>,
    ) -> Vec<TransactionEffects> {
        let (ccp_effects, unsorted) = if let Some(digest) = ccp_digest {
            assert_eq!(effects[0].transaction_digest(), &digest);
            (Some(effects[0].clone()), effects[1..].to_vec())
        } else {
            (None, effects)
        };

        let mut sorted: Vec<TransactionEffects> = Vec::with_capacity(unsorted.len() + 1);
        if let Some(ccp) = ccp_effects {
            if cfg!(debug_assertions) {
                let ccp_digest = ccp_digest.unwrap();
                for tx in unsorted.iter() {
                    assert!(tx.transaction_digest() != &ccp_digest);
                }
            }
            sorted.push(ccp);
        }
        sorted.extend(Self::causal_sort(unsorted));
        sorted
    }

    /// Causally sort given vector of effects
    ///
    /// Returned list has effects that
    ///
    /// (a) Causally sorted
    /// (b) Have deterministic order between transactions that are not causally dependent
    ///
    /// The order of result list does not depend on order of effects in the supplied vector
    pub fn causal_sort(effects: Vec<TransactionEffects>) -> Vec<TransactionEffects> {
        let mut this = Self::from_vec(effects);
        while let Some(item) = this.pop_first() {
            this.insert(item);
        }
        this.into_list()
    }

    /// Checks, using only object versions recorded in effects (never `dependencies()`),
    /// that `effects` in the given order is already a valid causal order.
    ///
    /// The invariant is that each object's versions advance monotonically in batch order:
    /// the first read of an object seeds its latest version, every later read must see that
    /// latest version, and every write must produce a strictly newer version. Read-only inputs
    /// therefore never advance the latest version. This subsumes the RWLock rule (a read-only
    /// reader of N must precede the writer of N+1) and catches reads of versions produced by a
    /// later transaction, including newly created objects.
    ///
    /// Reads of immutable objects and packages are not recorded in effects and are not checked.
    /// Returns a description of the first violation found.
    pub fn check_already_sorted(effects: &[TransactionEffects]) -> Result<(), String> {
        let mut latest: HashMap<ObjectID, (SequenceNumber, usize)> = HashMap::new();
        for (idx, e) in effects.iter().enumerate() {
            let digest = e.transaction_digest();
            for key in Self::input_versions(e) {
                match latest.get(&key.0) {
                    Some(&(seen, at)) if seen != key.1 => {
                        return Err(format!(
                            "stale read: tx {digest:?} at index {idx} reads {key:?}, but the \
                             latest version observed (at index {at}) is {seen:?}"
                        ));
                    }
                    Some(_) => {}
                    None => {
                        latest.insert(key.0, (key.1, idx));
                    }
                }
            }
            for key in Self::output_versions(e) {
                if let Some(&(seen, at)) = latest.get(&key.0)
                    && seen >= key.1
                {
                    return Err(format!(
                        "non-monotonic write: tx {digest:?} at index {idx} writes {key:?}, but \
                         version {seen:?} was already observed at index {at}"
                    ));
                }
                latest.insert(key.0, (key.1, idx));
            }
        }
        Ok(())
    }

    fn input_versions(e: &TransactionEffects) -> impl Iterator<Item = ObjectKey> + '_ {
        e.modified_at_versions()
            .into_iter()
            .map(|(id, v)| ObjectKey(id, v))
            .chain(
                e.accessed_consensus_objects()
                    .into_iter()
                    .filter_map(|kind| match kind {
                        InputConsensusObject::Mutate(r) | InputConsensusObject::ReadOnly(r) => {
                            Some(ObjectKey(r.0, r.1))
                        }
                        InputConsensusObject::ReadConsensusStreamEnded(id, v)
                        | InputConsensusObject::MutateConsensusStreamEnded(id, v) => {
                            Some(ObjectKey(id, v))
                        }
                        InputConsensusObject::Cancelled(..) => None,
                    }),
            )
    }

    fn output_versions(e: &TransactionEffects) -> impl Iterator<Item = ObjectKey> + '_ {
        e.all_changed_objects()
            .into_iter()
            .map(|(r, _, _)| ObjectKey(r.0, r.1))
            .chain(
                e.all_removed_objects()
                    .into_iter()
                    .map(|(r, _)| ObjectKey(r.0, r.1)),
            )
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
/// make it look like write transaction for next version causally depends on transactions that read
/// previous versions, for two reasons:
///
/// (1) Without this addition we could have peculiar checkpoints where transaction reading
/// version N appears after transaction that overwritten this object with version N+1.
/// This does not affect how transaction is executed, but it is not something one would expect in
/// causally ordered list.
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
            for kind in effect.accessed_consensus_objects() {
                match kind {
                    InputConsensusObject::ReadOnly(obj_ref) => {
                        let obj_key = obj_ref.into();
                        // Read only transaction
                        read_version
                            .entry(obj_key)
                            .or_default()
                            .push(*effect.transaction_digest());
                    }
                    InputConsensusObject::Mutate(obj_ref) => {
                        let obj_key = obj_ref.into();
                        // write transaction
                        overwrite_versions
                            .entry(*effect.transaction_digest())
                            .or_default()
                            .push(obj_key);
                    }
                    InputConsensusObject::ReadConsensusStreamEnded(oid, version) => read_version
                        .entry(ObjectKey(oid, version))
                        .or_default()
                        .push(*effect.transaction_digest()),
                    InputConsensusObject::MutateConsensusStreamEnded(oid, version) => {
                        overwrite_versions
                            .entry(*effect.transaction_digest())
                            .or_default()
                            .push(ObjectKey(oid, version))
                    }
                    InputConsensusObject::Cancelled(..) => (),
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
        let Some(overwrites) = self.overwrite_versions.get(&digest) else {
            return;
        };
        for obj_ver in overwrites {
            let Some(reads) = self.read_version.get(obj_ver) else {
                continue;
            };
            for dep in reads {
                trace!(
                    "Assuming additional dependency when constructing checkpoint {:?} -> {:?}",
                    digest, *dep
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

    pub fn process(&mut self, causal_order: &mut CausalOrder) -> Option<InsertState> {
        while let Some(dep) = self.dependencies.pop() {
            if let Some(dep_transaction) = causal_order.not_seen.remove(&dep) {
                return Some(InsertState::new(dep_transaction));
            }
        }
        let transaction = self
            .transaction
            .take()
            .expect("Can't use InsertState after it is finished");
        causal_order.output.push(transaction.effects);
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
    pub fn test_causal_order() {
        let e1 = e(d(1), vec![d(2), d(3)]);
        let e2 = e(d(2), vec![d(3), d(4)]);
        let e3 = e(d(3), vec![]);
        let e4 = e(d(4), vec![]);

        let r = extract(CausalOrder::causal_sort(vec![
            e1.clone(),
            e2,
            e3,
            e4.clone(),
        ]));
        assert_eq!(r, vec![3, 4, 2, 1]);

        // e1 and e4 are not (directly) causally dependent - ordered lexicographically
        let r = extract(CausalOrder::causal_sort(vec![e1.clone(), e4.clone()]));
        assert_eq!(r, vec![1, 4]);
        let r = extract(CausalOrder::causal_sort(vec![e4, e1]));
        assert_eq!(r, vec![1, 4]);
    }

    #[test]
    pub fn test_causal_order_rw_locks() {
        let mut e5 = e(d(5), vec![]);
        let mut e2 = e(d(2), vec![]);
        let mut e3 = e(d(3), vec![]);
        let obj_digest = ObjectDigest::new(Default::default());
        e5.unsafe_add_input_consensus_object_for_testing(InputConsensusObject::ReadOnly((
            o(1),
            SequenceNumber::from_u64(1),
            obj_digest,
        )));
        e2.unsafe_add_input_consensus_object_for_testing(InputConsensusObject::ReadOnly((
            o(1),
            SequenceNumber::from_u64(1),
            obj_digest,
        )));
        e3.unsafe_add_input_consensus_object_for_testing(InputConsensusObject::Mutate((
            o(1),
            SequenceNumber::from_u64(1),
            obj_digest,
        )));

        let r = extract(CausalOrder::causal_sort(vec![e5, e2, e3]));
        assert_eq!(r.len(), 3);
        assert_eq!(*r.get(2).unwrap(), 3); // [3] is the last
        // both [5] and [2] are present (but order is not fixed)
        assert!(r.contains(&5));
        assert!(r.contains(&2));
    }

    #[test]
    pub fn test_check_already_sorted() {
        // writer produces (o1, 5); consumer mutates (o1, 5) -> (o1, 7).
        let writer = tx(d(1), 5, &[(o(1), 3)], &[]);
        let consumer = tx(d(2), 7, &[(o(1), 5)], &[]);
        assert!(CausalOrder::check_already_sorted(&[writer.clone(), consumer.clone()]).is_ok());
        let err = CausalOrder::check_already_sorted(&[consumer, writer.clone()]).unwrap_err();
        assert!(err.starts_with("stale read"), "{err}");

        // read-only reader of (o1, 5) must precede the tx that overwrites it.
        let reader = tx(d(3), 6, &[], &[(o(1), 5)]);
        let overwriter = tx(d(4), 8, &[(o(1), 5)], &[]);
        assert!(
            CausalOrder::check_already_sorted(&[
                writer.clone(),
                reader.clone(),
                overwriter.clone()
            ])
            .is_ok()
        );
        let err = CausalOrder::check_already_sorted(&[writer.clone(), overwriter, reader.clone()])
            .unwrap_err();
        assert!(err.starts_with("stale read"), "{err}");

        // Readers of a deleted consensus object record the deleting tx's lamport version.
        let mut deleter = tx(d(5), 9, &[], &[]);
        deleter.unsafe_add_object_tombstone_for_testing((
            o(1),
            SequenceNumber::from_u64(5),
            ObjectDigest::new(Default::default()),
        ));
        let mut ended_reader = tx(d(6), 10, &[], &[]);
        ended_reader.unsafe_add_input_consensus_object_for_testing(
            InputConsensusObject::ReadConsensusStreamEnded(o(1), SequenceNumber::from_u64(9)),
        );
        assert!(
            CausalOrder::check_already_sorted(&[
                writer.clone(),
                deleter.clone(),
                ended_reader.clone()
            ])
            .is_ok()
        );
        let err = CausalOrder::check_already_sorted(&[writer.clone(), ended_reader, deleter])
            .unwrap_err();
        assert!(err.starts_with("stale read"), "{err}");

        // A tx that reads an object created by a later tx has no earlier version to compare
        // against, so the violation is caught on the write side.
        let mut creator = tx(d(7), 5, &[], &[]);
        creator.unsafe_add_created_object_for_testing((
            o(1),
            SequenceNumber::from_u64(5),
            ObjectDigest::new(Default::default()),
        ));
        assert!(CausalOrder::check_already_sorted(&[creator.clone(), reader.clone()]).is_ok());
        let err = CausalOrder::check_already_sorted(&[reader, creator]).unwrap_err();
        assert!(err.starts_with("non-monotonic write"), "{err}");
    }

    /// Effects for a tx with the given lamport version that mutates each `(id, input_version)`
    /// in `mutated` and reads each `(id, version)` in `read_only` as a consensus object.
    fn tx(
        digest: TransactionDigest,
        lamport: u64,
        mutated: &[(ObjectID, u64)],
        read_only: &[(ObjectID, u64)],
    ) -> TransactionEffects {
        use sui_types::execution_status::ExecutionStatus;
        use sui_types::gas::GasCostSummary;

        let obj_digest = ObjectDigest::new(Default::default());
        let mut effects = TransactionEffects::new_from_execution_v2(
            ExecutionStatus::Success,
            0,
            GasCostSummary::default(),
            vec![],
            digest,
            SequenceNumber::from_u64(lamport),
            Default::default(),
            None,
            None,
            vec![],
        );
        for (id, v) in mutated {
            effects.unsafe_add_deleted_live_object_for_testing((
                *id,
                SequenceNumber::from_u64(*v),
                obj_digest,
            ));
        }
        for (id, v) in read_only {
            effects.unsafe_add_input_consensus_object_for_testing(InputConsensusObject::ReadOnly(
                (*id, SequenceNumber::from_u64(*v), obj_digest),
            ));
        }
        effects
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
