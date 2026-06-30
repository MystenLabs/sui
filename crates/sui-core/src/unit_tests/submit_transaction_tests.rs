// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use consensus_core::BlockStatus;
use consensus_types::block::{BlockRef, PING_TRANSACTION_INDEX};
use fastcrypto::traits::KeyPair;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectRef, SuiAddress, random_object_ref};
use sui_types::crypto::{AccountKeyPair, get_account_key_pair};
use sui_types::effects::TransactionEffectsAPI as _;
use sui_types::error::{SuiError, SuiErrorKind, UserInputError};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::message_envelope::Message as _;
use sui_types::messages_consensus::{ConsensusPosition, ConsensusTransaction};
use sui_types::messages_grpc::{
    RawSubmitTxRequest, SubmitTxRequest, SubmitTxResponse, SubmitTxResult, SubmitTxType,
};
use sui_types::object::Object;
use sui_types::transaction::{
    Transaction, TransactionDataAPI, TransactionExpiration, VerifiedTransaction,
};
use sui_types::utils::to_sender_signed_transaction;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::{AuthorityState, ExecutionEnv};
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use crate::authority_server::AuthorityServer;
use crate::consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics, ConsensusClient};
use crate::consensus_test_utils::make_consensus_adapter_for_test;
use crate::mock_consensus::with_block_status;

use super::AuthorityServerHandle;

struct TestContext {
    state: Arc<AuthorityState>,
    _server_handle: AuthorityServerHandle,
    client: NetworkAuthorityClient,
    sender: SuiAddress,
    keypair: AccountKeyPair,
    gas_object_ref: ObjectRef,
}

impl TestContext {
    async fn new() -> Self {
        // Default: transactions execute, blocks immediately sequenced; extra responses cover
        // tests that submit multiple transactions.
        Self::new_with_consensus(
            true,
            vec![
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
            ],
        )
        .await
    }

    async fn new_with_consensus(
        execute: bool,
        block_status_receivers: Vec<crate::consensus_adapter::BlockStatusReceiver>,
    ) -> Self {
        Self::new_with_adapter(|authority| {
            make_consensus_adapter_for_test(
                authority,
                HashSet::new(),
                execute,
                block_status_receivers,
            )
        })
        .await
    }

    async fn new_with_consensus_client(consensus_client: Arc<dyn ConsensusClient>) -> Self {
        Self::new_with_adapter(|authority| {
            Arc::new(ConsensusAdapter::new(
                consensus_client,
                authority.checkpoint_store.clone(),
                authority.name,
                100_000,
                100_000,
                ConsensusAdapterMetrics::new_test(),
                Arc::new(tokio::sync::Notify::new()),
            ))
        })
        .await
    }

    async fn new_with_adapter(
        make_adapter: impl FnOnce(Arc<AuthorityState>) -> Arc<ConsensusAdapter>,
    ) -> Self {
        telemetry_subscribers::init_for_testing();
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_ref = gas_object.compute_object_reference();
        let authority = TestAuthorityBuilder::new()
            .with_starting_objects(&[gas_object])
            .build()
            .await;

        let adapter = make_adapter(authority.clone());
        let server =
            AuthorityServer::new_for_test_with_consensus_adapter(authority.clone(), adapter);
        let server_handle = server.spawn_for_test().await.unwrap();
        let client = NetworkAuthorityClient::connect(
            server_handle.address(),
            authority.config.network_key_pair().public().to_owned(),
        )
        .await
        .unwrap();

        Self {
            state: authority,
            _server_handle: server_handle,
            client,
            sender,
            keypair,
            gas_object_ref,
        }
    }

    fn build_test_transaction(&self) -> Transaction {
        let tx_data = TestTransactionBuilder::new(
            self.sender,
            self.gas_object_ref,
            self.state.reference_gas_price_for_testing().unwrap(),
        )
        .transfer_sui(None, self.sender)
        .build();
        to_sender_signed_transaction(tx_data, &self.keypair)
    }

