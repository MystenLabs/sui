// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::DeferralKey;
use narwhal_types::Round;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::SharedInputObject;

/// ObjectExecutionQueueMeasure stores the accumulated cost of executing transactions on an object, for
/// all transactions in a consensus commit.
///
/// Cost is an indication of transaction execution latency. When transactions are scheduled by
/// the consensus handler, each scheduled transaction adds cost (execution latency) to all the objects it
/// reads or writes.
///
/// The goal of this data structure is to capture the critical path of transaction execution latency on each
/// objects.
///
/// Note that we capture an object's total cost as well as the cost to the last write. This is because
/// for transactions that read an object, then can go in parallel. So their the execution path can all
/// all start from the object's last write.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectExecutionQueueMeasure {
    // Accumulated cost to last write on this object.
    pub total_cost_to_last_write: u64,

    // Accumulated cost to the last access on this object.
    pub total_cost: u64,
}

impl ObjectExecutionQueueMeasure {
    // Write operation bumps the object cost to `end_cost`.
    pub fn write_bump_cost(&mut self, end_cost: u64) {
        assert!(end_cost > self.total_cost_to_last_write);
        assert!(end_cost > self.total_cost);
        self.total_cost_to_last_write = end_cost;
        self.total_cost = end_cost;
    }

    // Read operation bumps the object cost to `end_cost`.
    pub fn read_bump_cost(&mut self, end_cost: u64) {
        assert!(end_cost >= self.total_cost_to_last_write);
        self.total_cost = core::cmp::max(end_cost, self.total_cost);
    }
}

// Given a list of shared input objects, returns the starting cost of a transaction that operates on
// these objects.
// Starting cost is a proxy for the starting time of the transaction. It is determined by the object
// with the highest current cost.
pub fn compute_tx_start_at_cost(
    object_execution_cost: &HashMap<ObjectID, ObjectExecutionQueueMeasure>,
    shared_input_objects: Vec<SharedInputObject>,
) -> u64 {
    let mut start_cost = 0;
    let default_object_cost = ObjectExecutionQueueMeasure::default();
    shared_input_objects.iter().for_each(|obj| {
        let object_cost = object_execution_cost
            .get(&obj.id)
            .unwrap_or(&default_object_cost);
        if obj.mutable {
            // For write, start cost must after all previous txs finish.
            start_cost = core::cmp::max(start_cost, object_cost.total_cost);
        } else {
            // For read, start cost only need to after previous write.
            start_cost = core::cmp::max(start_cost, object_cost.total_cost_to_last_write);
        }
    });
    start_cost
}

// Given a transaction, returns the deferral key and the congested objects if the transaction should be deferred.
pub fn should_defer_due_to_object_congestion(
    object_execution_cost: &HashMap<ObjectID, ObjectExecutionQueueMeasure>,
    cert: &VerifiedExecutableTransaction,
    max_accumulated_txn_cost_per_object_in_checkpoint: u64,
    previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
    commit_round: Round,
) -> Option<(DeferralKey, Vec<ObjectID>)> {
    let start_cost =
        compute_tx_start_at_cost(object_execution_cost, cert.shared_input_objects().collect());
    if start_cost + cert.gas_budget() <= max_accumulated_txn_cost_per_object_in_checkpoint {
        return None;
    }

    // Finds out the congested objects.
    // Note that the congested objects here may be caused by transaction dependency of other congested objects.
    // Consider in a consensus commit, there are many transactions touch object A, and later in processing the
    // consensus commit, there is a transaction touch both object A and B. Although there are fewer transactions
    // touch object B, becase it's starting execution is delayed due to dependency to other transactions on
    // object A, it may be shown up as congested objects.
    let mut congested_objects = vec![];
    for obj in cert.shared_input_objects() {
        if obj.mutable {
            if start_cost
                == object_execution_cost
                    .get(&obj.id)
                    .map_or(0, |cost| cost.total_cost)
            {
                congested_objects.push(obj.id);
            }
        } else if start_cost
            == object_execution_cost
                .get(&obj.id)
                .map_or(0, |cost| cost.total_cost_to_last_write)
        {
            congested_objects.push(obj.id);
        }
    }

    assert!(!congested_objects.is_empty());

    let deferral_key = if let Some(DeferralKey::ConsensusRound {
        future_round: _,
        deferred_from_round,
    }) = previously_deferred_tx_digests.get(cert.digest())
    {
        // This transaction has been deferred before. Use its previous deferred_from_round.
        DeferralKey::new_for_consensus_round(commit_round + 1, *deferred_from_round)
    } else {
        // This include previously deferred randomness transactions.
        DeferralKey::new_for_consensus_round(commit_round + 1, commit_round)
    };
    Some((deferral_key, congested_objects))
}

