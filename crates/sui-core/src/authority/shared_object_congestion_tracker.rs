// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::execution_time_estimator::ExecutionTimeEstimator;
use crate::authority::transaction_deferral::DeferralKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sui_protocol_config::{PerObjectCongestionControlMode, ProtocolConfig};
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::error::SuiResult;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::Round;
use sui_types::transaction::{Argument, SharedInputObject, TransactionDataAPI};
use tracing::trace;

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
    max_accumulated_txn_cost_per_object_in_commit: u64,
    gas_budget_based_txn_cost_cap_factor: Option<u64>,
    gas_budget_based_txn_cost_absolute_cap: Option<u64>,
    max_txn_cost_overage_per_object_in_commit: u64,
    allowed_txn_cost_overage_burst_per_object_in_commit: u64,
}

impl SharedObjectCongestionTracker {
    pub fn new(
        initial_object_debts: impl IntoIterator<Item = (ObjectID, u64)>,
        mode: PerObjectCongestionControlMode,
        max_accumulated_txn_cost_per_object_in_commit: Option<u64>,
        gas_budget_based_txn_cost_cap_factor: Option<u64>,
        gas_budget_based_txn_cost_absolute_cap_commit_count: Option<u64>,
        max_txn_cost_overage_per_object_in_commit: u64,
        allowed_txn_cost_overage_burst_per_object_in_commit: u64,
    ) -> Self {
        assert!(
            allowed_txn_cost_overage_burst_per_object_in_commit <= max_txn_cost_overage_per_object_in_commit,
            "burst limit bust be <= absolute limit; allowed_txn_cost_overage_burst_per_object_in_commit = {allowed_txn_cost_overage_burst_per_object_in_commit}, max_txn_cost_overage_per_object_in_commit = {max_txn_cost_overage_per_object_in_commit}"
        );

        let object_execution_cost: HashMap<ObjectID, u64> =
            initial_object_debts.into_iter().collect();
        let max_accumulated_txn_cost_per_object_in_commit =
            if mode == PerObjectCongestionControlMode::None {
                0
            } else {
                max_accumulated_txn_cost_per_object_in_commit.expect(
                    "max_accumulated_txn_cost_per_object_in_commit must be set if mode is not None",
                )
            };
        let gas_budget_based_txn_cost_absolute_cap =
            gas_budget_based_txn_cost_absolute_cap_commit_count
                .map(|m| m * max_accumulated_txn_cost_per_object_in_commit);
        trace!(
            "created SharedObjectCongestionTracker with
             {} initial object debts,
             mode: {mode:?}, 
             max_accumulated_txn_cost_per_object_in_commit: {max_accumulated_txn_cost_per_object_in_commit:?}, 
             gas_budget_based_txn_cost_cap_factor: {gas_budget_based_txn_cost_cap_factor:?}, 
             gas_budget_based_txn_cost_absolute_cap: {gas_budget_based_txn_cost_absolute_cap:?}, 
             max_txn_cost_overage_per_object_in_commit: {max_txn_cost_overage_per_object_in_commit:?}",
            object_execution_cost.len(),
        );
        Self {
            object_execution_cost,
            mode,
            max_accumulated_txn_cost_per_object_in_commit,
            gas_budget_based_txn_cost_cap_factor,
            gas_budget_based_txn_cost_absolute_cap,
            max_txn_cost_overage_per_object_in_commit,
            allowed_txn_cost_overage_burst_per_object_in_commit,
        }
    }

