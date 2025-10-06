// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
    authority_client::AuthorityAPI,
    transaction_driver::{
        effects_certifier::EffectsCertifier, error::TransactionDriverError,
        metrics::TransactionDriverMetrics, SubmitTransactionOptions,
    },
    validator_client_monitor::ValidatorClientMonitor,
};
use async_trait::async_trait;
use consensus_types::block::BlockRef;
use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{Arc, Mutex as StdMutex},
};
use sui_types::{
    base_types::{random_object_ref, AuthorityName},
    committee::Committee,
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::TransactionEffects,
    error::{SuiError, UserInputError},
    messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    },
    messages_consensus::ConsensusPosition,
    messages_grpc::{ExecutedData, SubmitTxRequest, SubmitTxResponse, SubmitTxResult, TxType},
    messages_grpc::{
        HandleCertificateRequestV3, HandleCertificateResponseV2, HandleCertificateResponseV3,
        HandleSoftBundleCertificatesRequestV3, HandleSoftBundleCertificatesResponseV3,
        HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse, SystemStateRequest,
        TransactionInfoRequest, TransactionInfoResponse, ValidatorHealthRequest,
        ValidatorHealthResponse, WaitForEffectsRequest, WaitForEffectsResponse,
    },
    quorum_driver_types::EffectsFinalityInfo,
    sui_system_state::SuiSystemState,
    transaction::{CertifiedTransaction, Transaction},
};
use tokio::time::{sleep, Duration};

// Mock AuthorityAPI for testing
#[derive(Clone)]
struct MockAuthority {
    _name: AuthorityName,
    response_delays: Arc<StdMutex<Option<Duration>>>,
    ack_responses: Arc<StdMutex<HashMap<TransactionDigest, WaitForEffectsResponse>>>,
    full_responses: Arc<StdMutex<HashMap<TransactionDigest, WaitForEffectsResponse>>>,
}

