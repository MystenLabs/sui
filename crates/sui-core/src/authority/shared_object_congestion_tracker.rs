// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::DeferralKey;
use narwhal_types::Round;
use std::collections::HashMap;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::SharedInputObject;

// SharedObjectCongestionTracker stores the accumulated cost of executing transactions on an object, for
// all transactions in a consensus commit.
//
// Cost is an indication of transaction execution latency. When transactions are scheduled by
// the consensus handler, each scheduled transaction adds cost (execution latency) to all the objects it
// reads or writes.
//
// The goal of this data structure is to capture the critical path of transaction execution latency on each
// objects.
#[derive(Default, PartialEq, Eq, Clone, Debug)]
pub struct SharedObjectCongestionTracker {
    object_execution_cost: HashMap<ObjectID, u64>,
}

impl SharedObjectCongestionTracker {
    pub fn new_with_initial_value_for_test(init_values: &[(ObjectID, u64)]) -> Self {
        let mut object_execution_cost = HashMap::new();
        for (object_id, total_cost) in init_values {
            object_execution_cost.insert(*object_id, *total_cost);
        }
        Self {
            object_execution_cost,
        }
    }

    // Given a list of shared input objects, returns the starting cost of a transaction that operates on
    // these objects.
    //
    // Starting cost is a proxy for the starting time of the transaction. It is determined by all the input
    // shared objects' last write.
    pub fn compute_tx_start_at_cost(&self, shared_input_objects: &[SharedInputObject]) -> u64 {
        shared_input_objects
            .iter()
            .map(|obj| *self.object_execution_cost.get(&obj.id).unwrap_or(&0))
            .max()
            .expect("There must be at least one object in shared_input_objects.")
    }

    // Given a transaction, returns the deferral key and the congested objects if the transaction should be deferred.
    pub fn should_defer_due_to_object_congestion(
        &self,
        cert: &VerifiedExecutableTransaction,
        max_accumulated_txn_cost_per_object_in_checkpoint: u64,
        previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
        commit_round: Round,
    ) -> Option<(DeferralKey, Vec<ObjectID>)> {
        let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();
        let start_cost = self.compute_tx_start_at_cost(&shared_input_objects);
        if start_cost + cert.gas_budget() <= max_accumulated_txn_cost_per_object_in_checkpoint {
            return None;
        }

        // Finds out the congested objects.
        //
        // Note that the congested objects here may be caused by transaction dependency of other congested objects.
        // Consider in a consensus commit, there are many transactions touching object A, and later in processing the
        // consensus commit, there is a transaction touching both object A and B. Although there are fewer transactions
        // touching object B, becase it's starting execution is delayed due to dependency to other transactions on
        // object A, it may be shown up as congested objects.
        let mut congested_objects = vec![];
        for obj in shared_input_objects {
            // TODO: right now, we only return objects that are on the execution critical path in this consensus commit.
            // However, for objects that are no on the critical path, they may potentially also be congested (e.g., an
            // object has start cost == start_cost - 1, and adding the gas budget will exceed the limit). We don't
            // return them for now because it's unclear how they can be used to return suggested gas price for the
            // user. We need to revisit this later once we have a clear idea of how to determine the suggested gas price.
            if &start_cost == self.object_execution_cost.get(&obj.id).unwrap_or(&0) {
                congested_objects.push(obj.id);
            }
        }

        assert!(!congested_objects.is_empty());

        let deferral_key = if let Some(DeferralKey::ConsensusRound {
            future_round: _,
            deferred_from_round,
        }) = previously_deferred_tx_digests.get(cert.digest())
        {
            // This transaction has been deferred in previous consensus commit. Use its previous deferred_from_round.
            DeferralKey::new_for_consensus_round(commit_round + 1, *deferred_from_round)
        } else {
            // There are two cases where we can end up here:
            // 1. This transaction has not been deferred before.
            // 2. This transaction has been deferred due to randomness.
            // In both case, we use the current commit round as the deferred_from_round.
            DeferralKey::new_for_consensus_round(commit_round + 1, commit_round)
        };
        Some((deferral_key, congested_objects))
    }

    pub fn bump_object_execution_cost(
        &mut self,
        shared_input_objects: &[SharedInputObject],
        tx_cost: u64,
    ) {
        let start_cost = self.compute_tx_start_at_cost(shared_input_objects);
        let end_cost = start_cost + tx_cost;

        for obj in shared_input_objects {
            if obj.mutable {
                let old_end_cost = self.object_execution_cost.insert(obj.id, end_cost);
                assert!(old_end_cost.is_none() || old_end_cost.unwrap() < end_cost);
            }
        }
    }
}

