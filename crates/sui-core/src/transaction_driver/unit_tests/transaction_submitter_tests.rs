// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
    authority_client::AuthorityAPI,
    transaction_driver::{
        error::TransactionDriverError, message_types::SubmitTxResult,
        metrics::TransactionDriverMetrics, transaction_submitter::TransactionSubmitter,
        SubmitTransactionOptions,
    },
    validator_client_monitor::ValidatorClientMonitor,
};
use async_trait::async_trait;
use consensus_types::block::BlockRef;
use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex as StdMutex,
    },
};
use sui_types::{
    base_types::{random_object_ref, AuthorityName},
    committee::Committee,
    digests::TransactionDigest,
    error::{SuiError, UserInputError},
    messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    },
    messages_consensus::ConsensusPosition,
    messages_grpc::{
        HandleCertificateRequestV3, HandleCertificateResponseV2, HandleCertificateResponseV3,
        HandleSoftBundleCertificatesRequestV3, HandleSoftBundleCertificatesResponseV3,
        HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse, RawSubmitTxRequest,
        RawSubmitTxResponse, RawSubmitTxResult, RawWaitForEffectsRequest,
        RawWaitForEffectsResponse, SystemStateRequest, TransactionInfoRequest,
        TransactionInfoResponse,
    },
    sui_system_state::SuiSystemState,
    transaction::{CertifiedTransaction, Transaction},
};
use tokio::time::{sleep, Duration};

// Mock AuthorityAPI for testing transaction submission.
#[derive(Clone)]
struct MockAuthority {
    _name: AuthorityName,
    submit_responses: Arc<StdMutex<HashMap<TransactionDigest, Result<SubmitTxResult, SuiError>>>>,
    response_delays: Arc<StdMutex<Option<Duration>>>,
    submission_count: Arc<AtomicUsize>,
}