impl MockAuthority {
    fn new(name: AuthorityName) -> Self {
        Self {
            _name: name,
            response_delays: Arc::new(StdMutex::new(None)),
            ack_responses: Arc::new(StdMutex::new(HashMap::new())),
            full_responses: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    fn set_response_delay(&self, delay: Duration) {
        *self.response_delays.lock().unwrap() = Some(delay);
    }

    fn set_ack_response(&self, tx_digest: TransactionDigest, response: WaitForEffectsResponse) {
        self.ack_responses
            .lock()
            .unwrap()
            .insert(tx_digest, response);
    }

    fn set_full_response(&self, tx_digest: TransactionDigest, response: WaitForEffectsResponse) {
        self.full_responses
            .lock()
            .unwrap()
            .insert(tx_digest, response);
    }
}

#[async_trait]
impl AuthorityAPI for MockAuthority {
    async fn submit_transaction(
        &self,
        _request: SubmitTxRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<SubmitTxResponse, SuiError> {
        unimplemented!();
    }

    async fn wait_for_effects(
        &self,
        request: WaitForEffectsRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<WaitForEffectsResponse, SuiError> {
        let response_delay = *self.response_delays.lock().unwrap();
        if let Some(delay) = response_delay {
            sleep(delay).await;
        }

        // Choose the right response based on include_details flag
        let responses = if request.include_details {
            &self.full_responses
        } else {
            &self.ack_responses
        };

        let maybe_response = {
            let responses = responses.lock().unwrap();
            responses.get(&request.transaction_digest.unwrap()).cloned()
        };

        if let Some(response) = maybe_response {
            Ok(response)
        } else {
            // No response configured - this simulates a scenario where effects are not available.
            // Since the actual timeout in effects_certifier is 10 seconds, we sleep longer
            // to ensure the timeout is triggered.
            sleep(Duration::from_secs(30)).await;
            Err(SuiError::TransactionNotFound {
                digest: request.transaction_digest.unwrap(),
            })
        }
    }

    async fn handle_transaction(
        &self,
        _transaction: Transaction,
        _client_addr: Option<SocketAddr>,
    ) -> Result<HandleTransactionResponse, SuiError> {
        unimplemented!();
    }

    async fn handle_certificate_v2(
        &self,
        _certificate: CertifiedTransaction,
        _client_addr: Option<SocketAddr>,
    ) -> Result<HandleCertificateResponseV2, SuiError> {
        unimplemented!();
    }

    async fn handle_certificate_v3(
        &self,
        _request: HandleCertificateRequestV3,
        _client_addr: Option<SocketAddr>,
    ) -> Result<HandleCertificateResponseV3, SuiError> {
        unimplemented!()
    }

    async fn handle_soft_bundle_certificates_v3(
        &self,
        _request: HandleSoftBundleCertificatesRequestV3,
        _client_addr: Option<SocketAddr>,
    ) -> Result<HandleSoftBundleCertificatesResponseV3, SuiError> {
        unimplemented!()
    }

    async fn handle_object_info_request(
        &self,
        _request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_transaction_info_request(
        &self,
        _request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_checkpoint_v2(
        &self,
        _request: CheckpointRequestV2,
    ) -> Result<CheckpointResponseV2, SuiError> {
        unimplemented!()
    }

    async fn handle_system_state_object(
        &self,
        _request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError> {
        unimplemented!()
    }

    async fn validator_health(
        &self,
        _request: ValidatorHealthRequest,
    ) -> Result<ValidatorHealthResponse, SuiError> {
        Ok(ValidatorHealthResponse::default())
    }
}

fn create_test_authority_aggregator() -> AuthorityAggregator<MockAuthority> {
    let (committee, _) = Committee::new_simple_test_committee_of_size(4);
    let mut authority_clients = BTreeMap::new();

    for (name, _) in committee.members() {
        let mock_authority = MockAuthority::new(*name);
        authority_clients.insert(*name, mock_authority);
    }

    AuthorityAggregatorBuilder::from_committee(committee).build_custom_clients(authority_clients)
}

fn create_test_effects_digest(value: u8) -> TransactionEffectsDigest {
    TransactionEffectsDigest::new([value; 32])
}

fn create_test_transaction_digest(value: u8) -> TransactionDigest {
    TransactionDigest::new([value; 32])
}

fn create_test_executed_data() -> ExecutedData {
    ExecutedData {
        effects: TransactionEffects::default(),
        events: None,
        input_objects: Vec::new(),
        output_objects: Vec::new(),
    }
}

#[tokio::test]
async fn test_successful_certified_effects() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let effects_digest = create_test_effects_digest(1);
    let executed_data = create_test_executed_data();

    // Set up successful responses from all authorities
    let executed_response_full = WaitForEffectsResponse::Executed {
        effects_digest,
        details: Some(Box::new(executed_data.clone())),
        fast_path: false,
    };

    let executed_response_ack = WaitForEffectsResponse::Executed {
        effects_digest,
        details: None,
        fast_path: false,
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        client.set_ack_response(tx_digest, executed_response_ack.clone());
        client.set_full_response(tx_digest, executed_response_full.clone());
    }

    let epoch = 0;
    let options = SubmitTransactionOptions::default();
    let consensus_position = ConsensusPosition {
        block: BlockRef::MIN,
        index: 0,
        epoch,
    };
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    // Get certified effects for tx when consensus positions is returned.
    let submit_tx_result = SubmitTxResult::Submitted { consensus_position };
    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            submit_tx_result,
            &options,
        )
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    match response.effects.finality_info {
        EffectsFinalityInfo::QuorumExecuted(returned_epoch) => {
            assert_eq!(returned_epoch, epoch);
        }
        _ => panic!("Expected QuorumExecuted finality info"),
    }

    // Get certified effects for tx when executed effects are returned.
    let executed_response_ack = WaitForEffectsResponse::Executed {
        effects_digest,
        details: None,
        fast_path: false,
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        // Getting the full effects will be skipped as we already have the full effects.
        client.set_ack_response(tx_digest, executed_response_ack.clone());
    }

    let submit_tx_result = SubmitTxResult::Executed {
        effects_digest,
        details: Some(Box::new(executed_data.clone())),
        fast_path: false,
    };
    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            submit_tx_result,
            &options,
        )
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    match response.effects.finality_info {
        EffectsFinalityInfo::QuorumExecuted(returned_epoch) => {
            assert_eq!(returned_epoch, epoch);
        }
        _ => panic!("Expected QuorumExecuted finality info"),
    }
}

#[tokio::test]
async fn test_transaction_rejected_non_retriable() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    // Set up rejected responses from all authorities
    let non_retriable_rejected_response = WaitForEffectsResponse::Rejected {
        error: Some(SuiError::UserInputError {
            error: UserInputError::ObjectVersionUnavailableForConsumption {
                provided_obj_ref: random_object_ref(),
                current_version: 1.into(),
            },
        }),
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        client.set_full_response(tx_digest, non_retriable_rejected_response.clone());
        client.set_ack_response(tx_digest, non_retriable_rejected_response.clone());
    }

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::RejectedByValidators {
            submission_non_retriable_errors,
            submission_retriable_errors,
        } => {
            assert!(submission_non_retriable_errors.total_stake == 7500);
            assert!(submission_retriable_errors.total_stake == 0);
        }
        e => panic!("Expected InvalidTransaction error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_transaction_rejected_retriable() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let retriable_rejected_response = WaitForEffectsResponse::Rejected {
        error: Some(SuiError::UserInputError {
            error: UserInputError::ObjectNotFound {
                object_id: random_object_ref().0,
                version: None,
            },
        }),
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        client.set_full_response(tx_digest, retriable_rejected_response.clone());
        client.set_ack_response(tx_digest, retriable_rejected_response.clone());
    }

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::Aborted {
            submission_non_retriable_errors,
            submission_retriable_errors,
            observed_effects_digests,
        } => {
            assert_eq!(submission_non_retriable_errors.total_stake, 0);
            assert_eq!(submission_retriable_errors.total_stake, 7500);
            assert_eq!(observed_effects_digests.total_stake(), 0);
        }
        e => panic!("Expected Aborted error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_transaction_rejected_with_conflicts() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let lock_conflict_rejected_response = WaitForEffectsResponse::Rejected {
        error: Some(SuiError::ObjectLockConflict {
            obj_ref: random_object_ref(),
            pending_transaction: TransactionDigest::new([0; 32]),
        }),
    };
    let consensus_rejected_response = WaitForEffectsResponse::Rejected { error: None };

    for (i, (_, safe_client)) in authority_aggregator.authority_clients.iter().enumerate() {
        let client = safe_client.authority_client();
        if i < 2 {
            client.set_full_response(tx_digest, lock_conflict_rejected_response.clone());
            client.set_ack_response(tx_digest, lock_conflict_rejected_response.clone());
        } else {
            client.set_full_response(tx_digest, consensus_rejected_response.clone());
            client.set_ack_response(tx_digest, consensus_rejected_response.clone());
        }
    }

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::RejectedByValidators {
            submission_non_retriable_errors,
            submission_retriable_errors,
        } => {
            assert_eq!(submission_non_retriable_errors.total_stake, 5000);
            assert_eq!(submission_retriable_errors.total_stake, 0);
        }
        e => panic!("Expected InvalidTransaction error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_transaction_expired() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let expired_response = WaitForEffectsResponse::Expired {
        epoch: 42,
        round: Some(100),
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        client.set_ack_response(tx_digest, expired_response.clone());
        client.set_full_response(tx_digest, expired_response.clone());
    }

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::Aborted {
            submission_non_retriable_errors,
            submission_retriable_errors,
            observed_effects_digests,
        } => {
            assert_eq!(submission_non_retriable_errors.total_stake, 0);
            assert_eq!(submission_retriable_errors.total_stake, 7500);
            assert_eq!(observed_effects_digests.total_stake(), 0);
        }
        e => panic!("Expected Aborted error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_mixed_rejected_and_expired() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let expired_response = WaitForEffectsResponse::Expired {
        epoch: 42,
        round: Some(100),
    };

    let non_retriable_rejected_response = WaitForEffectsResponse::Rejected {
        error: Some(SuiError::UserInputError {
            error: UserInputError::ObjectVersionUnavailableForConsumption {
                provided_obj_ref: random_object_ref(),
                current_version: 1.into(),
            },
        }),
    };

    tracing::debug!("Case #1: Test mixed rejected and expired responses - non-retriable");
    let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
    for (i, authority_name) in authorities.iter().enumerate() {
        let client = authority_aggregator
            .authority_clients
            .get(authority_name)
            .unwrap()
            .authority_client();
        if i % 2 == 0 {
            client.set_ack_response(tx_digest, non_retriable_rejected_response.clone());
            client.set_full_response(tx_digest, non_retriable_rejected_response.clone());
        } else {
            client.set_ack_response(tx_digest, expired_response.clone());
            client.set_full_response(tx_digest, expired_response.clone());
        }
    }

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::RejectedByValidators {
            submission_non_retriable_errors,
            submission_retriable_errors,
        } => {
            assert_eq!(submission_non_retriable_errors.total_stake, 5000); // 2 validators with non-retriable
            assert_eq!(submission_retriable_errors.total_stake, 2500); // 2 validators with retriable, only one recorded as we exit early.
        }
        e => panic!("Expected InvalidTransaction error, got: {:?}", e),
    }

    tracing::debug!("Case #2: Test mixed rejected and expired responses - retriable");
    let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
    for (i, authority_name) in authorities.iter().enumerate() {
        let client = authority_aggregator
            .authority_clients
            .get(authority_name)
            .unwrap()
            .authority_client();
        if i == 0 {
            client.set_ack_response(tx_digest, non_retriable_rejected_response.clone());
            client.set_full_response(tx_digest, non_retriable_rejected_response.clone());
        } else {
            client.set_ack_response(tx_digest, expired_response.clone());
            client.set_full_response(tx_digest, expired_response.clone());
        }
    }

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::Aborted {
            submission_non_retriable_errors,
            submission_retriable_errors,
            observed_effects_digests,
        } => {
            assert_eq!(submission_non_retriable_errors.total_stake, 2500);
            assert_eq!(submission_retriable_errors.total_stake, 7500);
            assert_eq!(observed_effects_digests.total_stake(), 0);
        }
        e => panic!("Expected Aborted error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_mixed_rejected_reasons() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let retriable_rejected_response = WaitForEffectsResponse::Rejected {
        error: Some(SuiError::UserInputError {
            error: UserInputError::ObjectNotFound {
                object_id: random_object_ref().0,
                version: None,
            },
        }),
    };
    let non_retriable_rejected_response = WaitForEffectsResponse::Rejected {
        error: Some(SuiError::UserInputError {
            error: UserInputError::ObjectVersionUnavailableForConsumption {
                provided_obj_ref: random_object_ref(),
                current_version: 1.into(),
            },
        }),
    };
    let reason_not_found_response = WaitForEffectsResponse::Rejected { error: None };

    {
        tracing::debug!("Case #1: Test 2 retriable and 2 non-retriable reasons that arrive later");
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
        for (i, authority_name) in authorities.iter().enumerate() {
            let client = authority_aggregator
                .authority_clients
                .get(authority_name)
                .unwrap()
                .authority_client();
            if i < 2 {
                client.set_ack_response(tx_digest, retriable_rejected_response.clone());
                client.set_full_response(tx_digest, retriable_rejected_response.clone());
            } else {
                // Delay non-retriable responses to ensure they are aggregated.
                client.set_response_delay(Duration::from_secs(1));
                client.set_ack_response(tx_digest, non_retriable_rejected_response.clone());
                client.set_full_response(tx_digest, non_retriable_rejected_response.clone());
            }
        }

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &client_monitor,
                Some(tx_digest),
                TxType::SingleWriter,
                *name,
                SubmitTxResult::Submitted { consensus_position },
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::RejectedByValidators {
                submission_non_retriable_errors,
                submission_retriable_errors: _,
            } => {
                assert_eq!(submission_non_retriable_errors.total_stake, 5000);
            }
            e => panic!("Expected InvalidTransaction error, got: {:?}", e),
        }
    }

    {
        tracing::debug!(
            "Case #2: Test 1 retriable, 1 not found, and 2 non-retriable reasons that arrive later"
        );
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
        for (i, authority_name) in authorities.iter().enumerate() {
            let client = authority_aggregator
                .authority_clients
                .get(authority_name)
                .unwrap()
                .authority_client();
            if i == 0 {
                client.set_ack_response(tx_digest, retriable_rejected_response.clone());
                client.set_full_response(tx_digest, retriable_rejected_response.clone());
            } else if i == 1 {
                client.set_ack_response(tx_digest, reason_not_found_response.clone());
                client.set_full_response(tx_digest, reason_not_found_response.clone());
            } else {
                // Delay non-retriable responses to ensure they are aggregated.
                client.set_response_delay(Duration::from_secs(1));
                client.set_ack_response(tx_digest, non_retriable_rejected_response.clone());
                client.set_full_response(tx_digest, non_retriable_rejected_response.clone());
            }
        }

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &client_monitor,
                Some(tx_digest),
                TxType::SingleWriter,
                *name,
                SubmitTxResult::Submitted { consensus_position },
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::RejectedByValidators {
                submission_non_retriable_errors,
                submission_retriable_errors: _,
            } => {
                assert_eq!(submission_non_retriable_errors.total_stake, 5000);
            }
            e => panic!("Expected InvalidTransaction error, got: {:?}", e),
        }
    }