    pub fn from_protocol_config(
        initial_object_debts: impl IntoIterator<Item = (ObjectID, u64)>,
        protocol_config: &ProtocolConfig,
        for_randomness: bool,
    ) -> SuiResult<Self> {
        let max_accumulated_txn_cost_per_object_in_commit =
            protocol_config.max_accumulated_txn_cost_per_object_in_mysticeti_commit_as_option();
        Ok(Self::new(
            initial_object_debts,
            protocol_config.per_object_congestion_control_mode(),
            if for_randomness {
                protocol_config
                    .max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit_as_option()
                    .or(max_accumulated_txn_cost_per_object_in_commit)
            } else {
                max_accumulated_txn_cost_per_object_in_commit
            },
            protocol_config.gas_budget_based_txn_cost_cap_factor_as_option(),
            protocol_config.gas_budget_based_txn_cost_absolute_cap_commit_count_as_option(),
            protocol_config
                .max_txn_cost_overage_per_object_in_commit_as_option()
                .unwrap_or(0),
            protocol_config
                .allowed_txn_cost_overage_burst_per_object_in_commit_as_option()
                .unwrap_or(0),
        ))
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

    pub fn get_tx_cost(
        &self,
        execution_time_estimator: &ExecutionTimeEstimator,
        cert: &VerifiedExecutableTransaction,
    ) -> Option<u64> {
        match self.mode {
            PerObjectCongestionControlMode::None => None,
            PerObjectCongestionControlMode::TotalGasBudget => Some(cert.gas_budget()),
            PerObjectCongestionControlMode::TotalTxCount => Some(1),
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                Some(std::cmp::min(cert.gas_budget(), self.get_tx_cost_cap(cert)))
            }
            PerObjectCongestionControlMode::ExecutionTimeEstimate => Some(
                execution_time_estimator
                    .get_estimate(cert.transaction_data())
                    .as_micros()
                    .try_into()
                    .unwrap_or(u64::MAX),
            ),
        }
    }