    fn build_submit_request(&self, transaction: Transaction) -> SubmitTxRequest {
        SubmitTxRequest {
            transaction: Some(transaction),
            ping_type: None,
        }
    }
}

struct BlockingConsensusClient {
    first_submit_seen: tokio::sync::watch::Sender<bool>,
    release_first_submit: tokio::sync::watch::Sender<bool>,
    submit_count: Arc<AtomicUsize>,
}

impl BlockingConsensusClient {
    fn new() -> Arc<Self> {
        let (first_submit_seen, _) = tokio::sync::watch::channel(false);
        let (release_first_submit, _) = tokio::sync::watch::channel(false);
        Arc::new(Self {
            first_submit_seen,
            release_first_submit,
            submit_count: Arc::new(AtomicUsize::new(0)),
        })
    }

    fn subscribe_first_submit(&self) -> tokio::sync::watch::Receiver<bool> {
        self.first_submit_seen.subscribe()
    }

    fn release_first_submit(&self) {
        let _ = self.release_first_submit.send(true);
    }

    fn submit_count(&self) -> usize {
        self.submit_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ConsensusClient for BlockingConsensusClient {
    async fn submit(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> sui_types::error::SuiResult<(
        Vec<ConsensusPosition>,
        crate::consensus_adapter::BlockStatusReceiver,
    )> {
        let submit_index = self.submit_count.fetch_add(1, Ordering::SeqCst);
        if submit_index == 0 {
            let _ = self.first_submit_seen.send(true);
            let mut release_first_submit = self.release_first_submit.subscribe();
            while !*release_first_submit.borrow() {
                release_first_submit
                    .changed()
                    .await
                    .map_err(|_| SuiError::from("blocking consensus release channel closed"))?;
            }
        }

        let consensus_positions = transactions
            .iter()
            .enumerate()
            .map(|(index, _)| ConsensusPosition {
                epoch: epoch_store.epoch(),
                block: BlockRef::MIN,
                index: index as u16,
            })
            .collect();

        Ok((
            consensus_positions,
            with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
        ))
    }
}

#[tokio::test]
async fn test_submit_transaction_success() {
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let request = test_context.build_submit_request(transaction);

    let response = test_context
        .client
        .submit_transaction(request, None)
        .await
        .unwrap();

    // Verify we got a consensus position back
    assert_eq!(response.results.len(), 1);
    match &response.results[0] {
        SubmitTxResult::Submitted { consensus_position } => {
            assert_eq!(consensus_position.index, 0);
        }
        _ => panic!("Expected Submitted response"),
    };
}

#[tokio::test]
async fn test_duplicate_submission_suppressed_within_window() {
    // `execute = false` so the duplicate reaches the recent-submission check rather than being
    // short-circuited by the executed-effects path.
    let test_context = TestContext::new_with_consensus(
        false,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
    )
    .await;

    let transaction = test_context.build_test_transaction();

    // First submission is accepted.
    let first = test_context
        .client
        .submit_transaction(test_context.build_submit_request(transaction.clone()), None)
        .await
        .unwrap();
    assert!(
        matches!(first.results[0], SubmitTxResult::Submitted { .. }),
        "first submission should be accepted, got {:?}",
        first.results[0]
    );

    // Resubmitting the same transaction within the window is suppressed.
    let second = test_context
        .client
        .submit_transaction(test_context.build_submit_request(transaction), None)
        .await
        .unwrap();
    match &second.results[0] {
        SubmitTxResult::Rejected { error } => match error.clone().into_inner() {
            SuiErrorKind::TransactionProcessing { status, .. } => assert!(
                status.contains("recently processed"),
                "unexpected rejection status: {status}"
            ),
            other => panic!("unexpected rejection error kind: {other:?}"),
        },
        other => panic!("expected duplicate submission to be rejected, got {other:?}"),
    }
}

#[tokio::test]
async fn test_concurrent_duplicate_submission_rejected_as_inflight() {
    let consensus_client = BlockingConsensusClient::new();
    let mut first_submit_seen = consensus_client.subscribe_first_submit();
    let test_context = TestContext::new_with_consensus_client(consensus_client.clone()).await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();

    let first_client = test_context.client.clone();
    let first_request = test_context.build_submit_request(transaction.clone());
    let first_submit =
        tokio::spawn(async move { first_client.submit_transaction(first_request, None).await });

    tokio::time::timeout(Duration::from_secs(10), async {
        while !*first_submit_seen.borrow() {
            first_submit_seen
                .changed()
                .await
                .expect("blocking consensus client should remain alive");
        }
    })
    .await
    .expect("first submission should reach consensus client");
    assert_eq!(consensus_client.submit_count(), 1);

    let second = test_context
        .client
        .submit_transaction(test_context.build_submit_request(transaction), None)
        .await
        .unwrap();

    assert_eq!(second.results.len(), 1);
    match &second.results[0] {
        SubmitTxResult::Rejected { error } => match error.as_inner() {
            SuiErrorKind::TransactionProcessing { digest, status } => {
                assert_eq!(*digest, tx_digest);
                assert!(
                    status.contains("submission in progress"),
                    "unexpected rejection status: {status}"
                );
            }
            other => panic!("unexpected rejection error kind: {other:?}"),
        },
        other => panic!("expected concurrent duplicate to be rejected, got {other:?}"),
    }
    assert_eq!(
        consensus_client.submit_count(),
        1,
        "concurrent duplicate should be rejected before consensus submission"
    );

    consensus_client.release_first_submit();
    let first = first_submit.await.unwrap().unwrap();
    assert_eq!(first.results.len(), 1);
    assert!(
        matches!(first.results[0], SubmitTxResult::Submitted { .. }),
        "first submission should complete after release, got {:?}",
        first.results[0]
    );
}

#[tokio::test]
async fn test_submit_ping_request() {
    let test_context = TestContext::new().await;

    println!("Case 1. Ping request cannot contain transactions.");
    {
        let request = RawSubmitTxRequest {
            transactions: vec![vec![0xFF, 0xFF, 0xFF].into()],
            submit_type: SubmitTxType::Ping.into(),
        };

        let response = test_context
            .client
            .client()
            .unwrap()
            .submit_transaction(request)
            .await;
        assert!(response.is_err());
        let error: SuiError = response.unwrap_err().into();
        assert!(matches!(
            error.into_inner(),
            SuiErrorKind::InvalidRequest { .. }
        ));
    }

    println!("Case 2. Valid ping request.");
    {
        // Submit an empty array of transactions.
        // The request should explicitly set type to `ping` to indicate a ping check.
        let request = RawSubmitTxRequest {
            transactions: vec![],
            submit_type: SubmitTxType::Ping.into(),
        };

        let response = test_context
            .client
            .client()
            .unwrap()
            .submit_transaction(request)
            .await
            .unwrap();

        // Verify we got a consensus position back
        let response: SubmitTxResponse = response.into_inner().try_into().unwrap();
        assert_eq!(response.results.len(), 1);
        match &response.results[0] {
            SubmitTxResult::Submitted { consensus_position } => {
                assert_eq!(consensus_position.index, PING_TRANSACTION_INDEX);
                assert_eq!(consensus_position.block, BlockRef::MIN);
            }
            _ => panic!("Expected Submitted response"),
        };
    }
}

#[tokio::test]
async fn test_submit_transaction_invalid_transaction() {
    let test_context = TestContext::new().await;

    // Create an invalid request with malformed transaction bytes
    let request = RawSubmitTxRequest {
        transactions: vec![vec![0xFF, 0xFF, 0xFF].into()],
        ..Default::default()
    };

    // Submit request with GRPC client directly.
    let response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await;

    assert!(response.is_err());
}

// test transaction submission after already executed.
#[tokio::test]
async fn test_submit_transaction_already_executed() {
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let request = test_context.build_submit_request(transaction.clone());

    let epoch_store = test_context.state.epoch_store_for_testing();
    let verified_transaction = VerifiedExecutableTransaction::new_from_checkpoint(
        VerifiedTransaction::new_unchecked(transaction),
        epoch_store.epoch(),
        1,
    );
    test_context
        .state
        .try_execute_immediately(&verified_transaction, ExecutionEnv::new(), &epoch_store)
        .unwrap();

    // Submit the same transaction that has already been executed.
    let response2 = test_context
        .client
        .submit_transaction(request, None)
        .await
        .unwrap();
    // Verify we got the full effects back.
    assert_eq!(response2.results.len(), 1);
    match &response2.results[0] {
        SubmitTxResult::Executed {
            effects_digest,
            details,
        } => {
            let details = details.as_ref().unwrap();
            assert_eq!(*effects_digest, details.effects.digest());
            assert_eq!(
                verified_transaction.digest(),
                details.effects.transaction_digest()
            );
        }
        _ => panic!("Expected Executed response"),
    };
}

// Test that a transaction already processed by consensus this epoch but not yet executed
// (e.g. deferred) is suppressed rather than resubmitted to consensus.
#[tokio::test]
async fn test_submit_transaction_consensus_message_processed() {
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let request = test_context.build_submit_request(transaction);

    // Mark the transaction as processed by consensus without executing it, simulating a
    // sequenced-but-deferred transaction. Its gas object is still unspent, so without the
    // is_consensus_message_processed check it would be resubmitted to consensus.
    let epoch_store = test_context.state.epoch_store_for_testing();
    epoch_store.test_insert_user_signature(tx_digest, vec![]);

    let response = test_context
        .client
        .submit_transaction(request, None)
        .await
        .unwrap();
    assert_eq!(response.results.len(), 1);
    match &response.results[0] {
        SubmitTxResult::Rejected { error } => {
            assert!(
                matches!(
                    error.as_inner(),
                    SuiErrorKind::TransactionProcessing { digest, .. } if *digest == tx_digest
                ),
                "unexpected rejection error: {error}"
            );
        }
        other => panic!("Expected Rejected response, got {other:?}"),
    };
}

#[tokio::test]
async fn test_submit_transaction_wrong_epoch() {
    let test_context = TestContext::new().await;
    test_context.state.reconfigure_for_testing().await;

    // Build a transaction with wrong epoch
    let tx_data = TestTransactionBuilder::new(
        test_context.sender,
        test_context.gas_object_ref,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();

    // Manually set wrong epoch
    let mut tx_data = tx_data;
    *tx_data.expiration_mut_for_testing() = TransactionExpiration::Epoch(0);

    let transaction = to_sender_signed_transaction(tx_data, &test_context.keypair);
    let request = test_context.build_submit_request(transaction);

    let response = test_context.client.submit_transaction(request, None).await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_submit_transaction_signature_verification_failure() {
    let test_context = TestContext::new().await;

    let tx_data = TestTransactionBuilder::new(
        test_context.sender,
        test_context.gas_object_ref,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();

    // Sign with a different keypair to cause signature verification failure
    let (_, wrong_keypair) = get_account_key_pair();
    let transaction = to_sender_signed_transaction(tx_data, &wrong_keypair);
    let request = test_context.build_submit_request(transaction);

    let response = test_context.client.submit_transaction(request, None).await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_submit_transaction_gas_object_validation() {
    let test_context = TestContext::new().await;

    // Build a transaction with an invalid gas object reference
    let invalid_gas_ref = random_object_ref();
    let tx_data = TestTransactionBuilder::new(
        test_context.sender,
        invalid_gas_ref,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();

    let transaction = to_sender_signed_transaction(tx_data, &test_context.keypair);
    let request = test_context.build_submit_request(transaction);

    // Because the error comes from validating transaction input, the response should contain SubmitTxResult
    // with the Rejected variant.
    let response = test_context.client.submit_transaction(request, None).await;
    let result: SubmitTxResult = response.unwrap().results.first().unwrap().clone();
    assert!(
        matches!(result, SubmitTxResult::Rejected { error } if matches!(error.as_inner(), SuiErrorKind::UserInputError {
                        error: UserInputError::ObjectNotFound { .. }
        }))
    );
}

#[tokio::test]
async fn test_submit_batched_transactions() {
    let test_context = TestContext::new().await;

    let tx1 = test_context.build_test_transaction();

    // Build a distinct, non-conflicting second transaction from its own gas object so both are
    // submitted (identical transactions would be deduped as duplicate resubmissions).
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);
    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    // Build request with batched transactions.
    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        ..Default::default()
    };

    // Submit request with batched transactions, using grpc client directly.
    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    // Verify we got results for both transactions
    assert_eq!(raw_response.results.len(), 2);

    // Both should be submitted to consensus
    for result in raw_response.results {
        match result.inner {
            Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_)) => {
                // Expected: transactions were submitted to consensus
            }
            _ => panic!("Expected Submitted status for all transactions"),
        }
    }
}

#[tokio::test]
async fn test_submit_batched_transactions_with_repeated_transaction() {
    let test_context = TestContext::new().await;

    let tx = test_context.build_test_transaction();
    let tx_digest = *tx.digest();

    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx).unwrap().into(),
            bcs::to_bytes(&tx).unwrap().into(),
        ],
        ..Default::default()
    };

    let response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await;
    assert!(response.is_err());
    let error: SuiError = response.unwrap_err().into();
    assert!(
        matches!(
            error.into_inner(),
            SuiErrorKind::UserInputError {
                error: UserInputError::RepeatedTransactions { digest }
            } if digest == tx_digest
        ),
        "expected RepeatedTransactions error"
    );
}

#[tokio::test]
async fn test_submit_batched_transactions_with_already_executed() {
    let test_context = TestContext::new().await;

    // Create 1st transaction and execute it
    let tx1 = test_context.build_test_transaction();
    let epoch_store = test_context.state.epoch_store_for_testing();
    let verified_tx1 = VerifiedExecutableTransaction::new_from_checkpoint(
        VerifiedTransaction::new_unchecked(tx1.clone()),
        epoch_store.epoch(),
        1,
    );
    test_context
        .state
        .try_execute_immediately(&verified_tx1, ExecutionEnv::new(), &epoch_store)
        .unwrap();

    // Create 2nd transaction (not executed)
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);

    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    // Build request with both transactions
    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        ..Default::default()
    };

    // Submit both transactions, using grpc client directly.
    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    // Verify we got results for both transactions
    assert_eq!(raw_response.results.len(), 2);

    // First should be already executed, second should be submitted
    match &raw_response.results[0].inner {
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Executed(_)) => {
            // Expected: first transaction was already executed
        }
        _ => panic!("Expected Executed status for first transaction"),
    }

    match &raw_response.results[1].inner {
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_)) => {
            // Expected: second transaction was submitted to consensus
        }
        _ => panic!("Expected Submitted status for second transaction"),
    }
}