    {
        tracing::debug!("Case #3: Test 2 retriable, 1 not found, and 1 non-retriable reason that arrives earlier");
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
        for (i, authority_name) in authorities.iter().enumerate() {
            let client = authority_aggregator
                .authority_clients
                .get(authority_name)
                .unwrap()
                .authority_client();
            if i == 0 || i == 1 {
                client.set_response_delay(Duration::from_secs(1));
                client.set_ack_response(tx_digest, retriable_rejected_response.clone());
                client.set_full_response(tx_digest, retriable_rejected_response.clone());
            } else if i == 2 {
                client.set_response_delay(Duration::from_secs(1));
                client.set_ack_response(tx_digest, reason_not_found_response.clone());
                client.set_full_response(tx_digest, reason_not_found_response.clone());
            } else {
                client.set_ack_response(tx_digest, non_retriable_rejected_response.clone());
                client.set_full_response(tx_digest, non_retriable_rejected_response.clone());
            }
        }

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &client_monitor,
                Some(tx_digest),
                TxType::SingleWriter,
                *name,
                SubmitTxResult::Submitted { consensus_position },
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::Aborted {
                submission_retriable_errors,
                submission_non_retriable_errors,
                observed_effects_digests,
            } => {
                assert_eq!(submission_retriable_errors.total_stake, 5000);
                assert_eq!(submission_non_retriable_errors.total_stake, 2500);
                assert!(observed_effects_digests.digests.is_empty());
            }
            e => panic!("Expected InvalidTransaction error, got: {:?}", e),
        }
    }

    {
        tracing::debug!("Case #4: Test 1 retriable, 2 not found, and 1 non-retriable reason that arrives earlier");
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
        for (i, authority_name) in authorities.iter().enumerate() {
            let client = authority_aggregator
                .authority_clients
                .get(authority_name)
                .unwrap()
                .authority_client();
            if i == 0 {
                client.set_response_delay(Duration::from_secs(1));
                client.set_ack_response(tx_digest, retriable_rejected_response.clone());
                client.set_full_response(tx_digest, retriable_rejected_response.clone());
            } else if i == 1 || i == 2 {
                client.set_response_delay(Duration::from_secs(1));
                client.set_ack_response(tx_digest, reason_not_found_response.clone());
                client.set_full_response(tx_digest, reason_not_found_response.clone());
            } else {
                client.set_ack_response(tx_digest, non_retriable_rejected_response.clone());
                client.set_full_response(tx_digest, non_retriable_rejected_response.clone());
            }
        }

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &client_monitor,
                Some(tx_digest),
                TxType::SingleWriter,
                *name,
                SubmitTxResult::Submitted { consensus_position },
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::Aborted {
                submission_retriable_errors,
                submission_non_retriable_errors,
                observed_effects_digests,
            } => {
                assert_eq!(submission_retriable_errors.total_stake, 2500);
                assert_eq!(submission_non_retriable_errors.total_stake, 2500);
                assert!(observed_effects_digests.digests.is_empty());
            }
            e => panic!("Expected InvalidTransaction error, got: {:?}", e),
        }
    }

    {
        tracing::debug!("Case #5: Test 2 retriable arriving later, 2 not found");
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
        for (i, authority_name) in authorities.iter().enumerate() {
            let client = authority_aggregator
                .authority_clients
                .get(authority_name)
                .unwrap()
                .authority_client();
            if i < 2 {
                client.set_response_delay(Duration::from_secs(1));
                client.set_ack_response(tx_digest, retriable_rejected_response.clone());
                client.set_full_response(tx_digest, retriable_rejected_response.clone());
            } else {
                client.set_ack_response(tx_digest, reason_not_found_response.clone());
                client.set_full_response(tx_digest, reason_not_found_response.clone());
            }
        }

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &client_monitor,
                Some(tx_digest),
                TxType::SingleWriter,
                *name,
                SubmitTxResult::Submitted { consensus_position },
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::Aborted {
                submission_retriable_errors,
                submission_non_retriable_errors,
                observed_effects_digests,
            } => {
                assert_eq!(submission_retriable_errors.total_stake, 5000);
                assert_eq!(submission_non_retriable_errors.total_stake, 0);
                assert!(observed_effects_digests.digests.is_empty());
            }
            e => panic!("Expected InvalidTransaction error, got: {:?}", e),
        }
    }
}

