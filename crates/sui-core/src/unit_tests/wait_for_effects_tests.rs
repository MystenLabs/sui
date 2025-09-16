// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use consensus_types::block::{BlockRef, TransactionIndex, PING_TRANSACTION_INDEX};
use fastcrypto::traits::KeyPair;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectRef, SuiAddress, TransactionDigest};
use sui_types::committee::EpochId;
use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
use sui_types::digests::TransactionEffectsDigest;
use sui_types::effects::TransactionEffectsAPI as _;
use sui_types::error::{SuiError, UserInputError};
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
use crate::transaction_driver::{PingType, WaitForEffectsRequest, WaitForEffectsResponse};

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

#[tokio::test(flavor = "current_thread", start_paused = true)]
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

    let state_clone = test_context.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position2, ConsensusTxStatus::FastpathCertified);
        state_clone
            .try_execute_immediately(
                &transaction,
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
                &epoch_store,
            )
            .await
            .unwrap()
            .0
    });

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position1),
        include_details: true,
        ping: None,
    })
    .unwrap();

    let response = test_context.client.wait_for_effects(request, None).await;

    assert!(response.is_err());
}

#[tokio::test]
async fn test_wait_for_effects_consensus_rejected_validator_accepted() {
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        include_details: true,
        ping: None,
    })
    .unwrap();

    // Validator does not reject the transaction, but it is rejected by the commit.
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
        WaitForEffectsResponse::Rejected { error } => {
            // TODO(fastpath): Test reject reason.
            assert!(error.is_none(), "{:?}", error);
        }
        _ => panic!("Expected Rejected response"),
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
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
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        include_details: true,
        ping: None,
    })
    .unwrap();

    let response = test_context.client.wait_for_effects(request, None).await;

    assert!(response.is_err());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
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
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        include_details: true,
        ping: None,
    })
    .unwrap();

    let response = test_context.client.wait_for_effects(request, None).await;

    assert!(response.is_err());
}

#[tokio::test]
async fn test_wait_for_effects_consensus_rejected_validator_rejected() {
    // This test exercises the path where the transaction is rejected by both consensus and the validator.
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        include_details: true,
        ping: None,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::Rejected);
        epoch_store.set_rejection_vote_reason(
            tx_position,
            &SuiError::UserInputError {
                error: UserInputError::TransactionDenied {
                    error: "object denied".to_string(),
                },
            },
        );
    });

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    match response {
        WaitForEffectsResponse::Rejected { error } => {
            assert_eq!(
                error,
                Some(SuiError::UserInputError {
                    error: UserInputError::TransactionDenied {
                        error: "object denied".to_string(),
                    },
                })
            );
        }
        _ => panic!("Expected Rejected response"),
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_wait_for_effects_fastpath_certified_only() {
    // This test exercises the path where the transaction is only fastpath certified.
    // Tests three scenarios:
    // 1. With consensus position and no details - should succeed
    // 2. With consensus position and details - should succeed with fastpath outputs
    // 3. Without consensus position - should timeout
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

    let state_clone = test_context.state.clone();
    let exec_handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::FastpathCertified);
        state_clone
            .try_execute_immediately(
                &transaction,
                ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
                &epoch_store,
            )
            .await
            .unwrap()
            .0
    });

    // -------- First, test getting effects acknowledgement with consensus position. --------

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        // Also test the case where details are not requested.
        include_details: false,
        ping: None,
    })
    .unwrap();

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
            fast_path: _,
        } => {
            assert!(details.is_none());
            assert_eq!(effects_digest, exec_effects.digest());
        }
        _ => panic!("Expected Executed response"),
    }

    // -------- Then, test getting effects with details when consensus position is provided. --------

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        include_details: true,
        ping: None,
    })
    .unwrap();

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    match response {
        WaitForEffectsResponse::Executed {
            details,
            effects_digest,
            fast_path: _,
        } => {
            assert!(details.is_some());
            assert_eq!(effects_digest, exec_effects.digest());
        }
        _ => panic!("Expected Executed response"),
    }

    // -------- Finally, test getting effects acknowledgement without consensus position. --------

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: None,
        include_details: true,
        ping: None,
    })
    .unwrap();

    let response = test_context.client.wait_for_effects(request, None).await;

    assert!(response.is_err());
}

#[tokio::test]
async fn test_wait_for_effects_fastpath_certified_then_executed() {
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
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        // Also test the case where details are not requested.
        include_details: false,
        ping: None,
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
            fast_path: _,
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
    // This test exercises the path where after the transaction has been executed,
    // it is possible to get acknowledgement of the execution with consensus position.
    // And it is possible to get the full effects without consensus position.
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef::MIN,
        index: TransactionIndex::MIN,
    };

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

    // -------- First, test getting effects acknowledgement with consensus position. --------

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        // Also test the case where details are not requested.
        include_details: false,
        ping: None,
    })
    .unwrap();

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
            fast_path: _,
        } => {
            assert!(details.is_none());
            assert_eq!(effects_digest, exec_effects.digest());
        }
        _ => panic!("Expected Executed response"),
    }

    // -------- Then, test getting full effects without consensus position. --------

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: None,
        include_details: true,
        ping: None,
    })
    .unwrap();

    let response = test_context
        .client
        .wait_for_effects(request, None)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    match response {
        WaitForEffectsResponse::Executed {
            details,
            effects_digest,
            fast_path: _,
        } => {
            let details = details.unwrap();
            assert_eq!(effects_digest, exec_effects.digest());
            assert_eq!(effects_digest, details.effects.digest());
            assert_eq!(tx_digest, *details.effects.transaction_digest());
        }
        _ => panic!("Expected Executed response"),
    }
}

