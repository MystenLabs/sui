// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

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
use sui_types::messages_grpc::{
    RawSubmitTxRequest, SubmitTxRequest, SubmitTxResponse, SubmitTxResult, SubmitTxType,
};
use sui_types::object::Object;
use sui_types::transaction::{
    Transaction, TransactionDataAPI, TransactionExpiration, VerifiedTransaction,
};
use sui_types::utils::to_sender_signed_transaction;

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::{AuthorityState, ExecutionEnv};
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use crate::authority_server::AuthorityServer;
use crate::consensus_test_utils::make_consensus_adapter_for_test;
use crate::execution_scheduler::SchedulingSource;
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
        telemetry_subscribers::init_for_testing();
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_ref = gas_object.compute_object_reference();
        let authority = TestAuthorityBuilder::new()
            .with_starting_objects(&[gas_object])
            .build()
            .await;

        // Create a server with mocked consensus.
        // This ensures transactions submitted to consensus will get processed.
        // We add extra mock responses to handle multiple transactions in tests
        let adapter = make_consensus_adapter_for_test(
            authority.clone(),
            HashSet::new(),
            true,
            vec![
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
            ],
        );
        let server =
            AuthorityServer::new_for_test_with_consensus_adapter(authority.clone(), adapter);
        let _metrics = server.metrics.clone();
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
        .try_execute_immediately(
            &verified_transaction,
            // Fastpath execution will only put outputs in a temporary cache,
            // and the object changes in this transaction are not yet committed.
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
            &epoch_store,
        )
        .await
        .unwrap();

    // Submit the same transaction that has already been fastpath executed.
    let response1 = test_context
        .client
        .submit_transaction(request.clone(), None)
        .await
        .unwrap();

    // Verify we still got a consensus position back, because the transaction has not been committed yet,
    // so we can still sign the same transaction.
    assert_eq!(response1.results.len(), 1);
    match &response1.results[0] {
        SubmitTxResult::Submitted { consensus_position } => {
            assert_eq!(consensus_position.index, 0);
        }
        _ => panic!("Expected Submitted response"),
    };

    // Execute it again through non-fastpath, which will commit the object changes.
    test_context
        .state
        .try_execute_immediately(
            &verified_transaction,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            &epoch_store,
        )
        .await
        .unwrap();

    // Submit the same transaction again.
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
            fast_path: _,
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
    let tx2 = test_context.build_test_transaction();

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
        .try_execute_immediately(
            &verified_tx1,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            &epoch_store,
        )
        .await
        .unwrap();

    // Create 2nd transaction (not executed)
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2).await;

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

#[tokio::test]
async fn test_submit_soft_bundle_transactions() {
    let test_context = TestContext::new().await;

    let tx1 = test_context.build_test_transaction();
    let tx2 = test_context.build_test_transaction();

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
        .try_execute_immediately(
            &verified_tx1,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            &epoch_store,
        )
        .await
        .unwrap();

    // Create 2nd transaction (not executed)
    let gas_object2 = Object::with_owner_for_testing(test_context.sender);
    let gas_object_ref2 = gas_object2.compute_object_reference();
    test_context.state.insert_genesis_object(gas_object2).await;

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