#[tokio::test]
async fn test_forked_execution() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let effects_digest_1 = create_test_effects_digest(2);
    let effects_digest_2 = create_test_effects_digest(3);
    let executed_data = create_test_executed_data();

    // Set up conflicting effects digests from different validators
    let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
    for (i, authority_name) in authorities.iter().enumerate() {
        let client = authority_aggregator
            .authority_clients
            .get(authority_name)
            .unwrap()
            .authority_client();
        let digest = if i % 2 == 0 {
            effects_digest_1
        } else {
            effects_digest_2
        };
        let response = WaitForEffectsResponse::Executed {
            effects_digest: digest,
            details: None,
            fast_path: false,
        };
        client.set_ack_response(tx_digest, response);

        let executed_response_full = WaitForEffectsResponse::Executed {
            effects_digest: digest,
            details: Some(Box::new(executed_data.clone())),
            fast_path: false,
        };
        client.set_full_response(tx_digest, executed_response_full.clone());
    }

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::ForkedExecution {
            observed_effects_digests,
            submission_non_retriable_errors,
            submission_retriable_errors,
        } => {
            assert_eq!(observed_effects_digests.total_stake(), 10000); // All validators returned effects
            assert_eq!(submission_non_retriable_errors.total_stake, 0);
            assert_eq!(submission_retriable_errors.total_stake, 0);
            // Should have 2 different effects digests, each with weight 2
            assert_eq!(observed_effects_digests.digests.len(), 2);
        }
        e => panic!("Expected ForkedExecution error, got: {:?}", e),
    }
}

