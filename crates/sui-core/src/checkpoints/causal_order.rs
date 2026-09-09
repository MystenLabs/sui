// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use sui_types::base_types::{FullObjectID, ObjectID, SequenceNumber, TransactionDigest};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::{InputConsensusObject, TransactionEffects};
use sui_types::storage::{FullObjectKey, ObjectKey};
use sui_types::transaction::TransactionDataAPI;
use tracing::trace;

use crate::execution_cache::TransactionCacheRead;

type ConsensusStartVersions = HashMap<(TransactionDigest, ObjectID), SequenceNumber>;

pub struct CausalOrder {
    not_seen: BTreeMap<TransactionDigest, TransactionDependencies>,
    output: Vec<TransactionEffects>,
}

impl CausalOrder {
    /// Causally sort effects, extracting the consensus commit prologue (if present at index 0)
    /// and placing it first in the result. The CCP is identified by its digest matching
    /// `ccp_digest`. All other effects are causally sorted after the CCP.
    ///
    /// Pass `transaction_cache` only when effects dependencies are disabled. In that mode,
    /// causality is limited to object-version and consensus-marker ordering needed for
    /// cache commits and shared-object reads/writes. Immutable-input producer dependencies
    /// are not retained.
    pub fn causal_sort_with_ccp(
        effects: Vec<TransactionEffects>,
        ccp_digest: Option<TransactionDigest>,
        transaction_cache: Option<&dyn TransactionCacheRead>,
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
        // Effects omit consensus stream start versions. Read transaction inputs only when
        // reconstructing dependencies, so distinct streams cannot introduce false edges.
        let start_versions = transaction_cache.map(|cache| {
            let digests: Vec<_> = unsorted.iter().map(|e| *e.transaction_digest()).collect();
            cache
                .multi_get_transaction_blocks(&digests)
                .into_iter()
                .zip_eq(digests)
                .flat_map(|(transaction, digest)| {
                    transaction
                        .expect("executed transaction must exist")
                        .data()
                        .transaction_data()
                        .shared_input_objects()
                        .into_iter()
                        .map(move |input| ((digest, input.id), input.initial_shared_version))
                })
                .collect()
        });
        sorted.extend(Self::causal_sort(unsorted, start_versions.as_ref()));
        sorted
    }

    /// Deterministically topologically sort the dependency graph built by `from_vec`.
    /// The result does not depend on the input order.
    fn causal_sort(
        effects: Vec<TransactionEffects>,
        start_versions: Option<&ConsensusStartVersions>,
    ) -> Vec<TransactionEffects> {
        let mut this = Self::from_vec(effects, start_versions);
        while let Some(item) = this.pop_first() {
            this.insert(item);
        }
        this.into_list()
    }

