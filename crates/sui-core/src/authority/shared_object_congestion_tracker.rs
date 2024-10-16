// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::transaction_deferral::DeferralKey;
use narwhal_types::Round;
use std::collections::HashMap;
use sui_protocol_config::PerObjectCongestionControlMode;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::{Argument, SharedInputObject, TransactionDataAPI};

// SharedObjectCongestionTracker stores the accumulated cost of executing transactions on an object, for
// all transactions in a consensus commit.
//
// Cost is an indication of transaction execution latency. When transactions are scheduled by
// the consensus handler, each scheduled transaction adds cost (execution latency) to all the objects it
// reads or writes.
//
// The goal of this data structure is to capture the critical path of transaction execution latency on each
// objects.
//
// The mode field determines how the cost is calculated. The cost can be calculated based on the total gas
// budget, or total number of transaction count.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct SharedObjectCongestionTracker {
    object_execution_cost: HashMap<ObjectID, u64>,
    mode: PerObjectCongestionControlMode,
    gas_budget_based_txn_cost_cap_factor: Option<u64>,
}

impl SharedObjectCongestionTracker {
    pub fn new(
        mode: PerObjectCongestionControlMode,
        gas_budget_based_txn_cost_cap_factor: Option<u64>,
    ) -> Self {
        Self {
            object_execution_cost: HashMap::new(),
            mode,
            gas_budget_based_txn_cost_cap_factor,
        }
    }