// Makes sure TD does not abort if some effects can still be finalized.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_aborted_with_multiple_effects() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();

    let effects_digest_1 = create_test_effects_digest(2);
    let effects_digest_2 = create_test_effects_digest(3);

    // Set up conflicting effects digests from different validators
    let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
    for (i, authority_name) in authorities.iter().enumerate() {
        let client = authority_aggregator
            .authority_clients
            .get(authority_name)
            .unwrap()
            .authority_client();
        let response = match i {
            0 => WaitForEffectsResponse::Executed {
                effects_digest: effects_digest_1, // from fastpath
                details: None,
                fast_path: false,
            },
            1 => WaitForEffectsResponse::Executed {
                effects_digest: effects_digest_2, // from fastpath
                details: None,
                fast_path: false,
            },
            2 => WaitForEffectsResponse::Rejected {
                error: Some(SuiError::ValidatorOverloadedRetryAfter {
                    retry_after_secs: 5,
                }),
            },
            3 => WaitForEffectsResponse::Rejected {
                error: None, // rejected by consensus
            },
            _ => panic!("Unexpected authority index: {}", i),
        };
        client.set_ack_response(tx_digest, response);
    }

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::Aborted {
            submission_non_retriable_errors,
            submission_retriable_errors,
            observed_effects_digests,
        } => {
            assert_eq!(submission_non_retriable_errors.total_stake, 0);
            assert_eq!(submission_retriable_errors.total_stake, 2500);
            assert_eq!(observed_effects_digests.total_stake(), 5000);
            // Should have 2 different effects digests
            assert_eq!(observed_effects_digests.digests.len(), 2);
        }
        e => panic!("Expected Aborted error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_full_effects_retry_loop() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let effects_digest = create_test_effects_digest(1);
    let executed_data = create_test_executed_data();

    // Set up successful acknowledgments from all authorities
    let executed_response_ack = WaitForEffectsResponse::Executed {
        effects_digest,
        details: None,
        fast_path: false,
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        client.set_ack_response(tx_digest, executed_response_ack.clone());
    }

    // Set up full effects responses - first authority fails, second succeeds
    let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
    for (i, authority_name) in authorities.iter().enumerate() {
        let client = authority_aggregator
            .authority_clients
            .get(authority_name)
            .unwrap()
            .authority_client();

        if i == 0 {
            // First authority fails to get full effects
            let failed_response = WaitForEffectsResponse::Rejected {
                error: Some(SuiError::UserInputError {
                    error: UserInputError::ObjectNotFound {
                        object_id: random_object_ref().0,
                        version: None,
                    },
                }),
            };
            client.set_full_response(tx_digest, failed_response);
        } else {
            // Other authorities succeed
            let successful_response = WaitForEffectsResponse::Executed {
                effects_digest,
                details: Some(Box::new(executed_data.clone())),
                fast_path: false,
            };
            client.set_full_response(tx_digest, successful_response);
        }
    }

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();
    let name = authorities[0]; // Use first authority as target

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    // Should succeed because the second authority provides valid full effects
    assert!(result.is_ok());
    let response = result.unwrap();
    match response.effects.finality_info {
        EffectsFinalityInfo::QuorumExecuted(returned_epoch) => {
            assert_eq!(returned_epoch, epoch);
        }
        _ => panic!("Expected QuorumExecuted finality info"),
    }
}