#[tokio::test]
async fn test_wait_for_effects_expired() {
    let test_context = TestContext::new().await;

    let transaction = test_context.build_test_transaction();
    let tx_digest = *transaction.digest();
    let block_round = 3;
    let tx_position = ConsensusPosition {
        epoch: EpochId::MIN,
        block: BlockRef {
            round: block_round,
            ..BlockRef::MIN
        },
        index: TransactionIndex::MIN,
    };

    let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
        transaction_digest: Some(tx_digest),
        consensus_position: Some(tx_position),
        include_details: true,
        ping: None,
    })
    .unwrap();

    let state_clone = test_context.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let epoch_store = state_clone.epoch_store_for_testing();
        let cache = epoch_store.consensus_tx_status_cache.as_ref().unwrap();

        // Initialize the last committed leader round.
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + block_round)
            .await;

        // Update that will actually trigger expiration using the leader round, CONSENSUS_STATUS_RETENTION_ROUNDS + block_round
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + block_round + 1)
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

#[tokio::test]
async fn test_wait_for_effects_ping() {
    let test_context = TestContext::new().await;

    println!("Case 1. Send a FastPath ping request. The end point should wait until the block is certified via MFP (we assume the ping transaction is in the block).");
    {
        let tx_position = ConsensusPosition {
            epoch: EpochId::MIN,
            block: BlockRef::MIN,
            index: PING_TRANSACTION_INDEX,
        };

        let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: None,
            consensus_position: Some(tx_position),
            include_details: false,
            ping: Some(PingType::FastPath),
        })
        .unwrap();

        let state_clone = test_context.state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let epoch_store = state_clone.epoch_store_for_testing();
            epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::FastpathCertified);
        });

        let response = test_context
            .client
            .wait_for_effects(request, None)
            .await
            .unwrap()
            .try_into()
            .unwrap();

        match response {
            WaitForEffectsResponse::Executed {
                effects_digest,
                details,
                fast_path,
            } => {
                assert!(details.is_none());
                assert_eq!(effects_digest, TransactionEffectsDigest::ZERO);
                assert!(fast_path);
            }
            _ => panic!("Expected Executed response for FastPath ping check"),
        }
    }

    println!("Case 2. Send a Consensus ping request. The end point should wait for the transaction is finalised via Consensus.");
    {
        let mut block = BlockRef::MIN;
        block.round = 5;
        let tx_position = ConsensusPosition {
            epoch: EpochId::MIN,
            block,
            index: TransactionIndex::MIN,
        };

        let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: None,
            consensus_position: Some(tx_position),
            include_details: false,
            ping: Some(PingType::Consensus),
        })
        .unwrap();

        let state_clone = test_context.state.clone();
        tokio::spawn(async move {
            let epoch_store = state_clone.epoch_store_for_testing();

            tokio::time::sleep(Duration::from_millis(100)).await;
            epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::FastpathCertified);

            tokio::time::sleep(Duration::from_millis(100)).await;
            epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::Finalized);
        });

        let response = test_context
            .client
            .wait_for_effects(request, None)
            .await
            .unwrap()
            .try_into()
            .unwrap();

        match response {
            WaitForEffectsResponse::Executed {
                effects_digest,
                details,
                fast_path,
            } => {
                assert!(details.is_none());
                assert_eq!(effects_digest, TransactionEffectsDigest::ZERO);
                assert!(
                    !fast_path,
                    "This is Consensus ping request, so fast_path should be false"
                );
            }
            _ => panic!("Expected Executed response for Consensus ping check"),
        }
    }

    println!("Case 3. Send a Consensus ping request but the corresponding block gets garbage collected and never committed.");
    {
        let mut block = BlockRef::MIN;
        block.round = 10;
        let tx_position = ConsensusPosition {
            epoch: EpochId::MIN,
            block,
            index: TransactionIndex::MIN,
        };

        let request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: None,
            consensus_position: Some(tx_position),
            include_details: false,
            ping: Some(PingType::Consensus),
        })
        .unwrap();

        let state_clone = test_context.state.clone();
        tokio::spawn(async move {
            let epoch_store = state_clone.epoch_store_for_testing();

            // First consider the block as fast path certified. The simulate a "garbage collection".
            tokio::time::sleep(Duration::from_millis(100)).await;
            epoch_store.set_consensus_tx_status(tx_position, ConsensusTxStatus::FastpathCertified);

            // Move the committed round to a round that is far enough in the future that the block is considered garbage collected.
            // get the gc depth and calculate the round that is far enough in the future.
            let gc_depth = epoch_store.protocol_config().gc_depth();
            let leader_round = gc_depth + 50;

            tokio::time::sleep(Duration::from_millis(100)).await;
            let consensus_tx_status_cache = epoch_store.consensus_tx_status_cache.as_ref().unwrap();
            consensus_tx_status_cache
                .update_last_committed_leader_round(leader_round)
                .await;
            // The second time we update the last committed leader round will kick of a clean up - the first one doesn't.
            consensus_tx_status_cache
                .update_last_committed_leader_round(leader_round + 1)
                .await;
        });

        let response = test_context
            .client
            .wait_for_effects(request, None)
            .await
            .unwrap()
            .try_into()
            .unwrap();

        match response {
            WaitForEffectsResponse::Rejected { error } => {
                assert_eq!(error, None);
            }
            _ => panic!("Expected Rejected response"),
        }
    }
}
