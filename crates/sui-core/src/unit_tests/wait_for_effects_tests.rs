// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use consensus_types::block::{BlockRef, TransactionIndex};
use fastcrypto::traits::KeyPair;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectRef, SuiAddress, TransactionDigest};
use sui_types::committee::EpochId;
use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::message_envelope::Message;
use sui_types::messages_consensus::ConsensusPosition;
use sui_types::messages_grpc::RawWaitForEffectsRequest;
use sui_types::object::Object;
use sui_types::transaction::VerifiedTransaction;
use sui_types::utils::to_sender_signed_transaction;

use crate::authority::consensus_tx_status_cache::{
    ConsensusTxStatus, CONSENSUS_STATUS_RETENTION_ROUNDS,
};
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::{AuthorityState, ExecutionEnv};
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use crate::authority_server::AuthorityServer;
use crate::execution_scheduler::SchedulingSource;
use crate::wait_for_effects_request::{
    RejectReason, WaitForEffectsRequest, WaitForEffectsResponse,
};

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
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_ref = gas_object.compute_object_reference();
        let state = TestAuthorityBuilder::new()
            .with_starting_objects(&[gas_object])
            .build()
            .await;
        let server_handle = AuthorityServer::new_for_test(state.clone())
            .spawn_for_test()
            .await
            .unwrap();
        let client = NetworkAuthorityClient::connect(
            server_handle.address(),
            state.config.network_key_pair().public().to_owned(),
        )
        .await
        .unwrap();

        Self {
            state,
            _server_handle: server_handle,
            client,
            sender,
            keypair,
            gas_object_ref,
        }
    }

    fn build_test_transaction(&self) -> VerifiedExecutableTransaction {
        let tx_data = TestTransactionBuilder::new(
            self.sender,
            self.gas_object_ref,
            self.state.reference_gas_price_for_testing().unwrap(),
        )
        .transfer_sui(None, self.sender)
        .build();
        let tx = to_sender_signed_transaction(tx_data, &self.keypair);
        VerifiedExecutableTransaction::new_from_checkpoint(
            VerifiedTransaction::new_unchecked(tx),
            self.state.epoch_store_for_testing().epoch(),
            1,
        )
    }
}

#[tokio::test]
async fn test_wait_for_effects_position_mismatch() {
    // This test exercise the path where if the position of the transaction
    // triggered the execution differs from the position in the request,
    // the request will timeout.
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position1 = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };
    let tx_position2 = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN + 1,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position1,
        include_details: true,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position2, ConsensusTxStatus::FastpathCertified);
        state_clone
            .try_execute_immediately(
                &transaction,
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
                &epoch_store,
            )
            .await
            .unwrap()
            .0
    });

    let response = test_context.client.wait_for_effects(request, None).await;

    assert!(response.is_err());
}

#[tokio::test]
async fn test_wait_for_effects_post_commit_rejected() {
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position,
        include_details: true,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::FastpathCertified);
        tokio::time::sleep(Duration::from_millis(100)).await;
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::Rejected);
    });

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    match response {
        WaitForEffectsResponse::Rejected { reason } => {
            // TODO(fastpath): Test reject reason.
            assert_eq!(reason, RejectReason::None);
        }
        _ => panic!("Expected Rejected response"),
    }
}

#[tokio::test]
async fn test_wait_for_effects_epoch_mismatch() {
    // This test exercises the path where the epoch of the request does not match the epoch
    // of the authority.
    let test_context = TestContext::new().await;

    let tx_digest = TransactionDigest::random();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position,
        include_details: true,
    })
    .unwrap();

    let response = test_context.client.wait_for_effects(request, None).await;

    assert!(response.is_err());
}

#[tokio::test]
async fn test_wait_for_effects_timeout() {
    // This test exercises the path where the transaction is never executed.
    // The request will timeout.
    let test_context = TestContext::new().await;

    let tx_digest = TransactionDigest::random();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position,
        include_details: true,
    })
    .unwrap();

    let response = test_context.client.wait_for_effects(request, None).await;

    assert!(response.is_err());
}

#[tokio::test]
async fn test_wait_for_effects_quorum_rejected() {
    // This test exercises the path where the transaction is rejected by a quorum in consensus.
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position,
        include_details: true,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::Rejected);
    });

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    match response {
        WaitForEffectsResponse::Rejected { reason } => {
            assert_eq!(reason, RejectReason::None);
        }
        _ => panic!("Expected Rejected response"),
    }
}

#[tokio::test]
async fn test_wait_for_effects_fastpath_certified() {
    // This test exercises the path where the transaction is first fastpath certified,
    // then executed right away.
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position,
        // Also test the case where details are not requested.
        include_details: false,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    let exec_handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::FastpathCertified);
        tokio::time::sleep(Duration::from_millis(100)).await;
        state_clone
            .try_execute_immediately(
                &transaction,
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
                &epoch_store,
            )
            .await
            .unwrap()
            .0
    });

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    let exec_effects = exec_handle.await.unwrap();
    match response {
        WaitForEffectsResponse::Executed {
            details,
            effects_digest,
        } => {
            assert!(details.is_none());
            assert_eq!(effects_digest, exec_effects.digest());
        }
        _ => panic!("Expected Executed response"),
    }
}

#[tokio::test]
async fn test_wait_for_effects_finalized() {
    telemetry_subscribers::init_for_testing();
    // This test exercises the path where the transaction is first fastpath certified,
    // then finalized, and then executed.
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position,
        // Also test the case where details are not requested.
        include_details: false,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    let exec_handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::FastpathCertified);
        tokio::time::sleep(Duration::from_millis(100)).await;
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::Finalized);
        tokio::time::sleep(Duration::from_millis(100)).await;
        state_clone
            .try_execute_immediately(
                &transaction,
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
                &epoch_store,
            )
            .await
            .unwrap()
            .0
    });

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    let exec_effects = exec_handle.await.unwrap();
    match response {
        WaitForEffectsResponse::Executed {
            details,
            effects_digest,
        } => {
            assert!(details.is_none());
            assert_eq!(effects_digest, exec_effects.digest());
        }
        _ => panic!("Expected Executed response"),
    }
}

#[tokio::test]
async fn test_wait_for_effects_expired() {
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: tx_digest,
        consensus_position: tx_position,
        include_details: true,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store
            .consensus_tx_status_cache
            .as_ref()
            .unwrap()
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 1)
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::Finalized);
        state_clone
            .try_execute_immediately(
                &transaction,
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
                &epoch_store,
            )
            .await
            .unwrap()
            .0
    });

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    assert!(matches!(response, WaitForEffectsResponse::Expired { .. }));
}
