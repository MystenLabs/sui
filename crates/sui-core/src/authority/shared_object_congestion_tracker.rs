// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::execution_time_estimator::ExecutionTimeEstimator;
use crate::authority::transaction_deferral::DeferralKey;
use crate::consensus_handler::{ConsensusCommitInfo, IndirectStateObserver};
use mysten_common::fatal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sui_protocol_config::{
    ExecutionTimeEstimateParams, PerObjectCongestionControlMode, ProtocolConfig,
};
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::Round;
use sui_types::transaction::{GasPaymentMode, SharedInputObject, TransactionDataAPI};
use tracing::{debug, trace};

#[derive(PartialEq, Eq, Clone, Debug)]
struct Params {
    params: ExecutionTimeEstimateParams,
    for_randomness: bool,
}

impl Params {
    // Get the target budget per commit. Over the long term, the scheduler will try to
    // schedule no more than this much work per object per commit on average.
    pub fn commit_budget(&self, commit_info: &ConsensusCommitInfo) -> u64 {
        let estimated_commit_period = commit_info.estimated_commit_period();
        let commit_period_micros = estimated_commit_period.as_micros() as u64;
        let mut budget = commit_period_micros.saturating_mul(self.params.target_utilization) / 100;
        if self.for_randomness {
            budget = budget.saturating_mul(self.params.randomness_scalar) / 100;
        }
        budget
    }

    // The amount scheduled in a commit can "burst" up to this much over the target budget.
    // The per-object debt limit will enforce the average limit over time.
    pub fn max_burst(&self) -> u64 {
        let mut burst = self.params.allowed_txn_cost_overage_burst_limit_us;
        if self.for_randomness {
            burst = burst.saturating_mul(self.params.randomness_scalar) / 100;
        }
        burst
    }
}

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
    params: Params,
    log_entries: Option<Vec<TransactionCostLogEntry>>,
}

impl SharedObjectCongestionTracker {
    pub fn new(
        initial_object_debts: impl IntoIterator<Item = (ObjectID, u64)>,
        params: ExecutionTimeEstimateParams,
        for_randomness: bool,
        enable_logging: bool,
    ) -> Self {
        let object_execution_cost: HashMap<ObjectID, u64> =
            initial_object_debts.into_iter().collect();
        trace!(
            "created SharedObjectCongestionTracker with
             {} initial object debts,
             params: {params:?},
             for_randomness: {for_randomness},",
            object_execution_cost.len(),
        );
        Self {
            object_execution_cost,
            params: Params {
                params,
                for_randomness,
            },
            log_entries: enable_logging.then(Vec::new),
        }
    }