// A batch containing a transaction that is already being processed by consensus must NOT fail the
// whole request. The already-processing transaction is reported per-tx as
// `Rejected { TransactionProcessing }` (a retriable rejection), while the rest of the batch still
// returns its consensus position.
#[tokio::test]
async fn test_submit_batched_transactions_with_already_processing() {
    let test_context = TestContext::new().await;
    let epoch_store = test_context.state.epoch_store_for_testing();

    // tx1: mark as already sequenced by consensus so the validator must not resubmit it.
    let tx1 = test_context.build_test_transaction();
    epoch_store.test_insert_user_signature(*tx1.digest(), vec![]);

    // tx2: a distinct, fresh transaction that should be submitted normally.
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);
    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        ..Default::default()
    };

    // The whole RPC must still succeed (no top-level error).
    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(raw_response.results.len(), 2);

    let result0: SubmitTxResult = raw_response.results[0].clone().try_into().unwrap();
    let result1: SubmitTxResult = raw_response.results[1].clone().try_into().unwrap();

    // tx1 is rejected specifically because it is already being processed by consensus.
    match result0 {
        SubmitTxResult::Rejected { error } => assert!(
            matches!(
                error.as_inner(),
                SuiErrorKind::TransactionProcessing { digest, .. } if *digest == *tx1.digest()
            ),
            "expected TransactionProcessing for tx1, got: {error}"
        ),
        other => {
            panic!("Expected Rejected status for already-processing transaction, got: {other:?}")
        }
    }

    // tx2 is still submitted to consensus.
    match result1 {
        SubmitTxResult::Submitted { .. } => {}
        other => panic!("Expected Submitted status for fresh transaction, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_submit_soft_bundle_transactions() {
    let test_context = TestContext::new().await;

    let tx1 = test_context.build_test_transaction();

    // tx2 must be distinct from tx1: a soft bundle may not repeat a transaction.
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);
    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    // Build request with batched transactions.
    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        submit_type: SubmitTxType::SoftBundle.into(),
    };

    // Submit request with batched transactions, using grpc client directly.
    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    // Verify we got results for both transactions
    assert_eq!(raw_response.results.len(), 2);

    // Both should be submitted to consensus
    for result in raw_response.results {
        match result.inner {
            Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_)) => {
                // Expected: transactions were submitted to consensus
            }
            _ => panic!("Expected Submitted status for all transactions"),
        }
    }
}