    fn from_vec(
        effects: Vec<TransactionEffects>,
        start_versions: Option<&ConsensusStartVersions>,
    ) -> Self {
        let rwlock_builder = RWLockDependencyBuilder::from_effects(&effects, start_versions);
        // Cache commits must flush successive versions of each object in order, even when
        // execution no longer records transaction dependencies in effects.
        let writers = start_versions.map(|start_versions| {
            let mut writers = HashMap::new();
            for effect in &effects {
                let digest = *effect.transaction_digest();
                for (id, version, _) in effect.written() {
                    writers.insert(FullObjectKey::Fastpath(ObjectKey(id, version)), digest);
                }
                let ended_streams = effect
                    .all_tombstones()
                    .into_iter()
                    .chain(
                        effect
                            .transferred_from_consensus()
                            .into_iter()
                            .chain(effect.consensus_owner_changed())
                            .map(|(id, version, _)| (id, version)),
                    )
                    .chain(
                        effect
                            .stream_ended_mutably_accessed_consensus_objects()
                            .into_iter()
                            .map(|id| (id, effect.lamport_version())),
                    );
                for (id, version) in ended_streams {
                    if let Some(start) = start_versions.get(&(digest, id)) {
                        let key = FullObjectKey::new(FullObjectID::new(id, Some(*start)), version);
                        writers.insert(key, digest);
                    }
                }
            }
            writers
        });
        let dependencies: Vec<_> = effects
            .into_iter()
            .map(|e| {
                TransactionDependencies::from_effects(
                    e,
                    &rwlock_builder,
                    writers.as_ref(),
                    start_versions,
                )
            })
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
    fn from_effects(
        effects: TransactionEffects,
        rwlock_builder: &RWLockDependencyBuilder,
        writers: Option<&HashMap<FullObjectKey, TransactionDigest>>,
        start_versions: Option<&ConsensusStartVersions>,
    ) -> Self {
        let mut dependencies: BTreeSet<_> = effects.dependencies().iter().cloned().collect();
        rwlock_builder.add_dependencies_for(*effects.transaction_digest(), &mut dependencies);
        if let Some(writers) = writers {
            let inputs = effects
                .modified_at_versions()
                .into_iter()
                .map(|(id, version)| FullObjectKey::Fastpath(ObjectKey(id, version)))
                .chain(
                    effects.accessed_consensus_objects().into_iter().filter_map(
                        |input| match input {
                            InputConsensusObject::Mutate((id, version, _))
                            | InputConsensusObject::ReadOnly((id, version, _)) => {
                                Some(FullObjectKey::Fastpath(ObjectKey(id, version)))
                            }
                            InputConsensusObject::ReadConsensusStreamEnded(id, version)
                            | InputConsensusObject::MutateConsensusStreamEnded(id, version) => {
                                Some(consensus_key(
                                    start_versions,
                                    effects.transaction_digest(),
                                    id,
                                    version,
                                ))
                            }
                            InputConsensusObject::Cancelled(..) => None,
                        },
                    ),
                );
            for key in inputs {
                if let Some(writer) = writers.get(&key) {
                    dependencies.insert(*writer);
                }
            }
        }
        Self {
            digest: *effects.transaction_digest(),
            dependencies,
            effects,
        }
    }
}

fn consensus_key(
    start_versions: Option<&ConsensusStartVersions>,
    digest: &TransactionDigest,
    id: ObjectID,
    version: SequenceNumber,
) -> FullObjectKey {
    FullObjectKey::new(
        FullObjectID::new(id, start_versions.map(|versions| versions[&(*digest, id)])),
        version,
    )
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
    read_version: HashMap<FullObjectKey, Vec<TransactionDigest>>,
    overwrite_versions: HashMap<TransactionDigest, Vec<FullObjectKey>>,
}

impl RWLockDependencyBuilder {
    pub fn from_effects(
        effects: &[TransactionEffects],
        start_versions: Option<&ConsensusStartVersions>,
    ) -> Self {
        let mut read_version: HashMap<FullObjectKey, Vec<TransactionDigest>> = Default::default();
        let mut overwrite_versions: HashMap<TransactionDigest, Vec<FullObjectKey>> =
            Default::default();
        for effect in effects {
            for kind in effect.accessed_consensus_objects() {
                match kind {
                    InputConsensusObject::ReadOnly(obj_ref) => {
                        // Live versions uniquely identify their stored object, including
                        // implicit system reads absent from transaction-declared inputs.
                        let obj_key = FullObjectKey::Fastpath(obj_ref.into());
                        // Read only transaction
                        read_version
                            .entry(obj_key)
                            .or_default()
                            .push(*effect.transaction_digest());
                    }
                    InputConsensusObject::Mutate(obj_ref) => {
                        let obj_key = FullObjectKey::Fastpath(obj_ref.into());
                        // write transaction
                        overwrite_versions
                            .entry(*effect.transaction_digest())
                            .or_default()
                            .push(obj_key);
                    }
                    InputConsensusObject::ReadConsensusStreamEnded(oid, version) => read_version
                        .entry(consensus_key(
                            start_versions,
                            effect.transaction_digest(),
                            oid,
                            version,
                        ))
                        .or_default()
                        .push(*effect.transaction_digest()),
                    InputConsensusObject::MutateConsensusStreamEnded(oid, version) => {
                        overwrite_versions
                            .entry(*effect.transaction_digest())
                            .or_default()
                            .push(consensus_key(
                                start_versions,
                                effect.transaction_digest(),
                                oid,
                                version,
                            ))
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
    use sui_types::effects::{EffectsObjectChange, UnchangedConsensusKind};
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::GasCostSummary;
    use sui_types::object::{Object, Owner};

    fn sort_by_object_versions(effects: Vec<TransactionEffects>) -> Vec<TransactionEffects> {
        let start_versions = effects
            .iter()
            .flat_map(|effect| {
                effect
                    .accessed_consensus_objects()
                    .into_iter()
                    .map(|input| {
                        (
                            (*effect.transaction_digest(), input.id_and_version().0),
                            SequenceNumber::from_u64(1),
                        )
                    })
            })
            .collect();
        CausalOrder::causal_sort(effects, Some(&start_versions))
    }

    #[tokio::test]
    async fn test_object_version_dependencies_without_effects_dependencies() {
        let owner = Owner::Shared {
            initial_shared_version: SequenceNumber::from_u64(1),
        };
        let digest = ObjectDigest::MIN;
        let object_effect = |tx, old_version: Option<u64>, version, deleted: bool, owner: Owner| {
            let written = (!deleted).then(|| {
                Object::with_id_owner_version_for_testing(
                    o(1),
                    SequenceNumber::from_u64(version),
                    owner.clone(),
                )
            });
            TransactionEffects::new_from_execution_v2(
                ExecutionStatus::Success,
                0,
                GasCostSummary::default(),
                vec![],
                d(tx),
                SequenceNumber::from_u64(version),
                BTreeMap::from([(
                    o(1),
                    EffectsObjectChange::new(
                        old_version.map(|v| ((SequenceNumber::from_u64(v), digest), owner.clone())),
                        written.as_ref(),
                        old_version.is_none(),
                        deleted,
                    ),
                )]),
                None,
                None,
                vec![],
            )
        };
        let created = object_effect(3, None, 10, false, owner.clone());
        let received = object_effect(2, Some(10), 11, false, owner.clone());
        let deleted = object_effect(1, Some(11), 12, true, owner);
        let ended_effect = |tx, input_version, output_version, mutable| {
            TransactionEffects::new_from_execution_v2(
                ExecutionStatus::Success,
                0,
                GasCostSummary::default(),
                vec![(
                    o(1),
                    if mutable {
                        UnchangedConsensusKind::MutateConsensusStreamEnded(
                            SequenceNumber::from_u64(input_version),
                        )
                    } else {
                        UnchangedConsensusKind::ReadConsensusStreamEnded(SequenceNumber::from_u64(
                            input_version,
                        ))
                    },
                )],
                d(tx),
                SequenceNumber::from_u64(output_version),
                BTreeMap::new(),
                None,
                None,
                vec![],
            )
        };
        // Successive ended-stream markers also have to flush in version order. The read
        // has a higher gas-derived Lamport version, so sorting only by Lamport is not enough.
        let smear = ended_effect(4, 12, 13, true);
        let read = ended_effect(5, 13, 100, false);
        let next_smear = ended_effect(0, 13, 14, true);
        let mut effects = vec![next_smear, read, smear, deleted, received, created];
        assert_eq!(
            extract(sort_by_object_versions(effects.clone())),
            vec![3, 2, 1, 4, 5, 0]
        );
        effects.reverse();
        assert_eq!(
            extract(sort_by_object_versions(effects)),
            vec![3, 2, 1, 4, 5, 0]
        );

        // An owned write and a smear of its former consensus stream can have the same
        // object ID and version. Neither producer may overwrite the other in the graph.
        let owner = Owner::AddressOwner(Default::default());
        let transferred = object_effect(9, None, 10, false, owner.clone());
        let mutated = object_effect(8, Some(10), 11, false, owner.clone());
        let smear = ended_effect(7, 10, 11, true);
        let consumer = object_effect(1, Some(11), 12, false, owner);
        let mut effects = vec![mutated, smear, consumer, transferred];
        assert_eq!(
            extract(sort_by_object_versions(effects.clone())),
            vec![9, 8, 1, 7]
        );
        effects.reverse();
        assert_eq!(extract(sort_by_object_versions(effects)), vec![9, 8, 1, 7]);
    }

    #[tokio::test]
    async fn test_ended_stream_does_not_depend_on_live_object_version() {
        let shared_owner = Owner::Shared {
            initial_shared_version: SequenceNumber::from_u64(1),
        };
        let change = |id, input, output, owner: Owner| {
            let object = Object::with_id_owner_version_for_testing(
                o(id),
                SequenceNumber::from_u64(output),
                owner.clone(),
            );
            (
                o(id),
                EffectsObjectChange::new(
                    Some(((SequenceNumber::from_u64(input), ObjectDigest::MIN), owner)),
                    Some(&object),
                    false,
                    false,
                ),
            )
        };
        let effect = |tx, version, unchanged, changes| {
            TransactionEffects::new_from_execution_v2(
                ExecutionStatus::Success,
                0,
                GasCostSummary::default(),
                unchanged,
                d(tx),
                SequenceNumber::from_u64(version),
                changes,
                None,
                None,
                vec![],
            )
        };
        let read_x = (
            o(2),
            UnchangedConsensusKind::ReadOnlyRoot((SequenceNumber::from_u64(1), ObjectDigest::MIN)),
        );
        let c = effect(
            1,
            12,
            vec![
                (
                    o(1),
                    UnchangedConsensusKind::ReadConsensusStreamEnded(SequenceNumber::from_u64(11)),
                ),
                read_x.clone(),
            ],
            BTreeMap::from([change(3, 1, 12, shared_owner.clone())]),
        );
        let d_effect = effect(
            2,
            13,
            vec![read_x],
            BTreeMap::from([change(3, 12, 13, shared_owner.clone())]),
        );
        let a = effect(
            3,
            11,
            vec![],
            BTreeMap::from([
                change(1, 10, 11, Owner::AddressOwner(Default::default())),
                change(2, 1, 11, shared_owner.clone()),
            ]),
        );
        // The old marker O@11 predates this batch. The unrelated live write O@11
        // must not create C -> A: shared X already requires A -> D -> C.
        for effects in [
            vec![c.clone(), d_effect.clone(), a.clone()],
            vec![a, d_effect.clone(), c.clone()],
        ] {
            assert_eq!(extract(sort_by_object_versions(effects)), vec![1, 2, 3]);
        }

        // Distinct ended consensus streams of the same object can also reach the
        // same numeric version. Stream start versions must disambiguate their markers.
        let other_stream = effect(
            3,
            11,
            vec![(
                o(1),
                UnchangedConsensusKind::MutateConsensusStreamEnded(SequenceNumber::from_u64(10)),
            )],
            BTreeMap::from([change(2, 1, 11, shared_owner)]),
        );
        let start_versions = HashMap::from([
            ((d(1), o(1)), SequenceNumber::from_u64(1)),
            ((d(1), o(2)), SequenceNumber::from_u64(1)),
            ((d(1), o(3)), SequenceNumber::from_u64(1)),
            ((d(2), o(2)), SequenceNumber::from_u64(1)),
            ((d(2), o(3)), SequenceNumber::from_u64(1)),
            ((d(3), o(1)), SequenceNumber::from_u64(2)),
            ((d(3), o(2)), SequenceNumber::from_u64(1)),
        ]);
        for effects in [
            vec![c.clone(), d_effect.clone(), other_stream.clone()],
            vec![other_stream, d_effect, c],
        ] {
            assert_eq!(
                extract(CausalOrder::causal_sort(effects, Some(&start_versions))),
                vec![1, 2, 3]
            );
        }
    }

    #[tokio::test]
    async fn test_implicit_shared_read_orders_before_overwrite() {
        let id = sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
        let version = SequenceNumber::from_u64(1);
        let object_ref = (id, version, ObjectDigest::MIN);
        let mut reader = e(d(2), vec![]);
        reader.unsafe_add_input_consensus_object_for_testing(InputConsensusObject::ReadOnly(
            object_ref,
        ));
        let mut writer = e(d(1), vec![]);
        writer.unsafe_add_input_consensus_object_for_testing(InputConsensusObject::Mutate(
            object_ref,
        ));
        // A withdrawal's root read is recorded in effects, not its declared inputs.
        let start_versions = HashMap::from([((d(1), id), version)]);
        assert_eq!(
            extract(CausalOrder::causal_sort(
                vec![writer, reader],
                Some(&start_versions),
            )),
            vec![2, 1]
        );
    }

    #[test]
    pub fn test_causal_order() {
        let e1 = e(d(1), vec![d(2), d(3)]);
        let e2 = e(d(2), vec![d(3), d(4)]);
        let e3 = e(d(3), vec![]);
        let e4 = e(d(4), vec![]);

        let r = extract(CausalOrder::causal_sort(
            vec![e1.clone(), e2, e3, e4.clone()],
            None,
        ));
        assert_eq!(r, vec![3, 4, 2, 1]);

        // e1 and e4 are not (directly) causally dependent - ordered lexicographically
        let r = extract(CausalOrder::causal_sort(vec![e1.clone(), e4.clone()], None));
        assert_eq!(r, vec![1, 4]);
        let r = extract(CausalOrder::causal_sort(vec![e4, e1], None));
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

        let r = extract(CausalOrder::causal_sort(vec![e5, e2, e3], None));
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