#[cfg(test)]
mod object_cost_tests {
    use super::*;

    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::{random_object_ref, SequenceNumber};
    use sui_types::crypto::{get_key_pair, AccountKeyPair};
    use sui_types::transaction::{CallArg, ObjectArg, TransactionDataAPI, VerifiedTransaction};

    fn init_object_execution_cost(
        init_values: &[(ObjectID, u64, u64)],
    ) -> HashMap<ObjectID, ObjectExecutionQueueMeasure> {
        let mut object_execution_cost = HashMap::new();
        for (object_id, total_cost_to_last_write, total_cost) in init_values {
            object_execution_cost.insert(
                *object_id,
                ObjectExecutionQueueMeasure {
                    total_cost_to_last_write: *total_cost_to_last_write,
                    total_cost: *total_cost,
                },
            );
        }
        object_execution_cost
    }

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

        // Construct execution cost as following
        //                5    10   15    20
        // object 0:          w|         r|
        // object 1:     w|        r|
        let object_execution_cost =
            init_object_execution_cost(&[(object_id_0, 10, 20), (object_id_1, 5, 15)]);

        // Test read object tx starts from last write.
        //                5    10   15    20
        // object 0:          w|         r|
        // object 1:     w|        r|
        // tx:                 |<-read 0->|
        let shared_input_objects = construct_shared_input_objects(&[(object_id_0, false)]);
        assert_eq!(
            compute_tx_start_at_cost(&object_execution_cost, shared_input_objects),
            10
        );

        // Test write object tx starts from last access.
        //                5    10   15    20
        // object 0:          w|         r|
        // object 1:     w|        r|
        // tx:                            |<-write 0->|
        let shared_input_objects = construct_shared_input_objects(&[(object_id_0, true)]);
        assert_eq!(
            compute_tx_start_at_cost(&object_execution_cost, shared_input_objects),
            20
        );

