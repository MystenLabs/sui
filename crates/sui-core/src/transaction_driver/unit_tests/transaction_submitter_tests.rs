// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
    authority_client::AuthorityAPI,
    transaction_driver::{
        SubmitTransactionOptions, error::TransactionDriverError, metrics::TransactionDriverMetrics,
        transaction_submitter::TransactionSubmitter,
    },
    validator_client_monitor::ValidatorClientMonitor,
};
use async_trait::async_trait;
use consensus_types::block::BlockRef;
use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use sui_types::{
    base_types::{AuthorityName, random_object_ref},
    committee::Committee,
    digests::TransactionDigest,
    error::{SuiError, SuiErrorKind, UserInputError},
    messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    },
    messages_consensus::ConsensusPosition,
    messages_grpc::{
        ObjectInfoRequest, ObjectInfoResponse, SubmitTxRequest, SubmitTxResponse, SubmitTxResult,
        SystemStateRequest, TransactionInfoRequest, TransactionInfoResponse, TxType,
        ValidatorHealthRequest, ValidatorHealthResponse, WaitForEffectsRequest,
        WaitForEffectsResponse,
    },
    sui_system_state::SuiSystemState,
    transaction::Transaction,
};
use tokio::time::{Duration, sleep};

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
        request: SubmitTxRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<SubmitTxResponse, SuiError> {
        self.submission_count.fetch_add(1, Ordering::Relaxed);

        let response_delay = *self.response_delays.lock().unwrap();
        if let Some(delay) = response_delay {
            sleep(delay).await;
        }

        let raw_request = request.into_raw()?;
        // Use 1st transaction in batch for response.
        let maybe_response = match raw_request.transactions.first() {
            Some(tx_bytes) => {
                let tx: Transaction =
                    bcs::from_bytes(tx_bytes).map_err(|e| SuiErrorKind::GenericAuthorityError {
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
                Ok(result) => Ok(SubmitTxResponse {
                    results: vec![result],
                }),
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
            Ok(SubmitTxResponse {
                results: vec![result],
            })
        }
    }

    async fn wait_for_effects(
        &self,
        _request: WaitForEffectsRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<WaitForEffectsResponse, SuiError> {
        unimplemented!();
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

    let mut aggregator = AuthorityAggregatorBuilder::from_committee(committee.clone())
        .build_custom_clients(&committee, authority_clients);
    aggregator.reference_gas_price = reference_gas_price;
    (aggregator, mock_authorities)
}

fn create_test_submit_request(gas_price: u64) -> SubmitTxRequest {
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::crypto::{AccountKeyPair, get_account_key_pair};

    let (sender, keypair): (_, AccountKeyPair) = get_account_key_pair();
    let gas_object_ref = random_object_ref();

    let tx_data = TestTransactionBuilder::new(sender, gas_object_ref, gas_price)
        .transfer_sui(None, sender)
        .build();

    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);

    SubmitTxRequest::new_transaction(tx)
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
        let request = create_test_submit_request(gas_price);
        let tx_digest = *request.transaction.as_ref().unwrap().digest();

        // Set up successful response from all authorities
        for mock_authority in &mock_authorities {
            mock_authority.set_submit_response(
                tx_digest,
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
                TxType::SingleWriter,
                amplification_factor,
                request,
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
        let request = create_test_submit_request(gas_price);
        let tx_digest = *request.transaction.as_ref().unwrap().digest();

        // Set up successful response from all authorities
        for mock_authority in &mock_authorities {
            mock_authority.set_submit_response(
                tx_digest,
                Ok(SubmitTxResult::Submitted {
                    consensus_position: ConsensusPosition {
                        block: BlockRef::MIN,
                        index: 0,
                        epoch: 0,
                    },
                }),
            );
            // Ensure all requests reach validators before they reply, but respond before backup delay (1s).
            mock_authority.set_response_delay(Duration::from_millis(500));
        }

        let amplification_factor = gas_price / reference_gas_price;
        let options = SubmitTransactionOptions::default();

        let result = submitter
            .submit_transaction(
                &authority_aggregator,
                &client_monitor,
                TxType::SingleWriter,
                amplification_factor,
                request,
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
        let request = create_test_submit_request(gas_price);
        let tx_digest = *request.transaction.as_ref().unwrap().digest();

        // Set up successful response from all authorities
        for mock_authority in &mock_authorities {
            mock_authority.set_submit_response(
                tx_digest,
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
                TxType::SingleWriter,
                amplification_factor,
                request,
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
        let request = create_test_submit_request(gas_price);
        let tx_digest = *request.transaction.as_ref().unwrap().digest();

        // Set up successful response from all authorities
        for (i, mock_authority) in mock_authorities.iter().enumerate() {
            if i < 2 {
                mock_authority.set_submit_response(
                    tx_digest,
                    Err(SuiErrorKind::ValidatorOverloadedRetryAfter {
                        retry_after_secs: 1,
                    }
                    .into()),
                );
            } else {
                mock_authority.set_submit_response(
                    tx_digest,
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
                TxType::SingleWriter,
                amplification_factor,
                request,
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
    let request = create_test_submit_request(gas_price);
    let tx_digest = *request.transaction.as_ref().unwrap().digest();

    // Set up all authorities to return non-retriable errors
    for mock_authority in &mock_authorities {
        mock_authority.set_submit_response(
            tx_digest,
            Err(SuiErrorKind::UserInputError {
                error: UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: random_object_ref(),
                    current_version: 1.into(),
                },
            }
            .into()),
        );
    }

    let amplification_factor = gas_price / reference_gas_price;
    let options = SubmitTransactionOptions::default();

    let result = submitter
        .submit_transaction(
            &authority_aggregator,
            &client_monitor,
            TxType::SingleWriter,
            amplification_factor,
            request,
            &options,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TransactionDriverError::RejectedByValidators { .. } => {
            // Expected - non-retriable error
        }
        e => panic!("Expected InvalidTransaction error, got: {:?}", e),
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_transaction_processing_returned_as_result() {
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

    let request = create_test_submit_request(reference_gas_price);
    let tx_digest = *request.transaction.as_ref().unwrap().digest();

    // Every validator reports the transaction is already being processed by consensus.
    for mock_authority in &mock_authorities {
        mock_authority.set_submit_response(
            tx_digest,
            Ok(SubmitTxResult::Rejected {
                error: SuiErrorKind::TransactionProcessing {
                    digest: tx_digest,
                    status: "consensus message processed".to_string(),
                }
                .into(),
            }),
        );
    }

    let options = SubmitTransactionOptions::default();
    // A `TransactionProcessing` rejection must be surfaced as a result (so the driver can wait for
    // effects by digest), NOT turned into a retriable submission error that resubmits the tx.
    let (_, result) = submitter
        .submit_transaction(
            &authority_aggregator,
            &client_monitor,
            TxType::SingleWriter,
            1,
            request,
            &options,
        )
        .await
        .expect("TransactionProcessing should be returned as a result, not a submission error");

    assert!(matches!(
        result,
        SubmitTxResult::Rejected { error }
            if matches!(error.as_inner(), SuiErrorKind::TransactionProcessing { .. })
    ));

    // Accepted from the first validator without resubmitting to the others.
    let total_submissions: usize = mock_authorities
        .iter()
        .map(|auth| auth.get_submission_count())
        .sum();
    assert_eq!(total_submissions, 1);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_transaction_submitted_is_retriable() {
    telemetry_subscribers::init_for_testing();

    let reference_gas_price = 1000;
    let (authority_aggregator, mock_authorities) =
        create_test_authority_aggregator_with_rgp(reference_gas_price);
    let authority_aggregator = Arc::new(authority_aggregator);

    // Hold the monitor metrics to assert on recorded submit feedback below.
    let monitor_metrics =
        Arc::new(crate::validator_client_monitor::ValidatorClientMetrics::new_for_tests());
    let client_monitor = ValidatorClientMonitor::new(
        sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig::default(),
        monitor_metrics.clone(),
        Arc::new(arc_swap::ArcSwap::new(authority_aggregator.clone())),
    );
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let submitter = TransactionSubmitter::new(metrics);

    let request = create_test_submit_request(reference_gas_price);
    let tx_digest = *request.transaction.as_ref().unwrap().digest();

    // Every validator responds with the `TransactionSubmitted` dedup rejection. Unlike a
    // `TransactionProcessing` rejection, this does NOT mean the transaction reached consensus,
    // so the submitter must keep resubmitting to other validators rather than short-circuiting into
    // a wait-for-effects.
    for mock_authority in &mock_authorities {
        mock_authority.set_submit_response(
            tx_digest,
            Ok(SubmitTxResult::Rejected {
                error: SuiErrorKind::TransactionSubmitted { digest: tx_digest }.into(),
            }),
        );
    }

    let options = SubmitTransactionOptions::default();
    let result = submitter
        .submit_transaction(
            &authority_aggregator,
            &client_monitor,
            TxType::SingleWriter,
            1,
            request,
            &options,
        )
        .await;

    // Comes back as a retriable submission error (the driver's outer loop retries), NOT surfaced as
    // a result to wait on.
    assert!(
        matches!(result, Err(TransactionDriverError::Aborted { .. })),
        "TransactionSubmitted rejections must stay retriable, got: {result:?}"
    );

    // The submitter resubmitted to every validator instead of stopping after the first.
    let total_submissions: usize = mock_authorities
        .iter()
        .map(|auth| auth.get_submission_count())
        .sum();
    assert_eq!(total_submissions, mock_authorities.len());

    // The dedup hit is caused by the driver's own resubmission, not a validator fault, so no
    // submit failure is recorded against any validator's score.
    for name in authority_aggregator.authority_clients.keys() {
        let display_name = authority_aggregator.get_display_name(name);
        assert_eq!(
            monitor_metrics
                .operation_failure
                .with_label_values(&[display_name.as_str(), "submit", "false"])
                .get(),
            0
        );
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_processing_wrong_digest_is_retriable() {
    telemetry_subscribers::init_for_testing();

    let reference_gas_price = 1000;
    let (authority_aggregator, mock_authorities) =
        create_test_authority_aggregator_with_rgp(reference_gas_price);
    let authority_aggregator = Arc::new(authority_aggregator);

    // Hold the monitor metrics to assert on recorded submit feedback below.
    let monitor_metrics =
        Arc::new(crate::validator_client_monitor::ValidatorClientMetrics::new_for_tests());
    let client_monitor = ValidatorClientMonitor::new(
        sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig::default(),
        monitor_metrics.clone(),
        Arc::new(arc_swap::ArcSwap::new(authority_aggregator.clone())),
    );
    let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
    let submitter = TransactionSubmitter::new(metrics);

    let request = create_test_submit_request(reference_gas_price);
    let tx_digest = *request.transaction.as_ref().unwrap().digest();
    let other_digest = TransactionDigest::random();
    assert_ne!(tx_digest, other_digest);

    // A `TransactionProcessing` rejection that refers to a DIFFERENT transaction. The submitter
    // must not stop resubmitting our transaction based on a claim about another one; it stays
    // retriable.
    for mock_authority in &mock_authorities {
        mock_authority.set_submit_response(
            tx_digest,
            Ok(SubmitTxResult::Rejected {
                error: SuiErrorKind::TransactionProcessing {
                    digest: other_digest,
                    status: "consensus message processed".to_string(),
                }
                .into(),
            }),
        );
    }

    let options = SubmitTransactionOptions::default();
    let result = submitter
        .submit_transaction(
            &authority_aggregator,
            &client_monitor,
            TxType::SingleWriter,
            1,
            request,
            &options,
        )
        .await;

    assert!(
        matches!(result, Err(TransactionDriverError::Aborted { .. })),
        "a durable rejection for a different digest must stay retriable, got: {result:?}"
    );

    let total_submissions: usize = mock_authorities
        .iter()
        .map(|auth| auth.get_submission_count())
        .sum();
    assert_eq!(total_submissions, mock_authorities.len());

    // A processing claim about a different transaction is a malformed response, and does count
    // against the validator's score.
    for name in authority_aggregator.authority_clients.keys() {
        let display_name = authority_aggregator.get_display_name(name);
        assert_eq!(
            monitor_metrics
                .operation_failure
                .with_label_values(&[display_name.as_str(), "submit", "false"])
                .get(),
            1
        );
    }
}
