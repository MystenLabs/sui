// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use crate::authority::AuthorityMetrics;
use crate::authority::AuthorityState;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_adapter::consensus_tests::test_user_transaction;
use crate::consensus_test_utils::{
    self, CapturedTransactions, TestConsensusCommit, TestConsensusHandlerSetup,
};
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::object::Object;

use crate::consensus_handler::ConsensusHandler;

struct DoubleSpendTestSetup {
    authority_state: Arc<AuthorityState>,
    consensus_handler: ConsensusHandler<CheckpointServiceNoop>,
    captured_transactions: CapturedTransactions,
    metrics: Arc<AuthorityMetrics>,
    sender: SuiAddress,
    keypair: AccountKeyPair,
    contested_object: Object,
    gas_objects: Vec<Object>,
}

impl DoubleSpendTestSetup {
    async fn new(protocol_config: ProtocolConfig, num_gas_objects: usize) -> Self {
        let (sender, keypair): (_, AccountKeyPair) = deterministic_random_account_key();

        let gas_objects: Vec<Object> = (0..num_gas_objects)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();
        let contested_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);

        let mut genesis_objects = gas_objects.clone();
        genesis_objects.push(contested_object.clone());

        let authority_state = TestAuthorityBuilder::new()
            .with_reference_gas_price(1000)
            .with_protocol_config(protocol_config)
            .build()
            .await;
        authority_state.insert_genesis_objects(&genesis_objects);

        let TestConsensusHandlerSetup {
            consensus_handler,
            captured_transactions,
            metrics,
        } = consensus_test_utils::setup_consensus_handler_for_testing(&authority_state).await;

        Self {
            authority_state,
            consensus_handler,
            captured_transactions,
            metrics,
            sender,
            keypair,
            contested_object,
            gas_objects,
        }
    }

    /// Build consensus transactions where each gas object produces a transaction
    /// that uses the contested owned object.
    async fn build_competing_consensus_txns(&self) -> Vec<ConsensusTransaction> {
        let mut consensus_txns = Vec::new();
        for gas_object in &self.gas_objects {
            let tx = test_user_transaction(
                &self.authority_state,
                self.sender,
                &self.keypair,
                gas_object.clone(),
                vec![self.contested_object.clone()],
            )
            .await;
            consensus_txns.push(ConsensusTransaction::new_user_transaction_v2_message(
                &self.authority_state.name,
                tx.into(),
            ));
        }
        consensus_txns
    }

    /// Submit a commit and return the number of scheduled transactions.
    async fn submit_commit_and_count_scheduled(&mut self, commit: TestConsensusCommit) -> usize {
        self.consensus_handler.handle_consensus_commit(commit).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut captured = self.captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected at least one scheduler message"
        );
        let (scheduled_txns, _) = captured.remove(0);
        scheduled_txns.len()
    }
}

fn base_protocol_config() -> ProtocolConfig {
    ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown)
}

/// Two transactions in the same commit compete for the same owned object. The first
/// to acquire the lock wins and is scheduled; the second is dropped (failed lock).
/// Deferral is not yet enabled on this branch, so the winner runs immediately - but
/// the contention must be detected and surfaced through the double-spend metrics.
#[tokio::test]
async fn test_double_spend_detection_emits_metrics() {
    telemetry_subscribers::init_for_testing();

    let mut setup = DoubleSpendTestSetup::new(base_protocol_config(), 2).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // prologue + winner (loser dropped due to lock conflict). No deferral on this branch.
    assert_eq!(count, 2, "Expected prologue + winner (detection only)");

    // The winner that contested an owned object is counted exactly once.
    assert_eq!(
        setup.metrics.consensus_handler_double_spend_deferrals.get(),
        1,
        "Expected one detected double-spend winner"
    );

    // The contested object is a non-gas owned object, so the non-gas histogram records
    // the conflict while the gas histogram records a zero observation.
    let non_gas = setup
        .metrics
        .consensus_handler_double_spend_conflict_count
        .with_label_values(&["non_gas_object"]);
    assert_eq!(non_gas.get_sample_count(), 1);
    assert_eq!(non_gas.get_sample_sum(), 1.0);

    let gas = setup
        .metrics
        .consensus_handler_double_spend_conflict_count
        .with_label_values(&["gas_object"]);
    assert_eq!(gas.get_sample_count(), 1);
    assert_eq!(gas.get_sample_sum(), 0.0);
}

/// When only one transaction uses an owned object there is no contention, so no
/// double-spend metrics should be recorded.
#[tokio::test]
async fn test_no_contention_no_metrics() {
    telemetry_subscribers::init_for_testing();

    let mut setup = DoubleSpendTestSetup::new(base_protocol_config(), 1).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // prologue + the single transaction (no contention).
    assert_eq!(count, 2, "Expected prologue + user tx (no contention)");
    assert_eq!(
        setup.metrics.consensus_handler_double_spend_deferrals.get(),
        0,
        "Expected no detected double-spend without contention"
    );
}