    pub fn from_protocol_config(
        initial_object_debts: impl IntoIterator<Item = (ObjectID, u64)>,
        protocol_config: &ProtocolConfig,
        for_randomness: bool,
        enable_logging: bool,
    ) -> Self {
        let PerObjectCongestionControlMode::ExecutionTimeEstimate(params) =
            protocol_config.per_object_congestion_control_mode()
        else {
            fatal!(
                "support for congestion control modes other than PerObjectCongestionControlMode::ExecutionTimeEstimate has been removed"
            );
        };
        Self::new(initial_object_debts, params, for_randomness, enable_logging)
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

    pub(crate) fn get_tx_cost(
        &self,
        execution_time_estimator: &ExecutionTimeEstimator,
        cert: &VerifiedExecutableTransaction,
        protocol_config: &ProtocolConfig,
        indirect_state_observer: &mut IndirectStateObserver,
    ) -> (u64, Option<(ObjectID, u64)>) {
        let estimate_us = Self::estimate_tx_cost(execution_time_estimator, cert);
        let allowance_settlement = Self::gas_allowance_settlement_cost(protocol_config, cert);
        let tx_cost = allowance_settlement
            .as_ref()
            .map(|(_, settlement_cost)| estimate_us.max(*settlement_cost))
            .unwrap_or(estimate_us);
        // Historically this was an Option<u64>, must keep it that way for consistency.
        indirect_state_observer.observe_indirect_state(&Some(tx_cost));
        (tx_cost, allowance_settlement)
    }

    pub(crate) fn gas_allowance_settlement_cost(
        protocol_config: &ProtocolConfig,
        cert: &VerifiedExecutableTransaction,
    ) -> Option<(ObjectID, u64)> {
        protocol_config
            .enable_gas_allowances()
            .then(|| cert.transaction_data().gas_payment_mode())
            .and_then(|payment_mode| match payment_mode {
                GasPaymentMode::Allowance(allowance_id) => Some((
                    allowance_id,
                    protocol_config.gas_allowance_congestion_control_cost(),
                )),
                _ => None,
            })
    }

    fn estimate_tx_cost(
        execution_time_estimator: &ExecutionTimeEstimator,
        cert: &VerifiedExecutableTransaction,
    ) -> u64 {
        let estimate_us = execution_time_estimator
            .get_estimate(cert.transaction_data())
            .as_micros()
            .try_into()
            .unwrap_or(u64::MAX);
        if estimate_us >= 15_000 {
            let digest = cert.digest();
            debug!(
                ?digest,
                "expensive tx cost estimate detected: {estimate_us}us"
            );
        }
        estimate_us
    }

    // Given a transaction, returns the deferral key and the congested objects if the transaction should be deferred.
    pub fn should_defer_due_to_object_congestion(
        &self,
        cert: &VerifiedExecutableTransaction,
        previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
        commit_info: &ConsensusCommitInfo,
    ) -> Option<(DeferralKey, Vec<ObjectID>)> {
        let commit_round = commit_info.round;

        let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();
        if shared_input_objects.is_empty() {
            // No shared object used by this transaction. No need to defer.
            return None;
        }

        // Allow tx if it's within configured limits.
        let start_cost = self.compute_tx_start_at_cost(&shared_input_objects);
        let budget = self.params.commit_budget(commit_info);
        let burst_limit = budget.saturating_add(self.params.max_burst());
        if start_cost <= burst_limit {
            return None;
        }

        // Finds out the congested objects.
        //
        // Note that the congested objects here may be caused by transaction dependency of other congested objects.
        // Consider in a consensus commit, there are many transactions touching object A, and later in processing the
        // consensus commit, there is a transaction touching both object A and B. Although there are fewer transactions
        // touching object B, because it's starting execution is delayed due to dependency to other transactions on
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
        tx_cost: u64,
        cert: &VerifiedExecutableTransaction,
    ) {
        let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();
        if shared_input_objects.is_empty() {
            return;
        }

        let start_cost = self.compute_tx_start_at_cost(&shared_input_objects);
        let end_cost = start_cost.saturating_add(tx_cost);

        for obj in shared_input_objects {
            if obj.is_accessed_exclusively() {
                let old_end_cost = self.object_execution_cost.insert(obj.id, end_cost);
                assert!(old_end_cost.is_none() || old_end_cost.unwrap() <= end_cost);
            }
        }

        if let Some(entries) = &mut self.log_entries {
            entries.push(TransactionCostLogEntry {
                tx_digest: *cert.digest(),
                start_cost,
                end_cost,
            });
        }
    }

    /// Adds the cancellation settlement cost only to the retained allowance. Other shared inputs
    /// use cancellation sentinels and never execute.
    pub(crate) fn bump_gas_allowance_settlement_cost(
        &mut self,
        allowance_id: ObjectID,
        settlement_cost: u64,
        tx_digest: TransactionDigest,
    ) {
        let start_cost = *self.object_execution_cost.get(&allowance_id).unwrap_or(&0);
        let end_cost = start_cost.saturating_add(settlement_cost);
        let old_end_cost = self.object_execution_cost.insert(allowance_id, end_cost);
        assert!(old_end_cost.is_none() || old_end_cost.unwrap() <= end_cost);

        if let Some(entries) = &mut self.log_entries {
            entries.push(TransactionCostLogEntry {
                tx_digest,
                start_cost,
                end_cost,
            });
        }
    }

    // Returns end-of-commit data from congestion control:
    // - Accumulated debts for objects whose budgets have been exceeded over the course of
    //   the commit.
    // - Detailed per-object congestion data for logging.
    // Consumes the tracker object, since this should only be called once after
    // all tx have been processed.
    pub fn finish_commit(self, commit_info: &ConsensusCommitInfo) -> FinishedCommitData {
        let commit_budget = self.params.commit_budget(commit_info);
        let log_entries = self.log_entries.unwrap_or_default();
        let final_object_execution_costs = self.object_execution_cost.clone();
        let accumulated_debts = self
            .object_execution_cost
            .into_iter()
            .filter_map(|(obj_id, cost)| {
                let remaining_cost = cost.saturating_sub(commit_budget);
                if remaining_cost > 0 {
                    Some((obj_id, remaining_cost))
                } else {
                    None
                }
            })
            .collect();
        FinishedCommitData {
            accumulated_debts,
            log_entries,
            final_object_execution_costs,
            commit_budget,
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

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TransactionCostLogEntry {
    pub tx_digest: TransactionDigest,
    pub start_cost: u64,
    pub end_cost: u64,
}

pub struct FinishedCommitData {
    pub accumulated_debts: Vec<(ObjectID, u64)>,
    pub log_entries: Vec<TransactionCostLogEntry>,
    pub final_object_execution_costs: HashMap<ObjectID, u64>,
    pub commit_budget: u64,
}

#[cfg(test)]
mod object_cost_tests {
    use super::*;

    use std::time::Duration;
    use sui_protocol_config::{
        Chain, ExecutionTimeEstimateParams, ProtocolConfig, ProtocolVersion,
    };
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::Identifier;
    use sui_types::base_types::{SequenceNumber, random_object_ref};
    use sui_types::crypto::{AccountKeyPair, get_key_pair};
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{
        CallArg, GasData, ObjectArg, SharedObjectMutability, Transaction, TransactionData,
        TransactionDataV1, TransactionExpiration, TransactionKind, VerifiedTransaction,
    };

    fn default_params() -> ExecutionTimeEstimateParams {
        ExecutionTimeEstimateParams {
            target_utilization: 0,
            allowed_txn_cost_overage_burst_limit_us: 0,
            max_estimate_us: u64::MAX,
            randomness_scalar: 0,
            stored_observations_num_included_checkpoints: 10,
            stored_observations_limit: u64::MAX,
            stake_weighted_median_threshold: 0,
            default_none_duration_for_new_keys: false,
            observations_chunk_size: Some(18),
        }
    }

    fn construct_shared_input_objects(objects: &[(ObjectID, bool)]) -> Vec<SharedInputObject> {
        objects
            .iter()
            .map(|(id, mutable)| SharedInputObject {
                id: *id,
                initial_shared_version: SequenceNumber::new(),
                mutability: if *mutable {
                    SharedObjectMutability::Mutable
                } else {
                    SharedObjectMutability::Immutable
                },
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
            default_params(),
            false,
            false,
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
                                    mutability: if *mutable {
                                        SharedObjectMutability::Mutable
                                    } else {
                                        SharedObjectMutability::Immutable
                                    },
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
        let mut tx_builder =
            TestTransactionBuilder::new(sender, gas_object, 1000).with_gas_budget(gas_budget);
        {
            let pt_builder = tx_builder.ptb_builder_mut();
            let mut arguments = Vec::new();
            for object in objects {
                arguments.push(
                    pt_builder
                        .obj(ObjectArg::SharedObject {
                            id: object.0,
                            initial_shared_version: SequenceNumber::new(),
                            mutability: if object.1 {
                                SharedObjectMutability::Mutable
                            } else {
                                SharedObjectMutability::Immutable
                            },
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
        }

        VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(tx_builder.build_and_sign(&keypair)),
            0,
        )
    }

    fn build_allowance_transaction(
        allowance_id: ObjectID,
        number_of_commands: u64,
    ) -> VerifiedExecutableTransaction {
        let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
        let mut ptb_builder = ProgrammableTransactionBuilder::new();
        ptb_builder
            .obj(ObjectArg::SharedObject {
                id: allowance_id,
                initial_shared_version: SequenceNumber::new(),
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();
        for _ in 0..number_of_commands {
            ptb_builder.programmable_move_call(
                ObjectID::random(),
                Identifier::new("unimportant_module").unwrap(),
                Identifier::new("unimportant_function").unwrap(),
                vec![],
                vec![],
            );
        }
        let transaction = Transaction::from_data_and_signer(
            TransactionData::V1(TransactionDataV1 {
                kind: TransactionKind::ProgrammableTransaction(ptb_builder.finish()),
                sender,
                gas_data: GasData {
                    payment: vec![],
                    owner: allowance_id.into(),
                    price: 1_000,
                    budget: 10,
                },
                expiration: TransactionExpiration::None,
            }),
            vec![&keypair],
        );
        VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(transaction),
            0,
        )
    }

    #[test]
    fn allowance_settlement_floor_only_applies_to_enabled_allowance_payments() {
        let tracker = SharedObjectCongestionTracker::new([], default_params(), false, false);
        let estimator = ExecutionTimeEstimator::new_for_testing();
        let allowance_id = ObjectID::random();
        let allowance_tx = build_allowance_transaction(allowance_id, 0);
        let expensive_allowance_tx = build_allowance_transaction(allowance_id, 7);
        let ordinary_tx = build_programmable_transaction(&[(allowance_id, true)], 0, 10);

        let enabled = ProtocolConfig::get_for_version(ProtocolVersion::new(142), Chain::Unknown);
        let disabled = ProtocolConfig::get_for_version(ProtocolVersion::new(142), Chain::Mainnet);

        let mut floored_observer = IndirectStateObserver::new();
        assert_eq!(
            tracker
                .get_tx_cost(&estimator, &allowance_tx, &enabled, &mut floored_observer,)
                .0,
            1_000
        );
        let mut expected_observer = IndirectStateObserver::new();
        expected_observer.observe_indirect_state(&Some(1_000u64));
        let additional_state = ConsensusCommitInfo::new_for_congestion_test(0, 0, Duration::ZERO)
            .additional_state_digest();
        assert_eq!(
            floored_observer.fold_with(additional_state),
            expected_observer.fold_with(additional_state)
        );
        assert_eq!(
            SharedObjectCongestionTracker::gas_allowance_settlement_cost(&enabled, &allowance_tx),
            Some((allowance_id, 1_000))
        );
        assert_eq!(
            tracker
                .get_tx_cost(
                    &estimator,
                    &allowance_tx,
                    &disabled,
                    &mut IndirectStateObserver::new(),
                )
                .0,
            0
        );
        assert_eq!(
            tracker
                .get_tx_cost(
                    &estimator,
                    &ordinary_tx,
                    &enabled,
                    &mut IndirectStateObserver::new(),
                )
                .0,
            0
        );

        let (estimated_cost, Some((allowance_id, settlement_cost))) = tracker.get_tx_cost(
            &estimator,
            &expensive_allowance_tx,
            &enabled,
            &mut IndirectStateObserver::new(),
        ) else {
            panic!("enabled allowance transaction must have settlement cost");
        };
        assert!(estimated_cost > settlement_cost);

        let mut cancellation_tracker =
            SharedObjectCongestionTracker::new([], default_params(), false, false);
        cancellation_tracker.bump_gas_allowance_settlement_cost(
            allowance_id,
            settlement_cost,
            *expensive_allowance_tx.digest(),
        );
        assert_eq!(
            cancellation_tracker.max_cost(),
            settlement_cost,
            "cancellation does not account for unexecuted PTB commands"
        );
    }

    #[test]
    fn test_should_defer_return_correct_congested_objects() {
        // Creates two shared objects and three transactions that operate on these objects.
        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let tx_gas_budget = 100;

        // Construct object execution cost:
        // object 0 has cost 750 (which exceeds burst limit)
        // object 1 has cost 0 (within burst limit)
        let shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(shared_obj_0, 750), (shared_obj_1, 0)],
            default_params(),
            false,
            false,
        );

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &tx,
                    &HashMap::new(),
                    &ConsensusCommitInfo::new_for_congestion_test(
                        0,
                        0,
                        Duration::from_micros(1_500),
                    ),
                )
            {
                assert_eq!(congested_objects.len(), 1);
                assert_eq!(congested_objects[0], shared_obj_0);
            } else {
                panic!("should defer");
            }
        }

        // Read/write to object 1 should go through.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_1, *mutable)], tx_gas_budget);
            assert!(
                shared_object_congestion_tracker
                    .should_defer_due_to_object_congestion(
                        &tx,
                        &HashMap::new(),
                        &ConsensusCommitInfo::new_for_congestion_test(
                            0,
                            0,
                            Duration::from_micros(1_500),
                        ),
                    )
                    .is_none()
            );
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
                        &HashMap::new(),
                        &ConsensusCommitInfo::new_for_congestion_test(
                            0,
                            0,
                            Duration::from_micros(1_500),
                        ),
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
        let tx = build_transaction(&[(shared_obj_0, true)], 100);

        // Set initial cost that exceeds 0 burst limit
        let shared_object_congestion_tracker =
            SharedObjectCongestionTracker::new([(shared_obj_0, 1)], default_params(), false, false);

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
            &previously_deferred_tx_digests,
            &ConsensusCommitInfo::new_for_congestion_test(
                10,
                10,
                Duration::from_micros(10_000_000),
            ),
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 10);
        } else {
            panic!("should defer");
        }

        // Insert `tx` as previously deferred transaction due to randomness.
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
            &previously_deferred_tx_digests,
            &ConsensusCommitInfo::new_for_congestion_test(
                10,
                10,
                Duration::from_micros(10_000_000),
            ),
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 4);
        } else {
            panic!("should defer");
        }

        // Insert `tx` as previously deferred consensus transaction.
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
            &previously_deferred_tx_digests,
            &ConsensusCommitInfo::new_for_congestion_test(
                10,
                10,
                Duration::from_micros(10_000_000),
            ),
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 5);
        } else {
            panic!("should defer");
        }
    }