// A soft bundle that repeats the same transaction is rejected outright.
#[tokio::test]
async fn test_submit_soft_bundle_with_repeated_transaction() {
    let test_context = TestContext::new().await;

    let tx = test_context.build_test_transaction();
    let tx_digest = *tx.digest();

    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx).unwrap().into(),
            bcs::to_bytes(&tx).unwrap().into(),
        ],
        submit_type: SubmitTxType::SoftBundle.into(),
    };

    let response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await;
    assert!(response.is_err());
    let error: SuiError = response.unwrap_err().into();
    assert!(
        matches!(
            error.into_inner(),
            SuiErrorKind::UserInputError {
                error: UserInputError::RepeatedTransactions { digest }
            } if digest == tx_digest
        ),
        "expected RepeatedTransactions error"
    );
}

#[tokio::test]
async fn test_submit_soft_bundle_transactions_with_already_executed() {
    let test_context = TestContext::new().await;

    // Create 1st transaction and execute it
    let tx1 = test_context.build_test_transaction();
    let epoch_store = test_context.state.epoch_store_for_testing();
    let verified_tx1 = VerifiedExecutableTransaction::new_from_checkpoint(
        VerifiedTransaction::new_unchecked(tx1.clone()),
        epoch_store.epoch(),
        1,
    );
    test_context
        .state
        .try_execute_immediately(&verified_tx1, ExecutionEnv::new(), &epoch_store)
        .unwrap();

    // Create 2nd transaction (not executed)
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);

    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    // Build request with both transactions
    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        submit_type: SubmitTxType::SoftBundle.into(),
    };

    // Submit request with batched transactions, using grpc client directly.
    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    // First should be already executed, second should be submitted
    match &raw_response.results[0].inner {
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Executed(_)) => {
            // Expected: first transaction was already executed
        }
        _ => panic!("Expected Executed status for first transaction"),
    }

    match &raw_response.results[1].inner {
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_)) => {
            // Expected: second transaction was submitted to consensus
        }
        _ => panic!("Expected Submitted status for second transaction"),
    }
}