#[tokio::test]
async fn test_full_effects_digest_mismatch() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let certified_digest = create_test_effects_digest(1);
    let mismatched_digest = create_test_effects_digest(2);
    let executed_data = create_test_executed_data();

    // Set up successful acknowledgments from all authorities
    let executed_response_ack = WaitForEffectsResponse::Executed {
        effects_digest: certified_digest,
        details: None,
        fast_path: false,
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        client.set_ack_response(tx_digest, executed_response_ack.clone());
    }

    // Set up full effects responses - first authority returns mismatched digest
    let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
    for (i, authority_name) in authorities.iter().enumerate() {
        let client = authority_aggregator
            .authority_clients
            .get(authority_name)
            .unwrap()
            .authority_client();

        if i == 0 {
            // First authority returns mismatched digest
            let mismatched_response = WaitForEffectsResponse::Executed {
                effects_digest: mismatched_digest,
                details: Some(Box::new(executed_data.clone())),
                fast_path: false,
            };
            client.set_full_response(tx_digest, mismatched_response);
        } else {
            // Other authorities return correct digest
            let correct_response = WaitForEffectsResponse::Executed {
                effects_digest: certified_digest,
                details: Some(Box::new(executed_data.clone())),
                fast_path: false,
            };
            client.set_full_response(tx_digest, correct_response);
        }
    }

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();
    let name = authorities[0]; // Use first authority as target

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    // Should succeed because the second authority provides correct full effects
    assert!(result.is_ok());
    let response = result.unwrap();
    match response.effects.finality_info {
        EffectsFinalityInfo::QuorumExecuted(returned_epoch) => {
            assert_eq!(returned_epoch, epoch);
        }
        _ => panic!("Expected QuorumExecuted finality info"),
    }
}