    #[test]
    fn test_should_defer_allow_overage() {
        telemetry_subscribers::init_for_testing();

        // Creates two shared objects.
        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let tx_gas_budget = 100;

        // Construct object execution cost:
        // object 0 has cost 1.7M (exceeds burst limit of ~1.6M with 16% utilization on 10s commit)
        // object 1 has cost 300K (within burst limit)
        let shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(shared_obj_0, 1_700_000), (shared_obj_1, 300_000)],
            ExecutionTimeEstimateParams {
                target_utilization: 16,
                allowed_txn_cost_overage_burst_limit_us: 0,
                randomness_scalar: 0,
                max_estimate_us: u64::MAX,
                stored_observations_num_included_checkpoints: 10,
                stored_observations_limit: u64::MAX,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: false,
                observations_chunk_size: Some(18),
            },
            false,
            false,
        );

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &tx,
                    &HashMap::new(),
                    &ConsensusCommitInfo::new_for_congestion_test(
                        0,
                        0,
                        Duration::from_micros(10_000_000),
                    ),
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
            assert!(
                shared_object_congestion_tracker
                    .should_defer_due_to_object_congestion(
                        &tx,
                        &HashMap::new(),
                        &ConsensusCommitInfo::new_for_congestion_test(
                            0,
                            0,
                            Duration::from_micros(10_000_000)
                        ),
                    )
                    .is_none()
            );
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
                        &HashMap::new(),
                        &ConsensusCommitInfo::new_for_congestion_test(
                            0,
                            0,
                            Duration::from_micros(10_000_000),
                        ),
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
    fn test_should_defer_allow_overage_with_burst() {
        telemetry_subscribers::init_for_testing();

        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let tx_gas_budget = 100;

        // Construct object execution cost:
        // object 0 has cost 4M (exceeds burst limit of 1.6M + 1.5M = 3.1M)
        // object 1 has cost 2M (within burst limit)
        // tx cost is ~1M (default estimate for unknown command)
        let shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(shared_obj_0, 4_000_000), (shared_obj_1, 2_000_000)],
            ExecutionTimeEstimateParams {
                target_utilization: 16,
                allowed_txn_cost_overage_burst_limit_us: 1_500_000,
                randomness_scalar: 0,
                max_estimate_us: u64::MAX,
                stored_observations_num_included_checkpoints: 10,
                stored_observations_limit: u64::MAX,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: false,
                observations_chunk_size: Some(18),
            },
            false,
            false,
        );

        // Read/write to object 0 should be deferred.
        for mutable in [true, false].iter() {
            let tx = build_transaction(&[(shared_obj_0, *mutable)], tx_gas_budget);
            if let Some((_, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    &tx,
                    &HashMap::new(),
                    &ConsensusCommitInfo::new_for_congestion_test(
                        0,
                        0,
                        Duration::from_micros(10_000_000),
                    ),
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
            assert!(
                shared_object_congestion_tracker
                    .should_defer_due_to_object_congestion(
                        &tx,
                        &HashMap::new(),
                        &ConsensusCommitInfo::new_for_congestion_test(
                            0,
                            0,
                            Duration::from_micros(10_000_000)
                        ),
                    )
                    .is_none()
            );
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
                        &HashMap::new(),
                        &ConsensusCommitInfo::new_for_congestion_test(
                            0,
                            0,
                            Duration::from_micros(10_000_000),
                        ),
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
    fn test_bump_object_execution_cost() {
        telemetry_subscribers::init_for_testing();

        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let params = default_params();
        let mut shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            [(object_id_0, 5), (object_id_1, 10)],
            params,
            false,
            false,
        );
        assert_eq!(shared_object_congestion_tracker.max_cost(), 10);

        // Read two objects should not change the object execution cost.
        let cert = build_transaction(&[(object_id_0, false), (object_id_1, false)], 10);
        shared_object_congestion_tracker.bump_object_execution_cost(
            shared_object_congestion_tracker
                .get_tx_cost(
                    &execution_time_estimator,
                    &cert,
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    &mut IndirectStateObserver::new(),
                )
                .0,
            &cert,
        );
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [(object_id_0, 5), (object_id_1, 10)],
                params,
                false,
                false,
            )
        );
        assert_eq!(shared_object_congestion_tracker.max_cost(), 10);

        // Write to object 0 should only bump object 0's execution cost. The start cost should be object 1's cost.
        let cert = build_transaction(&[(object_id_0, true), (object_id_1, false)], 10);
        shared_object_congestion_tracker.bump_object_execution_cost(
            shared_object_congestion_tracker
                .get_tx_cost(
                    &execution_time_estimator,
                    &cert,
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    &mut IndirectStateObserver::new(),
                )
                .0,
            &cert,
        );
        // start cost (10) + tx cost (~1000 for unknown command)
        let expected_object_0_cost = 1_010;
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [(object_id_0, expected_object_0_cost), (object_id_1, 10)],
                params,
                false,
                false,
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
        // previous cost (1_010) + tx cost (~1000 for unknown command)
        let expected_object_cost = 2_010;
        shared_object_congestion_tracker.bump_object_execution_cost(
            shared_object_congestion_tracker
                .get_tx_cost(
                    &execution_time_estimator,
                    &cert,
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    &mut IndirectStateObserver::new(),
                )
                .0,
            &cert,
        );
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [
                    (object_id_0, expected_object_cost),
                    (object_id_1, expected_object_cost),
                    (object_id_2, expected_object_cost)
                ],
                params,
                false,
                false,
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
        // previous cost 2_010 + (unknown-command default of 1000 * 7 commands)
        let expected_object_cost = 9_010;
        shared_object_congestion_tracker.bump_object_execution_cost(
            shared_object_congestion_tracker
                .get_tx_cost(
                    &execution_time_estimator,
                    &cert,
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    &mut IndirectStateObserver::new(),
                )
                .0,
            &cert,
        );
        assert_eq!(
            shared_object_congestion_tracker,
            SharedObjectCongestionTracker::new(
                [
                    (object_id_0, expected_object_cost),
                    (object_id_1, expected_object_cost),
                    (object_id_2, expected_object_cost)
                ],
                params,
                false,
                false,
            )
        );
        assert_eq!(
            shared_object_congestion_tracker.max_cost(),
            expected_object_cost
        );
    }

    #[test]
    fn test_finish_commit_empty_debts() {
        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();

        let tracker = SharedObjectCongestionTracker::new(
            [(object_id_0, 0), (object_id_1, 0), (object_id_2, 0)],
            default_params(),
            false,
            false,
        );

        let data = tracker.finish_commit(&ConsensusCommitInfo::new_for_congestion_test(
            0,
            0,
            Duration::ZERO,
        ));
        assert!(data.accumulated_debts.is_empty());
    }

    #[test]
    fn test_finish_commit_returns_correct_debts() {
        telemetry_subscribers::init_for_testing();

        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let params = ExecutionTimeEstimateParams {
            target_utilization: 100,
            allowed_txn_cost_overage_burst_limit_us: 1_600 * 5,
            randomness_scalar: 0,
            max_estimate_us: u64::MAX,
            stored_observations_num_included_checkpoints: 10,
            stored_observations_limit: u64::MAX,
            stake_weighted_median_threshold: 0,
            default_none_duration_for_new_keys: false,
            observations_chunk_size: Some(18),
        };
        let mut tracker = SharedObjectCongestionTracker::new(
            [(shared_obj_0, 500), (shared_obj_1, 500)],
            params,
            false,
            false,
        );

        let tx = build_transaction(&[(shared_obj_0, true)], 100);
        tracker.bump_object_execution_cost(
            tracker
                .get_tx_cost(
                    &execution_time_estimator,
                    &tx,
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    &mut IndirectStateObserver::new(),
                )
                .0,
            &tx,
        );

        let commit_info =
            ConsensusCommitInfo::new_for_congestion_test(0, 0, Duration::from_micros(800));
        let data = tracker.finish_commit(&commit_info);

        assert_eq!(data.accumulated_debts.len(), 1);
        assert_eq!(data.accumulated_debts[0], (shared_obj_0, 700));
        assert_eq!(data.commit_budget, 800);
    }

    #[test]
    fn test_finish_commit_with_logging() {
        telemetry_subscribers::init_for_testing();

        let execution_time_estimator = ExecutionTimeEstimator::new_for_testing();

        let shared_obj_0 = ObjectID::random();
        let shared_obj_1 = ObjectID::random();

        let params = ExecutionTimeEstimateParams {
            target_utilization: 100,
            allowed_txn_cost_overage_burst_limit_us: 1_600 * 5,
            randomness_scalar: 0,
            max_estimate_us: u64::MAX,
            stored_observations_num_included_checkpoints: 10,
            stored_observations_limit: u64::MAX,
            stake_weighted_median_threshold: 0,
            default_none_duration_for_new_keys: false,
            observations_chunk_size: Some(18),
        };
        let mut tracker = SharedObjectCongestionTracker::new(
            [(shared_obj_0, 0), (shared_obj_1, 0)],
            params,
            false,
            true,
        );

        let tx = build_transaction(&[(shared_obj_0, true), (shared_obj_1, true)], 100);
        let tx_digest = *tx.digest();
        tracker.bump_object_execution_cost(
            tracker
                .get_tx_cost(
                    &execution_time_estimator,
                    &tx,
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    &mut IndirectStateObserver::new(),
                )
                .0,
            &tx,
        );

        let commit_info =
            ConsensusCommitInfo::new_for_congestion_test(0, 0, Duration::from_micros(800));
        let data = tracker.finish_commit(&commit_info);

        assert_eq!(data.log_entries.len(), 1);
        assert_eq!(data.log_entries[0].tx_digest, tx_digest);
        assert_eq!(data.log_entries[0].start_cost, 0);
        assert!(
            data.final_object_execution_costs
                .contains_key(&shared_obj_0)
        );
        assert!(
            data.final_object_execution_costs
                .contains_key(&shared_obj_1)
        );
    }
}