// Test that a transaction already processed by consensus (but not executed) is removed from a
// soft bundle before submission, while the remaining transactions in the bundle are still
// submitted to consensus.
#[tokio::test]
async fn test_submit_soft_bundle_transactions_with_consensus_message_processed() {
    let test_context = TestContext::new().await;

    // 1st transaction: mark as already processed by consensus without executing it.
    let tx1 = test_context.build_test_transaction();
    let tx1_digest = *tx1.digest();
    let epoch_store = test_context.state.epoch_store_for_testing();
    epoch_store.test_insert_user_signature(tx1_digest, vec![]);

    // 2nd transaction: a fresh, unprocessed transaction with its own gas object.
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);

    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        submit_type: SubmitTxType::SoftBundle.into(),
    };

    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(raw_response.results.len(), 2);

    // The already-processed transaction is rejected and excluded from the bundle submitted to
    // consensus; the remaining transaction is still submitted.
    match &raw_response.results[0].inner {
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Rejected(_)) => {}
        other => {
            panic!("Expected Rejected status for already-processed transaction, got {other:?}")
        }
    }
    match &raw_response.results[1].inner {
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_)) => {}
        other => panic!("Expected Submitted status for remaining transaction, got {other:?}"),
    }
}

#[tokio::test]
async fn test_submit_oversized_transaction() {
    use sui_types::base_types::dbg_addr;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::TransactionData;

    let test_context = TestContext::new().await;

    let max_txn_size = test_context
        .state
        .epoch_store_for_testing()
        .protocol_config()
        .max_tx_size_bytes() as usize;

    // Get the gas object to use for the transaction
    let gas_object = test_context
        .state
        .get_object(&test_context.gas_object_ref.0)
        .unwrap();
    let full_object_ref = gas_object.compute_full_object_reference();
    let recipient = dbg_addr(2);

    // Construct an oversized transaction by putting lots of commands in it
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        // Put a lot of commands in the txn so it's large
        for _ in 0..(1024 * 16) {
            builder.transfer_object(recipient, full_object_ref).unwrap();
        }
        builder.finish()
    };

    let txn_data = TransactionData::new_programmable(
        test_context.sender,
        vec![test_context.gas_object_ref],
        pt,
        0,
        0,
    );

    let txn = to_sender_signed_transaction(txn_data, &test_context.keypair);
    let tx_size = bcs::serialized_size(&txn).unwrap();

    // Making sure the txn is larger than the max txn size
    assert!(tx_size > max_txn_size);

    let request = test_context.build_submit_request(txn);
    let response = test_context.client.submit_transaction(request, None).await;

    // The txn should be rejected due to its size
    assert!(response.is_err());
    let error_str = response.unwrap_err().to_string();
    assert!(
        error_str.contains("serialized transaction size exceeded maximum"),
        "Expected size limit error but got: {error_str}"
    );
}

