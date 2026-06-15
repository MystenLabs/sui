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

fn protocol_config_with_double_spend_deferral() -> ProtocolConfig {
    let mut config = ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    config.set_defer_owned_object_double_spend_for_testing(true);
    config
}

/// Detection + metrics fire regardless of the deferral feature flag: two transactions
/// in the same commit compete for the same owned object, the winner is scheduled (since
/// deferral is disabled here) and the contention is recorded through the metrics.
#[tokio::test]
async fn test_double_spend_detection_emits_metrics() {
    telemetry_subscribers::init_for_testing();

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.set_defer_owned_object_double_spend_for_testing(false);

    let mut setup = DoubleSpendTestSetup::new(protocol_config, 2).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // prologue + winner (loser dropped due to lock conflict), deferral disabled.
    assert_eq!(count, 2, "Expected prologue + winner (detection only)");

    // The winner that contested an owned object is counted exactly once.
    assert_eq!(
        setup
            .metrics
            .consensus_handler_double_spend_deferrals
            .get(),
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

/// Two transactions in the same commit compete for the same owned object.
/// The first to acquire the lock wins but gets deferred (penalty for being contested).
/// The second is dropped (failed lock).
/// Verifying: only the prologue transaction is scheduled in the first commit;
/// the winner is deferred and scheduled in the second commit.
#[tokio::test]
async fn test_double_spend_winner_deferred() {
    telemetry_subscribers::init_for_testing();

    let mut setup =
        DoubleSpendTestSetup::new(protocol_config_with_double_spend_deferral(), 2).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    // Round 1: both transactions in the same commit.
    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // Only the prologue should be scheduled (winner deferred, loser dropped).
    assert_eq!(count, 1, "Expected only the prologue in round 1");

    // Round 2: empty commit picks up the deferred transaction.
    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::empty(2, 100, 1))
        .await;

    // prologue + the previously deferred winner.
    assert_eq!(count, 2, "Expected prologue + deferred winner in round 2");
}

/// Same setup but with the feature flag disabled.
/// The winner should NOT be deferred - it should be scheduled immediately.
#[tokio::test]
async fn test_double_spend_no_deferral_when_disabled() {
    telemetry_subscribers::init_for_testing();

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.set_defer_owned_object_double_spend_for_testing(false);

    let mut setup = DoubleSpendTestSetup::new(protocol_config, 2).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // Without the feature, the winner is scheduled immediately.
    // prologue + winner = 2 (loser is still dropped due to lock conflict).
    assert_eq!(
        count, 2,
        "Expected prologue + winner when deferral is disabled"
    );
}

/// When max deferral rounds is 0 and the feature is enabled, the winner should NOT
/// be deferred (exceeds the deferral limit) and should be scheduled immediately.
#[tokio::test]
async fn test_double_spend_not_deferred_when_max_rounds_zero() {
    telemetry_subscribers::init_for_testing();

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.set_defer_owned_object_double_spend_for_testing(true);
    protocol_config.set_max_deferral_rounds_for_congestion_control_for_testing(0);

    let mut setup = DoubleSpendTestSetup::new(protocol_config, 2).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // With max_deferral_rounds=0, the deferral limit check fails so the winner
    // is scheduled immediately.
    assert_eq!(
        count, 2,
        "Expected prologue + winner (no deferral with max_rounds=0)"
    );
}

/// When only one transaction uses an owned object and there's no contention,
/// no deferral should happen even with the feature enabled.
#[tokio::test]
async fn test_no_deferral_without_contention() {
    telemetry_subscribers::init_for_testing();

    let mut setup =
        DoubleSpendTestSetup::new(protocol_config_with_double_spend_deferral(), 1).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // prologue + the single transaction (no contention, no deferral).
    assert_eq!(count, 2, "Expected prologue + user tx (no contention)");
}

/// Three transactions compete for the same owned object. The first wins and gets
/// deferred. The other two are dropped. In round 2, the deferred winner is scheduled.
#[tokio::test]
async fn test_double_spend_multiple_contestants() {
    telemetry_subscribers::init_for_testing();

    let mut setup =
        DoubleSpendTestSetup::new(protocol_config_with_double_spend_deferral(), 3).await;
    let consensus_txns = setup.build_competing_consensus_txns().await;

    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::new(consensus_txns, 1, 0, 0))
        .await;

    // Only the prologue (winner deferred, two losers dropped).
    assert_eq!(count, 1, "Expected only prologue with 3 contestants");

    // Round 2: the deferred winner should now be scheduled.
    let count = setup
        .submit_commit_and_count_scheduled(TestConsensusCommit::empty(2, 100, 1))
        .await;

    assert_eq!(count, 2, "Expected prologue + deferred winner in round 2");
}