#[tokio::test]
async fn test_request_retrier_exhaustion() {
    telemetry_subscribers::init_for_testing();
    let authority_aggregator = Arc::new(create_test_authority_aggregator());
    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let certifier = EffectsCertifier::new(metrics);

    let tx_digest = create_test_transaction_digest(1);
    let effects_digest = create_test_effects_digest(1);

    // Set up successful acknowledgments from all authorities
    let executed_response_ack = WaitForEffectsResponse::Executed {
        effects_digest,
        details: None,
        fast_path: false,
    };

    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        client.set_ack_response(tx_digest, executed_response_ack.clone());
    }

    // Set up all authorities to fail getting full effects
    for (_, safe_client) in authority_aggregator.authority_clients.iter() {
        let client = safe_client.authority_client();
        let failed_response = WaitForEffectsResponse::Rejected {
            error: Some(SuiError::UserInputError {
                error: UserInputError::ObjectNotFound {
                    object_id: random_object_ref().0,
                    version: None,
                },
            }),
        };
        client.set_full_response(tx_digest, failed_response);
    }

    let epoch = 0;
    let consensus_position = ConsensusPosition {
        epoch,
        block: BlockRef::MIN,
        index: 0,
    };
    let options = SubmitTransactionOptions::default();
    let name = authority_aggregator
        .authority_clients
        .keys()
        .next()
        .unwrap();

    let result = certifier
        .get_certified_finalized_effects(
            &authority_aggregator,
            &client_monitor,
            Some(tx_digest),
            TxType::SingleWriter,
            *name,
            SubmitTxResult::Submitted { consensus_position },
            &options,
        )
        .await;

    // Should fail because all authorities fail to get full effects
    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::Aborted { .. } => {
            // Expected - all authorities failed to get full effects
        }
        e => panic!("Expected Aborted error, got: {:?}", e),
    }
}