// A batch containing a transaction that was already submitted (and is now in the recent-submission
// window) rejects only that index with `TransactionProcessing { status: "recently processed" }`,
// while a fresh transaction in the same batch is still submitted.
#[tokio::test]
async fn test_batch_with_recently_processed_duplicate_rejects_only_that_index() {
    // `execute = false` so the prior submission is suppressed via the recent-submission window
    // rather than short-circuited by the executed-effects path.
    let test_context = TestContext::new_with_consensus(
        false,
        vec![
            with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
            with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
        ],
    )
    .await;

    // tx1: submit it once so it is demoted into the recent-submission cache when the handler returns.
    let tx1 = test_context.build_test_transaction();
    let first = test_context
        .client
        .submit_transaction(test_context.build_submit_request(tx1.clone()), None)
        .await
        .unwrap();
    assert!(
        matches!(first.results[0], SubmitTxResult::Submitted { .. }),
        "first submission of tx1 should be accepted, got {:?}",
        first.results[0]
    );

    // tx2: a distinct, fresh transaction from its own gas object.
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);
    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    // Batch [tx1, tx2]: tx1 is a recent duplicate, tx2 is fresh.
    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        ..Default::default()
    };

    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(raw_response.results.len(), 2);
    let result0: SubmitTxResult = raw_response.results[0].clone().try_into().unwrap();
    let result1: SubmitTxResult = raw_response.results[1].clone().try_into().unwrap();

    // tx1 rejected specifically as a recent duplicate.
    match result0 {
        SubmitTxResult::Rejected { error } => match error.as_inner() {
            SuiErrorKind::TransactionProcessing { digest, status } => {
                assert_eq!(*digest, *tx1.digest());
                assert!(
                    status.contains("recently processed"),
                    "unexpected rejection status: {status}"
                );
            }
            other => panic!("unexpected rejection error kind: {other:?}"),
        },
        other => panic!("expected tx1 to be rejected as recent duplicate, got: {other:?}"),
    }

    // tx2 still submitted to consensus.
    match result1 {
        SubmitTxResult::Submitted { .. } => {}
        other => panic!("expected tx2 to be submitted, got: {other:?}"),
    }
}