impl MockAuthority {
    fn new(name: AuthorityName) -> Self {
        Self {
            _name: name,
            submit_responses: Arc::new(StdMutex::new(HashMap::new())),
            response_delays: Arc::new(StdMutex::new(None)),
            submission_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn set_submit_response(
        &self,
        tx_digest: TransactionDigest,
        response: Result<SubmitTxResult, SuiError>,
    ) {
        self.submit_responses
            .lock()
            .unwrap()
            .insert(tx_digest, response);
    }

    fn set_response_delay(&self, delay: Duration) {
        *self.response_delays.lock().unwrap() = Some(delay);
    }

    fn get_submission_count(&self) -> usize {
        self.submission_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl AuthorityAPI for MockAuthority {
    async fn submit_transaction(
        &self,
        request: RawSubmitTxRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<RawSubmitTxResponse, SuiError> {
        self.submission_count.fetch_add(1, Ordering::Relaxed);

        let response_delay = *self.response_delays.lock().unwrap();
        if let Some(delay) = response_delay {
            sleep(delay).await;
        }

        // Use 1st transaction in batch for response.
        let maybe_response = match request.transactions.first() {
            Some(tx_bytes) => {
                let tx: Transaction =
                    bcs::from_bytes(tx_bytes).map_err(|e| SuiError::GenericAuthorityError {
                        error: format!("Failed to deserialize transaction: {}", e),
                    })?;
                let tx_digest = tx.digest();
                let responses = self.submit_responses.lock().unwrap();
                responses.get(tx_digest).cloned()
            }
            None => None,
        };

        if let Some(response) = maybe_response {
            match response {
                Ok(result) => {
                    let raw_result: RawSubmitTxResult =
                        result
                            .try_into()
                            .map_err(|_| SuiError::GenericAuthorityError {
                                error: "Failed to convert result".to_string(),
                            })?;
                    let raw_response = RawSubmitTxResponse {
                        results: vec![raw_result],
                    };
                    Ok(raw_response)
                }
                Err(e) => Err(e),
            }
        } else {
            // Default response
            let consensus_position = ConsensusPosition {
                block: BlockRef::MIN,
                index: 0,
                epoch: 0,
            };
            let result = SubmitTxResult::Submitted { consensus_position };
            let raw_result: RawSubmitTxResult =
                result
                    .try_into()
                    .map_err(|_| SuiError::GenericAuthorityError {
                        error: "Failed to convert result".to_string(),
                    })?;
            let raw_response = RawSubmitTxResponse {
                results: vec![raw_result],
            };
            Ok(raw_response)
        }
    }

    async fn wait_for_effects(
        &self,
        _request: RawWaitForEffectsRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<RawWaitForEffectsResponse, SuiError> {
        unimplemented!();
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
        _request: sui_types::messages_grpc::RawValidatorHealthRequest,
    ) -> Result<sui_types::messages_grpc::RawValidatorHealthResponse, SuiError> {
        Ok(sui_types::messages_grpc::RawValidatorHealthResponse::default())
    }
}

fn create_test_authority_aggregator_with_rgp(
    reference_gas_price: u64,
) -> (AuthorityAggregator<MockAuthority>, Vec<Arc<MockAuthority>>) {
    let (committee, _) = Committee::new_simple_test_committee_of_size(4);

    let mut authority_clients = BTreeMap::new();
    let mut mock_authorities = Vec::new();

    for (name, _) in committee.members() {
        let mock_authority = Arc::new(MockAuthority::new(*name));
        authority_clients.insert(*name, (*mock_authority).clone());
        mock_authorities.push(mock_authority);
    }

    let mut aggregator = AuthorityAggregatorBuilder::from_committee(committee)
        .build_custom_clients(authority_clients);
    aggregator.reference_gas_price = reference_gas_price;
    (aggregator, mock_authorities)
}

fn create_test_raw_request(gas_price: u64) -> RawSubmitTxRequest {
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::crypto::{get_account_key_pair, AccountKeyPair};

    let (sender, keypair): (_, AccountKeyPair) = get_account_key_pair();
    let gas_object_ref = random_object_ref();

    let tx_data = TestTransactionBuilder::new(sender, gas_object_ref, gas_price)
        .transfer_sui(None, sender)
        .build();

    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);

    RawSubmitTxRequest {
        transactions: vec![bcs::to_bytes(&tx).unwrap().into()],
        soft_bundle: false,
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_submit_transaction_with_amplification() {
    telemetry_subscribers::init_for_testing();

    let reference_gas_price = 1000;
    let (authority_aggregator, mock_authorities) =
        create_test_authority_aggregator_with_rgp(reference_gas_price);
    let authority_aggregator = Arc::new(authority_aggregator);

    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let submitter = TransactionSubmitter::new(metrics);

    // Test 1: Transaction with 1x RGP (amplification factor = 1)
    {
        // Reset submission counts
        for mock_authority in &mock_authorities {
            mock_authority.submission_count.store(0, Ordering::Relaxed);
        }

        let gas_price = reference_gas_price;
        let raw_request = create_test_raw_request(gas_price);
        let tx: Transaction = bcs::from_bytes(&raw_request.transactions[0]).unwrap();
        let tx_digest = tx.digest();

        // Set up successful response from all authorities
        for mock_authority in &mock_authorities {
            mock_authority.set_submit_response(
                *tx_digest,
                Ok(SubmitTxResult::Submitted {
                    consensus_position: ConsensusPosition {
                        block: BlockRef::MIN,
                        index: 0,
                        epoch: 0,
                    },
                }),
            );
        }

        let amplification_factor = gas_price / reference_gas_price;
        let options = SubmitTransactionOptions::default();

        let result = submitter
            .submit_transaction(
                &authority_aggregator,
                &client_monitor,
                tx_digest,
                amplification_factor,
                raw_request,
                &options,
            )
            .await;

        assert!(result.is_ok());

        // Verify only one authority was contacted (amplification factor = 1)
        let total_submissions: usize = mock_authorities
            .iter()
            .map(|auth| auth.get_submission_count())
            .sum();
        assert_eq!(total_submissions, 1);
    }

    // Test 2: Transaction with 3x RGP (amplification factor = 3)
    {
        // Reset submission counts
        for mock_authority in &mock_authorities {
            mock_authority.submission_count.store(0, Ordering::Relaxed);
        }

        let gas_price = reference_gas_price * 3;
        let raw_request = create_test_raw_request(gas_price);
        let tx: Transaction = bcs::from_bytes(&raw_request.transactions[0]).unwrap();
        let tx_digest = tx.digest();

        // Set up successful response from all authorities
        for mock_authority in &mock_authorities {
            mock_authority.set_submit_response(
                *tx_digest,
                Ok(SubmitTxResult::Submitted {
                    consensus_position: ConsensusPosition {
                        block: BlockRef::MIN,
                        index: 0,
                        epoch: 0,
                    },
                }),
            );
            // Ensure all requests reach validators before they reply.
            mock_authority.set_response_delay(Duration::from_secs(5));
        }

        let amplification_factor = gas_price / reference_gas_price;
        let options = SubmitTransactionOptions::default();

        let result = submitter
            .submit_transaction(
                &authority_aggregator,
                &client_monitor,
                tx_digest,
                amplification_factor,
                raw_request,
                &options,
            )
            .await;

        assert!(result.is_ok());

        // Verify that 3 authorities were contacted
        let total_submissions: usize = mock_authorities
            .iter()
            .map(|auth| auth.get_submission_count())
            .sum();
        assert_eq!(total_submissions, 3);
    }

    // Test 3: Transaction with high amplification factor still works.
    {
        // Reset submission counts
        for mock_authority in &mock_authorities {
            mock_authority.submission_count.store(0, Ordering::Relaxed);
        }

        let gas_price = reference_gas_price * 100; // Very high gas price
        let raw_request = create_test_raw_request(gas_price);
        let tx: Transaction = bcs::from_bytes(&raw_request.transactions[0]).unwrap();
        let tx_digest = tx.digest();

        // Set up successful response from all authorities
        for mock_authority in &mock_authorities {
            mock_authority.set_submit_response(
                *tx_digest,
                Ok(SubmitTxResult::Submitted {
                    consensus_position: ConsensusPosition {
                        block: BlockRef::MIN,
                        index: 0,
                        epoch: 0,
                    },
                }),
            );
            // Ensure all requests reach validators before they reply.
            mock_authority.set_response_delay(Duration::from_secs(5));
        }

        let amplification_factor = gas_price / reference_gas_price;
        let options = SubmitTransactionOptions::default();

        let result = submitter
            .submit_transaction(
                &authority_aggregator,
                &client_monitor,
                tx_digest,
                amplification_factor,
                raw_request,
                &options,
            )
            .await;

        assert!(result.is_ok());

        // Verify that all 4 authorities were contacted once.
        let total_submissions: usize = mock_authorities
            .iter()
            .map(|auth| auth.get_submission_count())
            .sum();
        assert_eq!(
            total_submissions, 4,
            "Expected 4 submissions (all validators), got {}",
            total_submissions
        );
    }

    // Test 4: Transaction with errors in submission.
    {
        // Reset submission counts
        for mock_authority in &mock_authorities {
            mock_authority.submission_count.store(0, Ordering::Relaxed);
        }

        let gas_price = reference_gas_price * 4;
        let raw_request = create_test_raw_request(gas_price);
        let tx: Transaction = bcs::from_bytes(&raw_request.transactions[0]).unwrap();
        let tx_digest = tx.digest();

        // Set up successful response from all authorities
        for (i, mock_authority) in mock_authorities.iter().enumerate() {
            if i < 2 {
                mock_authority.set_submit_response(
                    *tx_digest,
                    Err(SuiError::ValidatorOverloadedRetryAfter {
                        retry_after_secs: 1,
                    }),
                );
            } else {
                mock_authority.set_submit_response(
                    *tx_digest,
                    Ok(SubmitTxResult::Submitted {
                        consensus_position: ConsensusPosition {
                            block: BlockRef::MIN,
                            index: 0,
                            epoch: 0,
                        },
                    }),
                );
                // Ensure all requests reach validators before they reply.
                mock_authority.set_response_delay(Duration::from_secs(5));
            }
        }

        let amplification_factor = gas_price / reference_gas_price;
        let options = SubmitTransactionOptions::default();

        let result = submitter
            .submit_transaction(
                &authority_aggregator,
                &client_monitor,
                tx_digest,
                amplification_factor,
                raw_request,
                &options,
            )
            .await;

        assert!(result.is_ok());

        // Verify that all 4 authorities were contacted once.
        let total_submissions: usize = mock_authorities
            .iter()
            .map(|auth| auth.get_submission_count())
            .sum();
        assert_eq!(
            total_submissions, 4,
            "Expected 4 submissions (all validators), got {}",
            total_submissions
        );
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_submit_transaction_invalid_input() {
    telemetry_subscribers::init_for_testing();

    let reference_gas_price = 1000;
    let (authority_aggregator, mock_authorities) =
        create_test_authority_aggregator_with_rgp(reference_gas_price);
    let authority_aggregator = Arc::new(authority_aggregator);

    let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(
        authority_aggregator.clone(),
    ));
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let submitter = TransactionSubmitter::new(metrics);

    // Transaction with 2x RGP for amplification factor = 2
    let gas_price = reference_gas_price * 2;
    let raw_request = create_test_raw_request(gas_price);
    let tx: Transaction = bcs::from_bytes(&raw_request.transactions[0]).unwrap();
    let tx_digest = tx.digest();

    // Set up all authorities to return non-retriable errors
    for mock_authority in &mock_authorities {
        mock_authority.set_submit_response(
            *tx_digest,
            Err(SuiError::UserInputError {
                error: UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: random_object_ref(),
                    current_version: 1.into(),
                },
            }),
        );
    }

    let amplification_factor = gas_price / reference_gas_price;
    let options = SubmitTransactionOptions::default();

    let result = submitter
        .submit_transaction(
            &authority_aggregator,
            &client_monitor,
            tx_digest,
            amplification_factor,
            raw_request,
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::InvalidTransaction { .. } => {
            // Expected - non-retriable error
        }
        e => panic!("Expected InvalidTransaction error, got: {:?}", e),
    }
}