    pub fn new_with_initial_value_for_test(
        init_values: &[(ObjectID, u64)],
        mode: PerObjectCongestionControlMode,
        gas_budget_based_txn_cost_cap_factor: Option<u64>,
    ) -> Self {
        let mut object_execution_cost = HashMap::new();
        for (object_id, total_cost) in init_values {
            object_execution_cost.insert(*object_id, *total_cost);
        }
        Self {
            object_execution_cost,
            mode,
            gas_budget_based_txn_cost_cap_factor,
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

    pub fn get_tx_cost(&self, cert: &VerifiedExecutableTransaction) -> Option<u64> {
        match self.mode {
            PerObjectCongestionControlMode::None => None,
            PerObjectCongestionControlMode::TotalGasBudget => Some(cert.gas_budget()),
            PerObjectCongestionControlMode::TotalTxCount => Some(1),
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                Some(std::cmp::min(cert.gas_budget(), self.get_tx_cost_cap(cert)))
            }
        }
    }

    // Given a transaction, returns the deferral key and the congested objects if the transaction should be deferred.
    pub fn should_defer_due_to_object_congestion(
        &self,
        cert: &VerifiedExecutableTransaction,
        max_accumulated_txn_cost_per_object_in_commit: u64,
        previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
        commit_round: Round,
    ) -> Option<(DeferralKey, Vec<ObjectID>)> {
        let tx_cost = self.get_tx_cost(cert)?;

        let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();
        if shared_input_objects.is_empty() {
            // This is an owned object only transaction. No need to defer.
            return None;
        }
        let start_cost = self.compute_tx_start_at_cost(&shared_input_objects);

        if start_cost + tx_cost <= max_accumulated_txn_cost_per_object_in_commit {
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

        let deferral_key =
            if let Some(previous_key) = previously_deferred_tx_digests.get(cert.digest()) {
                // This transaction has been deferred in previous consensus commit. Use its previous deferred_from_round.
                DeferralKey::new_for_consensus_round(
                    commit_round + 1,
                    previous_key.deferred_from_round(),
                )
            } else {
                // This transaction has not been deferred before. Use the current commit round
                // as the deferred_from_round.
                DeferralKey::new_for_consensus_round(commit_round + 1, commit_round)
            };
        Some((deferral_key, congested_objects))
    }

    // Update shared objects' execution cost used in `cert` using `cert`'s execution cost.
    // This is called when `cert` is scheduled for execution.
    pub fn bump_object_execution_cost(&mut self, cert: &VerifiedExecutableTransaction) {
        let Some(tx_cost) = self.get_tx_cost(cert) else {
            return;
        };

        let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();
        let start_cost = self.compute_tx_start_at_cost(&shared_input_objects);
        let end_cost = start_cost + tx_cost;

        for obj in shared_input_objects {
            if obj.mutable {
                let old_end_cost = self.object_execution_cost.insert(obj.id, end_cost);
                assert!(old_end_cost.is_none() || old_end_cost.unwrap() < end_cost);
            }
        }
    }

    // Returns the maximum cost of all objects.
    pub fn max_cost(&self) -> u64 {
        self.object_execution_cost
            .values()
            .max()
            .copied()
            .unwrap_or(0)
    }

    fn get_tx_cost_cap(&self, cert: &VerifiedExecutableTransaction) -> u64 {
        let mut number_of_move_call = 0;
        let mut number_of_move_input = 0;
        for command in cert.transaction_data().kind().iter_commands() {
            if let sui_types::transaction::Command::MoveCall(move_call) = command {
                number_of_move_call += 1;
                for aug in move_call.arguments.iter() {
                    if let Argument::Input(_) = aug {
                        number_of_move_input += 1;
                    }
                }
            }
        }
        (number_of_move_call + number_of_move_input) as u64
            * self
                .gas_budget_based_txn_cost_cap_factor
                .expect("cap factor must be set if TotalGasBudgetWithCap mode is used.")
    }
}

#[cfg(test)]
mod object_cost_tests {
    use super::*;

    use rstest::rstest;
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::{random_object_ref, SequenceNumber};
    use sui_types::crypto::{get_key_pair, AccountKeyPair};
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{CallArg, ObjectArg, VerifiedTransaction};
    use sui_types::Identifier;

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
            SharedObjectCongestionTracker::new_with_initial_value_for_test(
                &[(object_id_0, 5), (object_id_1, 10)],
                PerObjectCongestionControlMode::TotalGasBudget,
                None,
            );

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
    // test the SharedObjectCongestionTracker functions, therefore the content other than shared inputs and gas budget
    // are not important.
    fn build_transaction(
        objects: &[(ObjectID, bool)],
        gas_budget: u64,
    ) -> VerifiedExecutableTransaction {
        let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
        let gas_object = random_object_ref();
        VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(
                TestTransactionBuilder::new(sender, gas_object, 1000)
                    .with_gas_budget(gas_budget)
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

    fn build_programmable_transaction(
        objects: &[(ObjectID, bool)],
        number_of_commands: u64,
        gas_budget: u64,
    ) -> VerifiedExecutableTransaction {
        let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
        let gas_object = random_object_ref();

        let package_id = ObjectID::random();
        let mut pt_builder = ProgrammableTransactionBuilder::new();
        let mut arguments = Vec::new();
        for object in objects {
            arguments.push(
                pt_builder
                    .obj(ObjectArg::SharedObject {
                        id: object.0,
                        initial_shared_version: SequenceNumber::new(),
                        mutable: object.1,
                    })
                    .unwrap(),
            );
        }
        for _ in 0..number_of_commands {
            pt_builder.programmable_move_call(
                package_id,
                Identifier::new("unimportant_module").unwrap(),
                Identifier::new("unimportant_function").unwrap(),
                vec![],
                arguments.clone(),
            );
        }

        let pt = pt_builder.finish();
        VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(
                TestTransactionBuilder::new(sender, gas_object, 1000)
                    .with_gas_budget(gas_budget)
                    .programmable(pt)
                    .build_and_sign(&keypair),
            ),
            0,
        )
    }

    #[rstest]
    fn test_should_defer_return_correct_congested_objects(
        #[values(
            PerObjectCongestionControlMode::TotalGasBudget,
            PerObjectCongestionControlMode::TotalTxCount,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        // Creates two shared objects and three transactions that operate on these objects.
        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let tx_gas_budget = 100;

        // Set max_accumulated_txn_cost_per_object_in_commit to only allow 1 transaction to go through.
        let max_accumulated_txn_cost_per_object_in_commit = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => tx_gas_budget + 1,
            PerObjectCongestionControlMode::TotalTxCount => 2,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => tx_gas_budget - 1,
        };

        let shared_object_congestion_tracker = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => {
                // Construct object execution cost as following
                //                1     10
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new_with_initial_value_for_test(
                    &[(shared_obj_0, 10), (shared_obj_1, 1)],
                    mode,
                    None,
                )
            }
            PerObjectCongestionControlMode::TotalTxCount => {
                // Construct object execution cost as following
                //                1     2
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new_with_initial_value_for_test(
                    &[(shared_obj_0, 2), (shared_obj_1, 1)],
                    mode,
                    None,
                )
            }
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                // Construct object execution cost as following
                //                1     10
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new_with_initial_value_for_test(
                    &[(shared_obj_0, 10), (shared_obj_1, 1)],
                    mode,
                    Some(45), // Make the cap just less than the gas budget, there are 1 objects in tx.
                )
            }
        };

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &tx,
                    max_accumulated_txn_cost_per_object_in_commit,
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

        // Read/write to object 1 should go through.
        // When congestion control mode is TotalGasBudgetWithCap, even though the gas budget is over the limit,
        // the cap should prevent the transaction from being deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_1, *mutable)], tx_gas_budget);
            assert!(shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &tx,
                    max_accumulated_txn_cost_per_object_in_commit,
                    &HashMap::new(),
                    0,
                )
                .is_none());
        }

        // Transactions touching both objects should be deferred, with object 0 as the congested object.
        for mutable_0 in [true, false].iter() {
            for mutable_1 in [true, false].iter() {
                let tx = build_transaction(
                    &[(shared_obj_0, *mutable_0), (shared_obj_1, *mutable_1)],
                    tx_gas_budget,
                );
                if let Some((_, congested_objects)) = shared_object_congestion_tracker
                    .should_defer_due_to_object_congestion(
                        &tx,
                        max_accumulated_txn_cost_per_object_in_commit,
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

    #[rstest]
    fn test_should_defer_return_correct_deferral_key(
        #[values(
            PerObjectCongestionControlMode::TotalGasBudget,
            PerObjectCongestionControlMode::TotalTxCount,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        let shared_obj_0 = ObjectID::random();
        let tx = build_transaction(&[(shared_obj_0, true)], 100);
        // Make should_defer_due_to_object_congestion always defer transactions.
        let max_accumulated_txn_cost_per_object_in_commit = 0;
        let shared_object_congestion_tracker = SharedObjectCongestionTracker::new(mode, Some(2));

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
            max_accumulated_txn_cost_per_object_in_commit,
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
            DeferralKey::Randomness {
                deferred_from_round: 4,
            },
        );

        // New deferral key should have deferred_from_round equal to the deferred randomness round.
        if let Some((
            DeferralKey::ConsensusRound {
                future_round,
                deferred_from_round,
            },
            _,
        )) = shared_object_congestion_tracker.should_defer_due_to_object_congestion(
            &tx,
            max_accumulated_txn_cost_per_object_in_commit,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 4);
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
            max_accumulated_txn_cost_per_object_in_commit,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 5);
        } else {
            panic!("should defer");
        }
    }

    #[rstest]
    fn test_bump_object_execution_cost(
        #[values(
            PerObjectCongestionControlMode::TotalGasBudget,
            PerObjectCongestionControlMode::TotalTxCount,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let cap_factor = Some(1);

        let mut shared_object_congestion_tracker =
            SharedObjectCongestionTracker::new_with_initial_value_for_test(
                &[(object_id_0, 5), (object_id_1, 10)],
                mode,
                cap_factor,
            );
        assert_eq!(shared_object_congestion_tracker.max_cost(), 10);

        // Read two objects should not change the object execution cost.
        let cert = build_transaction(&[(object_id_0, false), (object_id_1, false)], 10);
        shared_object_congestion_tracker.bump_object_execution_cost(&cert);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new_with_initial_value_for_test(
                &[(object_id_0, 5), (object_id_1, 10)],
                mode,
                cap_factor,
            )
        );
        assert_eq!(shared_object_congestion_tracker.max_cost(), 10);

        // Write to object 0 should only bump object 0's execution cost. The start cost should be object 1's cost.
        let cert = build_transaction(&[(object_id_0, true), (object_id_1, false)], 10);
        shared_object_congestion_tracker.bump_object_execution_cost(&cert);
        let expected_object_0_cost = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => 20,
            PerObjectCongestionControlMode::TotalTxCount => 11,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => 13, // 2 objects, 1 command.
        };
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new_with_initial_value_for_test(
                &[(object_id_0, expected_object_0_cost), (object_id_1, 10)],
                mode,
                cap_factor,
            )
        );
        assert_eq!(
            shared_object_congestion_tracker.max_cost(),
            expected_object_0_cost
        );

        // Write to all objects should bump all objects' execution cost, including objects that are seen for the first time.
        let cert = build_transaction(
            &[
                (object_id_0, true),
                (object_id_1, true),
                (object_id_2, true),
            ],
            10,
        );
        let expected_object_cost = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => 30,
            PerObjectCongestionControlMode::TotalTxCount => 12,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => 17, // 3 objects, 1 command
        };
        shared_object_congestion_tracker.bump_object_execution_cost(&cert);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new_with_initial_value_for_test(
                &[
                    (object_id_0, expected_object_cost),
                    (object_id_1, expected_object_cost),
                    (object_id_2, expected_object_cost)
                ],
                mode,
                cap_factor,
            )
        );
        assert_eq!(
            shared_object_congestion_tracker.max_cost(),
            expected_object_cost
        );

        // Write to all objects with PTBs containing 7 commands.
        let cert = build_programmable_transaction(
            &[
                (object_id_0, true),
                (object_id_1, true),
                (object_id_2, true),
            ],
            7,
            30,
        );
        let expected_object_cost = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => 60,
            PerObjectCongestionControlMode::TotalTxCount => 13,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => 45, // 3 objects, 7 commands
        };
        shared_object_congestion_tracker.bump_object_execution_cost(&cert);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new_with_initial_value_for_test(
                &[
                    (object_id_0, expected_object_cost),
                    (object_id_1, expected_object_cost),
                    (object_id_2, expected_object_cost)
                ],
                mode,
                cap_factor,
            )
        );
        assert_eq!(
            shared_object_congestion_tracker.max_cost(),
            expected_object_cost
        );
    }
}