    // Given a transaction, returns the deferral key and the congested objects if the transaction should be deferred.
    pub fn should_defer_due_to_object_congestion(
        &self,
        execution_time_estimator: &ExecutionTimeEstimator,
        cert: &VerifiedExecutableTransaction,
        previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
        commit_round: Round,
    ) -> Option<(DeferralKey, Vec<ObjectID>)> {
        let tx_cost = self.get_tx_cost(execution_time_estimator, cert)?;

        let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();
        if shared_input_objects.is_empty() {
            // This is an owned object only transaction. No need to defer.
            return None;
        }
        let start_cost = self.compute_tx_start_at_cost(&shared_input_objects);
        let end_cost = start_cost.saturating_add(tx_cost);

        // Allow tx if it's within configured limits.
        let burst_limit = self
            .max_accumulated_txn_cost_per_object_in_commit
            .saturating_add(self.allowed_txn_cost_overage_burst_per_object_in_commit);
        let absolute_limit = self
            .max_accumulated_txn_cost_per_object_in_commit
            .saturating_add(self.max_txn_cost_overage_per_object_in_commit);
        if start_cost <= burst_limit && end_cost <= absolute_limit {
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
    pub fn bump_object_execution_cost(
        &mut self,
        execution_time_estimator: &ExecutionTimeEstimator,
        cert: &VerifiedExecutableTransaction,
    ) {
        let Some(tx_cost) = self.get_tx_cost(execution_time_estimator, cert) else {
            return;
        };

        let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();
        let start_cost = self.compute_tx_start_at_cost(&shared_input_objects);
        let end_cost = start_cost.saturating_add(tx_cost);

        for obj in shared_input_objects {
            if obj.mutable {
                let old_end_cost = self.object_execution_cost.insert(obj.id, end_cost);
                assert!(old_end_cost.is_none() || old_end_cost.unwrap() <= end_cost);
            }
        }
    }

    // Returns accumulated debts for objects whose budgets have been exceeded over the course
    // of the commit. Consumes the tracker object, since this should only be called once after
    // all tx have been processed.
    pub fn accumulated_debts(self) -> Vec<(ObjectID, u64)> {
        if self.max_txn_cost_overage_per_object_in_commit == 0 {
            return vec![]; // early-exit if overage is not allowed
        }

        self.object_execution_cost
            .into_iter()
            .filter_map(|(obj_id, cost)| {
                let remaining_cost =
                    cost.saturating_sub(self.max_accumulated_txn_cost_per_object_in_commit);
                if remaining_cost > 0 {
                    Some((obj_id, remaining_cost))
                } else {
                    None
                }
            })
            .collect()
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
        let cap = (number_of_move_call + number_of_move_input) as u64
            * self
                .gas_budget_based_txn_cost_cap_factor
                .expect("cap factor must be set if TotalGasBudgetWithCap mode is used.");

        // Apply absolute cap if configured.
        std::cmp::min(
            cap,
            self.gas_budget_based_txn_cost_absolute_cap
                .unwrap_or(u64::MAX),
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CongestionPerObjectDebt {
    V1(Round, u64),
}

impl CongestionPerObjectDebt {
    pub fn new(round: Round, debt: u64) -> Self {
        Self::V1(round, debt)
    }

    pub fn into_v1(self) -> (Round, u64) {
        match self {
            Self::V1(round, debt) => (round, debt),
        }
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

        let shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(object_id_0, 5), (object_id_1, 10)],
            PerObjectCongestionControlMode::TotalGasBudget,
            Some(0), // not part of this test
            None,
            None,
            0,
            0,
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
            PerObjectCongestionControlMode::TotalGasBudgetWithCap,
            PerObjectCongestionControlMode::ExecutionTimeEstimate
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

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
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 2_000_000,
        };

        let shared_object_congestion_tracker = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => {
                // Construct object execution cost as following
                //                1     10
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 10), (shared_obj_1, 1)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    0,
                    0,
                )
            }
            PerObjectCongestionControlMode::TotalTxCount => {
                // Construct object execution cost as following
                //                1     2
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 2), (shared_obj_1, 1)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    0,
                    0,
                )
            }
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                // Construct object execution cost as following
                //                1     10
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 10), (shared_obj_1, 1)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    Some(45), // Make the cap just less than the gas budget, there are 1 objects in tx.
                    None,
                    0,
                    0,
                )
            }
            PerObjectCongestionControlMode::ExecutionTimeEstimate => {
                // Construct object execution cost as following
                //                0     1_000_000
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 1_000_000), (shared_obj_1, 0)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    0,
                    0,
                )
            }
        };

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &execution_time_estimator,
                    &tx,
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
                    &execution_time_estimator,
                    &tx,
                    &HashMap::new(),
                    0
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
                        &execution_time_estimator,
                        &tx,
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
            PerObjectCongestionControlMode::TotalGasBudgetWithCap,
            PerObjectCongestionControlMode::ExecutionTimeEstimate
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        let shared_obj_0 = ObjectID::random();
        let tx = build_transaction(&[(shared_obj_0, true)], 100);

        let shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [],
            mode,
            Some(0), // Make should_defer_due_to_object_congestion always defer transactions.
            Some(2),
            None,
            0,
            0,
        );

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
            &execution_time_estimator,
            &tx,
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
            &execution_time_estimator,
            &tx,
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
            &execution_time_estimator,
            &tx,
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
    fn test_should_defer_allow_overage(
        #[values(
            PerObjectCongestionControlMode::TotalGasBudget,
            PerObjectCongestionControlMode::TotalTxCount,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap,
            PerObjectCongestionControlMode::ExecutionTimeEstimate
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        telemetry_subscribers::init_for_testing();

        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        // Creates two shared objects and three transactions that operate on these objects.
        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let tx_gas_budget = 100;

        // Set max_accumulated_txn_cost_per_object_in_commit to only allow 1 transaction to go through
        // before overage occurs.
        let max_accumulated_txn_cost_per_object_in_commit = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => tx_gas_budget + 1,
            PerObjectCongestionControlMode::TotalTxCount => 2,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => tx_gas_budget - 1,
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 1_600_000,
        };

        let shared_object_congestion_tracker = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => {
                // Construct object execution cost as following
                //                90    102
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 102), (shared_obj_1, 90)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    0,
                )
            }
            PerObjectCongestionControlMode::TotalTxCount => {
                // Construct object execution cost as following
                //                2     3
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 3), (shared_obj_1, 2)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    0,
                )
            }
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                // Construct object execution cost as following
                //                90    100
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 100), (shared_obj_1, 90)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    Some(45), // Make the cap just less than the gas budget, there are 1 objects in tx.
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    0,
                )
            }
            PerObjectCongestionControlMode::ExecutionTimeEstimate => {
                // Construct object execution cost as following
                //                300K  1.7M
                // object 0:            |
                // object 1:      |
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 1_700_000), (shared_obj_1, 300_000)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    0,
                )
            }
        };

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &execution_time_estimator,
                    &tx,
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

        // Read/write to object 1 should go through even though the budget is exceeded.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_1, *mutable)], tx_gas_budget);
            assert!(shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &execution_time_estimator,
                    &tx,
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
                        &execution_time_estimator,
                        &tx,
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
    fn test_should_defer_allow_overage_with_burst(
        #[values(
            PerObjectCongestionControlMode::TotalGasBudget,
            PerObjectCongestionControlMode::TotalTxCount,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap,
            PerObjectCongestionControlMode::ExecutionTimeEstimate
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        telemetry_subscribers::init_for_testing();

        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let tx_gas_budget = 100;

        // Set max_accumulated_txn_cost_per_object_in_commit to allow 1 transaction to go through
        // before overage occurs.
        let max_accumulated_txn_cost_per_object_in_commit = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => tx_gas_budget,
            PerObjectCongestionControlMode::TotalTxCount => 2,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => tx_gas_budget,
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 1_600_000,
        };

        // Set burst limit to allow 1 extra transaction to go through.
        let allowed_txn_cost_overage_burst_per_object_in_commit = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => tx_gas_budget * 2,
            PerObjectCongestionControlMode::TotalTxCount => 2,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => tx_gas_budget * 2,
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 1_500_000,
        };

        let shared_object_congestion_tracker = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => {
                // Construct object execution cost as following
                //                199   301
                // object 0:            |
                // object 1:      |
                //
                // burst limit is 100 + 200 = 300
                // tx cost is 100 (gas budget)
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 301), (shared_obj_1, 199)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    allowed_txn_cost_overage_burst_per_object_in_commit,
                )
            }
            PerObjectCongestionControlMode::TotalTxCount => {
                // Construct object execution cost as following
                //                4     5
                // object 0:            |
                // object 1:      |
                //
                // burst limit is 2 + 2 = 4
                // tx cost is 1 (tx count)
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 5), (shared_obj_1, 4)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    allowed_txn_cost_overage_burst_per_object_in_commit,
                )
            }
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                // Construct object execution cost as following
                //                250   301
                // object 0:            |
                // object 1:      |
                //
                // burst limit is 100 + 200 = 300
                // tx cost is 90 (gas budget capped at 45*(1 move call + 1 input))
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 301), (shared_obj_1, 250)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    Some(45), // Make the cap just less than the gas budget, there are 1 objects in tx.
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    allowed_txn_cost_overage_burst_per_object_in_commit,
                )
            }
            PerObjectCongestionControlMode::ExecutionTimeEstimate => {
                // Construct object execution cost as following
                //                4M    2M
                // object 0:            |
                // object 1:      |
                //
                // burst limit is 1.6M + 1.5M = 3.1M
                // tx cost is 1.5M (default)
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 4_000_000), (shared_obj_1, 2_000_000)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    allowed_txn_cost_overage_burst_per_object_in_commit,
                )
            }
        };

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &execution_time_estimator,
                    &tx,
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

        // Read/write to object 1 should go through even though the budget is exceeded
        // even before the cost of this tx is considered.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_1, *mutable)], tx_gas_budget);
            assert!(shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &execution_time_estimator,
                    &tx,
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
                        &execution_time_estimator,
                        &tx,
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
    fn test_bump_object_execution_cost(
        #[values(
            PerObjectCongestionControlMode::TotalGasBudget,
            PerObjectCongestionControlMode::TotalTxCount,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap,
            PerObjectCongestionControlMode::ExecutionTimeEstimate
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        telemetry_subscribers::init_for_testing();

        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let cap_factor = Some(1);

        let mut shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(object_id_0, 5), (object_id_1, 10)],
            mode,
            Some(0), // not part of this test
            cap_factor,
            None,
            0,
            0,
        );
        assert_eq!(shared_object_congestion_tracker.max_cost(), 10);

        // Read two objects should not change the object execution cost.
        let cert = build_transaction(&[(object_id_0, false), (object_id_1, false)], 10);
        shared_object_congestion_tracker
            .bump_object_execution_cost(&execution_time_estimator, &cert);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [(object_id_0, 5), (object_id_1, 10)],
                mode,
                Some(0), // not part of this test
                cap_factor,
                None,
                0,
                0,
            )
        );
        assert_eq!(shared_object_congestion_tracker.max_cost(), 10);

        // Write to object 0 should only bump object 0's execution cost. The start cost should be object 1's cost.
        let cert = build_transaction(&[(object_id_0, true), (object_id_1, false)], 10);
        shared_object_congestion_tracker
            .bump_object_execution_cost(&execution_time_estimator, &cert);
        let expected_object_0_cost = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => 20,
            PerObjectCongestionControlMode::TotalTxCount => 11,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => 13, // 2 objects, 1 command.
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 1_500_010,
        };
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [(object_id_0, expected_object_0_cost), (object_id_1, 10)],
                mode,
                Some(0), // not part of this test
                cap_factor,
                None,
                0,
                0,
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
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 3_000_010,
        };
        shared_object_congestion_tracker
            .bump_object_execution_cost(&execution_time_estimator, &cert);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [
                    (object_id_0, expected_object_cost),
                    (object_id_1, expected_object_cost),
                    (object_id_2, expected_object_cost)
                ],
                mode,
                Some(0), // not part of this test
                cap_factor,
                None,
                0,
                0,
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
            // previous cost 3_000_010 + (unknown-command default of 1.5M)
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 4_500_010,
        };
        shared_object_congestion_tracker
            .bump_object_execution_cost(&execution_time_estimator, &cert);
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [
                    (object_id_0, expected_object_cost),
                    (object_id_1, expected_object_cost),
                    (object_id_2, expected_object_cost)
                ],
                mode,
                Some(0), // not part of this test
                cap_factor,
                None,
                0,
                0,
            )
        );
        assert_eq!(
            shared_object_congestion_tracker.max_cost(),
            expected_object_cost
        );
    }

    #[rstest]
    fn test_accumulated_debts(
        #[values(
            PerObjectCongestionControlMode::TotalGasBudget,
            PerObjectCongestionControlMode::TotalTxCount,
            PerObjectCongestionControlMode::TotalGasBudgetWithCap,
            PerObjectCongestionControlMode::ExecutionTimeEstimate
        )]
        mode: PerObjectCongestionControlMode,
    ) {
        telemetry_subscribers::init_for_testing();

        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        // Creates two shared objects and three transactions that operate on these objects.
        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let tx_gas_budget = 100;

        // Set max_accumulated_txn_cost_per_object_in_commit to only allow 1 transaction to go through
        // before overage occurs.
        let max_accumulated_txn_cost_per_object_in_commit = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget
            | PerObjectCongestionControlMode::TotalGasBudgetWithCap => 90,
            PerObjectCongestionControlMode::TotalTxCount => 2,
            PerObjectCongestionControlMode::ExecutionTimeEstimate => 1_600_000,
        };

        let mut shared_object_congestion_tracker = match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => {
                // Starting with two objects with accumulated cost 80.
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 80), (shared_obj_1, 80)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    // Set a burst limit to verify that it does not affect debt calculation.
                    max_accumulated_txn_cost_per_object_in_commit * 5,
                )
            }
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                // Starting with two objects with accumulated cost 80.
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 80), (shared_obj_1, 80)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    Some(45),
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    // Set a burst limit to verify that it does not affect debt calculation.
                    max_accumulated_txn_cost_per_object_in_commit * 5,
                )
            }
            PerObjectCongestionControlMode::TotalTxCount => {
                // Starting with two objects with accumulated tx count 2.
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 2), (shared_obj_1, 2)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    // Set a burst limit to verify that it does not affect debt calculation.
                    max_accumulated_txn_cost_per_object_in_commit * 5,
                )
            }
            PerObjectCongestionControlMode::ExecutionTimeEstimate => {
                // Starting with two objects with accumulated cost 500K.
                SharedObjectCongestionTracker::new(
                    [(shared_obj_0, 500_000), (shared_obj_1, 500_000)],
                    mode,
                    Some(max_accumulated_txn_cost_per_object_in_commit),
                    None,
                    None,
                    max_accumulated_txn_cost_per_object_in_commit * 10,
                    // Set a burst limit to verify that it does not affect debt calculation.
                    max_accumulated_txn_cost_per_object_in_commit * 5,
                )
            }
        };

        // Simulate a tx on object 0 that exceeds the budget.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            shared_object_congestion_tracker
                .bump_object_execution_cost(&execution_time_estimator, &tx);
        }

        // Verify that accumulated_debts reports the debt for object 0.
        let accumulated_debts = shared_object_congestion_tracker.accumulated_debts();
        assert_eq!(accumulated_debts.len(), 1);
        match mode {
            PerObjectCongestionControlMode::None => unreachable!(),
            PerObjectCongestionControlMode::TotalGasBudget => {
                assert_eq!(accumulated_debts[0], (shared_obj_0, 90)); // init 80 + cost 100 - budget 90 = 90
            }
            PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                assert_eq!(accumulated_debts[0], (shared_obj_0, 80)); // init 80 + capped cost 90 - budget 90 = 80
            }
            PerObjectCongestionControlMode::TotalTxCount => {
                assert_eq!(accumulated_debts[0], (shared_obj_0, 1)); // init 2 + 1 tx - budget 2 = 1
            }
            PerObjectCongestionControlMode::ExecutionTimeEstimate => {
                // init 500K + 1.5M tx - budget 1.6M = 400K
                assert_eq!(accumulated_debts[0], (shared_obj_0, 400_000));
            }
        }
    }

    #[test]
    fn test_accumulated_debts_empty() {
        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(object_id_0, 5), (object_id_1, 10), (object_id_2, 100)],
            PerObjectCongestionControlMode::TotalGasBudget,
            Some(100),
            None,
            None,
            0,
            0,
        );

        let accumulated_debts = shared_object_congestion_tracker.accumulated_debts();
        assert!(accumulated_debts.is_empty());
    }

    #[test]
    fn test_tx_cost_absolute_cap() {
        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let tx_gas_budget = 2000;

        let mut shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(object_id_0, 5), (object_id_1, 10), (object_id_2, 100)],
            PerObjectCongestionControlMode::TotalGasBudgetWithCap,
            Some(100),
            Some(1000),
            Some(2),
            1000,
            0,
        );

        // Create a transaction using all three objects
        let tx = build_transaction(
            &[
                (object_id_0, false),
                (object_id_1, false),
                (object_id_2, true),
            ],
            tx_gas_budget,
        );

        // Verify that the transaction is allowed to execute.
        // 2000 gas budget would exceed overage limit of 1000 but is capped to 200 by the absolute cap.
        assert!(shared_object_congestion_tracker
            .should_defer_due_to_object_congestion(
                &execution_time_estimator,
                &tx,
                &HashMap::new(),
                0,
            )
            .is_none());

        // Verify max cost after bumping is limited by the absolute cap.
        shared_object_congestion_tracker.bump_object_execution_cost(&execution_time_estimator, &tx);
        assert_eq!(300, shared_object_congestion_tracker.max_cost());

        // Verify accumulated debts still uses the per-commit budget to decrement.
        let accumulated_debts = shared_object_congestion_tracker.accumulated_debts();
        assert_eq!(accumulated_debts.len(), 1);
        assert_eq!(accumulated_debts[0], (object_id_2, 200));
    }
}