#[cfg(test)]
mod object_cost_tests {
    use super::*;

    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::{random_object_ref, SequenceNumber};
    use sui_types::crypto::{get_key_pair, AccountKeyPair};
    use sui_types::transaction::{CallArg, ObjectArg, TransactionDataAPI, VerifiedTransaction};

    fn construct_shared_input_objects(objects: &[(ObjectID, bool)]) -> Vec<SharedInputObject> {
        objects
            .iter()
            .map(|(id, mutable)| SharedInputObject {
                id: *id,
                initial_shared_version: SequenceNumber::new(),
                mutable: *mutable,
            })
            .collect()
    }

    #[test]
    fn test_compute_tx_start_at_cost() {
        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let shared_object_congestion_tracker =
            SharedObjectCongestionTracker::new_with_initial_value_for_test(&[
                (object_id_0, 5),
                (object_id_1, 10),
            ]);

        let shared_input_objects = construct_shared_input_objects(&[(object_id_0, false)]);
        assert_eq!(
            shared_object_congestion_tracker.compute_tx_start_at_cost(&shared_input_objects),
            5
        );

        let shared_input_objects = construct_shared_input_objects(&[(object_id_1, true)]);
        assert_eq!(
            shared_object_congestion_tracker.compute_tx_start_at_cost(&shared_input_objects),
            10
        );

        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, false), (object_id_1, false)]);
        assert_eq!(
            shared_object_congestion_tracker.compute_tx_start_at_cost(&shared_input_objects),
            10
        );

        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, true), (object_id_1, true)]);
        assert_eq!(
            shared_object_congestion_tracker.compute_tx_start_at_cost(&shared_input_objects),
            10
        );

        // Test tx that touch object for the first time, which should start from 0.
        let shared_input_objects = construct_shared_input_objects(&[(object_id_2, true)]);
        assert_eq!(
            shared_object_congestion_tracker.compute_tx_start_at_cost(&shared_input_objects),
            0
        );
    }

    // Builds a certificate with a list of shared objects and their mutability. The certificate is only used to
    // test the should_defer_due_to_object_congestion function, therefore the content other than shared inputs
    // are not important.
    fn build_transaction(objects: &[(ObjectID, bool)]) -> VerifiedExecutableTransaction {
        let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
        let gas_object = random_object_ref();
        VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(
                TestTransactionBuilder::new(sender, gas_object, 1000)
                    .move_call(
                        ObjectID::random(),
                        "unimportant_module",
                        "unimportant_function",
                        objects
                            .iter()
                            .map(|(id, mutable)| {
                                CallArg::Object(ObjectArg::SharedObject {
                                    id: *id,
                                    initial_shared_version: SequenceNumber::new(),
                                    mutable: *mutable,
                                })
                            })
                            .collect(),
                    )
                    .build_and_sign(&keypair),
            ),
            0,
        )
    }

    #[test]
    fn test_should_defer_return_correct_congested_objects() {
        // Creates two shared objects and three transactions that operate on these objects.
        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        // Set max_accumulated_txn_cost_per_object_in_checkpoint to only allow 1 transaction to go through.
        let max_accumulated_txn_cost_per_object_in_checkpoint =
            build_transaction(&[(shared_obj_0, true)])
                .transaction_data()
                .gas_budget()
                + 1;

        // Construct object execution cost as following
        //                1     10
        // object 0:            |
        // object 1:      |
        let shared_object_congestion_tracker =
            SharedObjectCongestionTracker::new_with_initial_value_for_test(&[
                (shared_obj_0, 10),
                (shared_obj_1, 1),
            ]);

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)]);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &tx,
                    max_accumulated_txn_cost_per_object_in_checkpoint,
                    &HashMap::new(),
                    0,
                )
            {
                assert_eq!(congested_objects.len(), 1);
                assert_eq!(congested_objects[0], shared_obj_0);
            } else {
                panic!("should defer");
            }
        }

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_1, *mutable)]);
            assert!(shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &tx,
                    max_accumulated_txn_cost_per_object_in_checkpoint,
                    &HashMap::new(),
                    0,
                )
                .is_none());
        }

        // Transactions touching both objects should be deferred, with object 0 as the congested object.
        for mutable_0 in [true, false].iter() {
            for mutable_1 in [true, false].iter() {
                let tx =
                    build_transaction(&[(shared_obj_0, *mutable_0), (shared_obj_1, *mutable_1)]);
                if let Some((_, congested_objects)) = shared_object_congestion_tracker
                    .should_defer_due_to_object_congestion(
                        &tx,
                        max_accumulated_txn_cost_per_object_in_checkpoint,
                        &HashMap::new(),
                        0,
                    )
                {
                    assert_eq!(congested_objects.len(), 1);
                    assert_eq!(congested_objects[0], shared_obj_0);
                } else {
                    panic!("should defer");
                }
            }
        }
    }

    #[test]
    fn test_should_defer_return_correct_deferral_key() {
        let shared_obj_0 = ObjectID::random();
        let tx = build_transaction(&[(shared_obj_0, true)]);
        // Make should_defer_due_to_object_congestion always defer transactions.
        let max_accumulated_txn_cost_per_object_in_checkpoint = 1;
        let shared_object_congestion_tracker: SharedObjectCongestionTracker = Default::default();

        // Insert a random pre-existing transaction.
        let mut previously_deferred_tx_digests = HashMap::new();
        previously_deferred_tx_digests.insert(
            TransactionDigest::random(),
            DeferralKey::ConsensusRound {
                future_round: 10,
                deferred_from_round: 5,
            },
        );

        // Test deferral key for a transaction that has not been deferred before.
        if let Some((
            DeferralKey::ConsensusRound {
                future_round,
                deferred_from_round,
            },
            _,
        )) = shared_object_congestion_tracker.should_defer_due_to_object_congestion(
            &tx,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 10);
        } else {
            panic!("should defer");
        }

        // Insert `tx`` as previously deferred transaction due to randomness.
        previously_deferred_tx_digests.insert(
            *tx.digest(),
            DeferralKey::RandomnessDkg {
                deferred_from_round: 5,
            },
        );

        // New deferral key should have deferred_from_round equal to the current round.
        if let Some((
            DeferralKey::ConsensusRound {
                future_round,
                deferred_from_round,
            },
            _,
        )) = shared_object_congestion_tracker.should_defer_due_to_object_congestion(
            &tx,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 10);
        } else {
            panic!("should defer");
        }

        // Insert `tx`` as previously deferred consensus transaction.
        previously_deferred_tx_digests.insert(
            *tx.digest(),
            DeferralKey::ConsensusRound {
                future_round: 10,
                deferred_from_round: 5,
            },
        );

        // New deferral key should have deferred_from_round equal to the one in the old deferral key.
        if let Some((
            DeferralKey::ConsensusRound {
                future_round,
                deferred_from_round,
            },
            _,
        )) = shared_object_congestion_tracker.should_defer_due_to_object_congestion(
            &tx,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 5);
        } else {
            panic!("should defer");
        }
    }

    #[test]
    fn test_bump_object_execution_cost() {
        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let mut shared_object_congestion_tracker =
            SharedObjectCongestionTracker::new_with_initial_value_for_test(&[
                (object_id_0, 5),
                (object_id_1, 10),
            ]);

        // Read two objects should not change the object execution cost.
        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, false), (object_id_1, false)]);
        shared_object_congestion_tracker.bump_object_execution_cost(&shared_input_objects, 10);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new_with_initial_value_for_test(&[
                (object_id_0, 5),
                (object_id_1, 10)
            ])
        );

        // Write to object 0 should only bump object 0's execution cost. The start cost should be object 1's cost.
        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, true), (object_id_1, false)]);
        shared_object_congestion_tracker.bump_object_execution_cost(&shared_input_objects, 10);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new_with_initial_value_for_test(&[
                (object_id_0, 20),
                (object_id_1, 10)
            ])
        );

        // Write to all objects should bump all objects' execution cost, including objects that are seen for the first time.
        let shared_input_objects = construct_shared_input_objects(&[
            (object_id_0, true),
            (object_id_1, true),
            (object_id_2, true),
        ]);
        shared_object_congestion_tracker.bump_object_execution_cost(&shared_input_objects, 10);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new_with_initial_value_for_test(&[
                (object_id_0, 30),
                (object_id_1, 30),
                (object_id_2, 30)
            ])
        );
    }
}