        // Test read two objects.
        //                5    10   15    20
        // object 0:          w|         r|
        // object 1:     w|        r|
        // tx:                 |<-r 0, r 1->|
        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, false), (object_id_1, false)]);
        assert_eq!(
            compute_tx_start_at_cost(&object_execution_cost, shared_input_objects),
            10
        );

        // Test read object 0, write object 1.
        //                5    10   15    20
        // object 0:          w|         r|
        // object 1:     w|        r|
        // tx:                      |<-r 0, w 1->|
        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, false), (object_id_1, true)]);
        assert_eq!(
            compute_tx_start_at_cost(&object_execution_cost, shared_input_objects),
            15
        );

        // Test write object 0, read object 1.
        //                5    10   15    20
        // object 0:          w|         r|
        // object 1:     w|        r|
        // tx:                            |<-w 0, r 1->|
        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, true), (object_id_1, false)]);
        assert_eq!(
            compute_tx_start_at_cost(&object_execution_cost, shared_input_objects),
            20
        );

        // Test write two objects.
        //                5    10   15    20
        // object 0:          w|         r|
        // object 1:     w|        r|
        // tx:                            |<-w 0, w 1->|
        let shared_input_objects =
            construct_shared_input_objects(&[(object_id_0, true), (object_id_1, true)]);
        assert_eq!(
            compute_tx_start_at_cost(&object_execution_cost, shared_input_objects),
            20
        );

        // Test tx that touch object for the first time, which should start from 0.
        let shared_input_objects = construct_shared_input_objects(&[(object_id_2, true)]);
        assert_eq!(
            compute_tx_start_at_cost(&object_execution_cost, shared_input_objects),
            0
        );
    }

    #[test]
    fn test_update_read_write_cost() {
        let mut object_cost = ObjectExecutionQueueMeasure {
            total_cost_to_last_write: 0,
            total_cost: 0,
        };

        // Write bump cost to 10.
        //                  10       20       30
        // object:        wr|
        object_cost.write_bump_cost(10);
        assert_eq!(object_cost.total_cost_to_last_write, 10);
        assert_eq!(object_cost.total_cost, 10);

        // Read bump cost to 20.
        //                  10       20       30
        // object:         w|       r|
        object_cost.read_bump_cost(20);
        assert_eq!(object_cost.total_cost_to_last_write, 10);
        assert_eq!(object_cost.total_cost, 20);

        // Read bump cost to 15, no change to total cost.
        //                  10       20       30
        // object:         w|       r|
        object_cost.read_bump_cost(15);
        assert_eq!(object_cost.total_cost, 20);

        // Write bump cost to 30.
        //                  10       20       30
        // object:                          wr|
        object_cost.write_bump_cost(30);
        assert_eq!(object_cost.total_cost_to_last_write, 30);
        assert_eq!(object_cost.total_cost, 30);
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
        let tx_write_0 = build_transaction(&[(shared_obj_0, true)]);
        let tx_read_0_read_1 = build_transaction(&[(shared_obj_0, false), (shared_obj_1, false)]);
        let tx_read_0_write_1 = build_transaction(&[(shared_obj_0, false), (shared_obj_1, true)]);

        // Set max_accumulated_txn_cost_per_object_in_checkpoint to only allow 1 transaction to go through.
        let max_accumulated_txn_cost_per_object_in_checkpoint =
            tx_write_0.transaction_data().gas_budget() + 1;

        // Construct object execution cost as following
        //                1     10
        // object 0:     w|    r|
        // object 1:     w|    r|
        let object_execution_cost =
            init_object_execution_cost(&[(shared_obj_0, 1, 10), (shared_obj_1, 1, 10)]);

        // Write to object 0 should be deferred.
        if let Some((_, congested_objects)) = should_defer_due_to_object_congestion(
            &object_execution_cost,
            &tx_write_0,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        ) {
            assert_eq!(congested_objects.len(), 1);
            assert_eq!(congested_objects[0], shared_obj_0);
        } else {
            panic!("should defer");
        }

        // Read to both objects can go through.
        assert!(should_defer_due_to_object_congestion(
            &object_execution_cost,
            &tx_read_0_read_1,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        )
        .is_none());

        // Read to object 0 and Write to object 1 should be deferred due to congestion on object 1.
        if let Some((_, congested_objects)) = should_defer_due_to_object_congestion(
            &object_execution_cost,
            &tx_read_0_write_1,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        ) {
            assert_eq!(congested_objects.len(), 1);
            assert_eq!(congested_objects[0], shared_obj_1);
        } else {
            panic!("should defer");
        }

        // Construct object execution cost as following
        //                1     10
        // object 0:          wr|
        // object 1:     w|    r|
        let object_execution_cost =
            init_object_execution_cost(&[(shared_obj_0, 10, 10), (shared_obj_1, 1, 10)]);

        // Read to both objects should be deferred due to congestion on object 0.
        if let Some((_, congested_objects)) = should_defer_due_to_object_congestion(
            &object_execution_cost,
            &tx_read_0_read_1,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        ) {
            assert_eq!(congested_objects.len(), 1);
            assert_eq!(congested_objects[0], shared_obj_0);
        } else {
            panic!("should defer");
        }
    }

    #[test]
    fn test_should_defer_return_correct_deferral_key() {
        let shared_obj_0 = ObjectID::random();
        let tx = build_transaction(&[(shared_obj_0, true)]);
        // Make should_defer_due_to_object_congestion always defer transactions.
        let max_accumulated_txn_cost_per_object_in_checkpoint = 1;

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
        )) = should_defer_due_to_object_congestion(
            &HashMap::new(),
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
        )) = should_defer_due_to_object_congestion(
            &HashMap::new(),
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
        )) = should_defer_due_to_object_congestion(
            &HashMap::new(),
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
}