// Same per-index dedup inside an atomic soft bundle: a previously-submitted member is rejected with
// "recently processed" while the fresh member is still submitted. Confirms the dedup is
// req_type-agnostic (the per-tx loop sets `results[idx]` regardless of submit type).
#[tokio::test]
async fn test_soft_bundle_with_recently_processed_duplicate_rejects_only_that_index() {
    let test_context = TestContext::new_with_consensus(
        false,
        vec![
            with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
            with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
        ],
    )
    .await;

    // tx1: submit once so it is demoted into the recent-submission cache on handler return.
    let tx1 = test_context.build_test_transaction();
    let first = test_context
        .client
        .submit_transaction(test_context.build_submit_request(tx1.clone()), None)
        .await
        .unwrap();
    assert!(
        matches!(first.results[0], SubmitTxResult::Submitted { .. }),
        "first submission of tx1 should be accepted, got {:?}",
        first.results[0]
    );

    // tx2: distinct, fresh, same gas price (soft bundle requires a single shared gas price).
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2);
    let tx_data2 = TestTransactionBuilder::new(
        test_context.sender,
        gas_object_ref2,
        test_context
            .state
            .reference_gas_price_for_testing()
            .unwrap(),
    )
    .transfer_sui(None, test_context.sender)
    .build();
    let tx2 = to_sender_signed_transaction(tx_data2, &test_context.keypair);

    // Soft bundle [tx1, tx2]: tx1 is a recent duplicate, tx2 is fresh.
    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&tx1).unwrap().into(),
            bcs::to_bytes(&tx2).unwrap().into(),
        ],
        submit_type: SubmitTxType::SoftBundle.into(),
    };

    let raw_response = test_context
        .client
        .client()
        .unwrap()
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(raw_response.results.len(), 2);
    let result0: SubmitTxResult = raw_response.results[0].clone().try_into().unwrap();
    let result1: SubmitTxResult = raw_response.results[1].clone().try_into().unwrap();

    match result0 {
        SubmitTxResult::Rejected { error } => match error.as_inner() {
            SuiErrorKind::TransactionProcessing { digest, status } => {
                assert_eq!(*digest, *tx1.digest());
                assert!(
                    status.contains("recently processed"),
                    "unexpected rejection status: {status}"
                );
            }
            other => panic!("unexpected rejection error kind: {other:?}"),
        },
        other => panic!("expected tx1 to be rejected as recent duplicate, got: {other:?}"),
    }

    match result1 {
        SubmitTxResult::Submitted { .. } => {}
        other => panic!("expected tx2 to be submitted, got: {other:?}"),
    }
}
