// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use futures::{join, stream::FuturesUnordered, StreamExt as _};
use mysten_common::debug_fatal;
use sui_types::{
    base_types::AuthorityName,
    committee::StakeUnit,
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::TransactionEffectsAPI as _,
    error::SuiError,
    messages_consensus::ConsensusPosition,
    messages_grpc::RawWaitForEffectsRequest,
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
};
use tokio::time::{sleep, timeout};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    safe_client::SafeClient,
    status_aggregator::StatusAggregator,
    transaction_driver::{
        error::{
            aggregate_request_errors, AggregatedEffectsDigests, TransactionDriverError,
            TransactionRequestError,
        },
        metrics::TransactionDriverMetrics,
        request_retrier::RequestRetrier,
        ExecutedData, QuorumTransactionResponse, SubmitTransactionOptions, SubmitTxResponse,
        WaitForEffectsRequest, WaitForEffectsResponse,
    },
};

const WAIT_FOR_EFFECTS_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct EffectsCertifier {
    metrics: Arc<TransactionDriverMetrics>,
}

impl EffectsCertifier {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "error", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn get_certified_finalized_effects<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        // This keeps track of the current target for getting full effects.
        mut current_target: AuthorityName,
        submit_txn_resp: SubmitTxResponse,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        // When consensus position is provided, wait for finalized and fastpath outputs at the validators' side.
        // Otherwise, only wait for finalized effects.
        // Skip the first attempt to get full effects if it is already provided.
        let (consensus_position, full_effects) = match submit_txn_resp {
            SubmitTxResponse::Submitted { consensus_position } => (Some(consensus_position), None),
            SubmitTxResponse::Executed {
                effects_digest,
                details,
            } => match details {
                Some(details) => (None, Some((effects_digest, details))),
                // Details should always be set in correct responses.
                // But if it is not set, continuing to get full effects and certify the digest are still correct.
                None => (None, None),
            },
        };

        let mut retrier = RequestRetrier::new(authority_aggregator);

        let (acknowledgments_result, mut full_effects_result) = join!(
            self.wait_for_acknowledgments(
                authority_aggregator,
                tx_digest,
                consensus_position,
                options,
            ),
            async {
                // No need to send a full effects request if it is already provided.
                if let Some(full_effects) = full_effects {
                    // In this branch, current_target is the authority providing the full effects,
                    // so it is consistent. This is not used though because current_target is
                    // only used with failed full effects query.
                    return Ok(full_effects);
                }
                let (name, client) = retrier
                    .next_target()
                    .expect("there should be at least 1 target");
                current_target = name;
                self.get_full_effects(client, tx_digest, consensus_position, options)
                    .await
            },
        );

        // If the consensus position got rejected, effects certification will see the failure and gather
        // error messages to explain the rejection.
        let certified_digest = acknowledgments_result?;

        // Retry until there is a valid full effects that matches the certified digest, or all targets
        // have been attempted.
        //
        // TODO(fastpath): send backup requests to get full effects before timeout or failure.
        loop {
            match full_effects_result {
                Ok((effects_digest, executed_data)) => {
                    if effects_digest != certified_digest {
                        tracing::warn!(
                            ?current_target,
                            "Full effects digest mismatch ({} vs certified {})",
                            effects_digest,
                            certified_digest
                        );
                    } else {
                        return Ok(
                            self.get_quorum_transaction_response(effects_digest, executed_data)
                        );
                    }
                }
                Err(e) => {
                    tracing::debug!(?current_target, "Failed to get full effects: {e}");
                    // This emits an error when retrier gathers enough (f+1) non-retriable effects errors,
                    // but the error should not happen after effects certification unless there are software bugs
                    // or > f malicious validators.
                    retrier.add_error(current_target, e)?;
                }
            };

            tokio::task::yield_now().await;

            // Retry getting full effects from the next target.

            // This emits an error when retrier has no targets available.
            let (name, client) = retrier.next_target()?;
            current_target = name;
            full_effects_result = self
                .get_full_effects(client, tx_digest, consensus_position, options)
                .await;
        }
    }

    async fn get_full_effects<A>(
        &self,
        client: Arc<SafeClient<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: Option<ConsensusPosition>,
        options: &SubmitTransactionOptions,
    ) -> Result<(TransactionEffectsDigest, Box<ExecutedData>), TransactionRequestError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: *tx_digest,
            consensus_position,
            include_details: true,
        })
        .unwrap();

        match timeout(
            WAIT_FOR_EFFECTS_TIMEOUT,
            client.wait_for_effects(raw_request.clone(), options.forwarded_client_addr),
        )
        .await
        {
            Ok(Ok(response)) => match response {
                WaitForEffectsResponse::Executed {
                    effects_digest,
                    details,
                } => {
                    if let Some(details) = details {
                        Ok((effects_digest, details))
                    } else {
                        tracing::debug!("Execution data not found, retrying...");
                        Err(TransactionRequestError::ExecutionDataNotFound)
                    }
                }
                WaitForEffectsResponse::Rejected { error } => {
                    Err(TransactionRequestError::RejectedAtValidator(error))
                }
                WaitForEffectsResponse::Expired { epoch, round } => Err(
                    TransactionRequestError::StatusExpired(epoch, round.unwrap_or(0)),
                ),
            },
            Ok(Err(e)) => Err(TransactionRequestError::Aborted(e)),
            Err(_) => Err(TransactionRequestError::TimedOutGettingFullEffectsAtValidator),
        }
    }

    async fn wait_for_acknowledgments<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: Option<ConsensusPosition>,
        options: &SubmitTransactionOptions,
    ) -> Result<TransactionEffectsDigest, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let clients = authority_aggregator
            .authority_clients
            .iter()
            .collect::<Vec<_>>();
        let committee = authority_aggregator.committee.clone();
        let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: *tx_digest,
            consensus_position,
            include_details: false,
        })
        .unwrap();

        // Create futures for all validators (digest-only requests)
        let mut futures = FuturesUnordered::new();
        for (name, client) in clients {
            let client = client.clone();
            let name = *name;
            let raw_request = raw_request.clone();
            let future = async move {
                // This loop can only retry RPC errors, timeouts, and other errors retriable
                // without new submission.
                let backoff = ExponentialBackoff::from_millis(100)
                    .max_delay(Duration::from_secs(2))
                    .map(jitter)
                    .take(5);
                for (attempt, delay) in backoff.enumerate() {
                    let result = timeout(
                        WAIT_FOR_EFFECTS_TIMEOUT,
                        client.wait_for_effects(raw_request.clone(), options.forwarded_client_addr),
                    )
                    .await;
                    match result {
                        Ok(Ok(response)) => {
                            return (name, Ok(response));
                        }
                        Ok(Err(e)) => {
                            if !matches!(e, SuiError::RpcError(_, _)) {
                                return (name, Err(e));
                            }
                            tracing::trace!(
                                ?name,
                                "Wait for effects acknowledgement (attempt {attempt}): rpc error: {:?}",
                                e
                            );
                        }
                        Err(_) => {
                            tracing::trace!(
                                ?name,
                                "Wait for effects acknowledgement (attempt {attempt}): timeout"
                            );
                        }
                    };
                    sleep(delay).await;
                }
                (name, Err(SuiError::TimeoutError))
            };

            futures.push(future);
        }

        let mut effects_digest_aggregators: BTreeMap<
            TransactionEffectsDigest,
            StatusAggregator<()>,
        > = BTreeMap::new();
        // Collect errors non-retriable even with new transaction submission.
        let mut non_retriable_errors_aggregator =
            StatusAggregator::<TransactionRequestError>::new(committee.clone());
        // Collect errors retriable with new transaction submission.
        let mut retriable_errors_aggregator =
            StatusAggregator::<TransactionRequestError>::new(committee.clone());

        // Every validator returns at most one WaitForEffectsResponse.
        while let Some((name, response)) = futures.next().await {
            match response {
                Ok(WaitForEffectsResponse::Executed {
                    effects_digest,
                    details: _,
                }) => {
                    let aggregator = effects_digest_aggregators
                        .entry(effects_digest)
                        .or_insert_with(|| StatusAggregator::<()>::new(committee.clone()));
                    aggregator.insert(name, ());
                    if aggregator.reached_quorum_threshold() {
                        let quorum_weight = aggregator.total_votes();
                        for (other_digest, other_aggregator) in effects_digest_aggregators {
                            if other_digest != effects_digest && other_aggregator.total_votes() > 0
                            {
                                tracing::warn!(?name,
                                    "Effects digest inconsistency detected: quorum digest {effects_digest:?} (weight {quorum_weight}), other digest {other_digest:?} (weight {})",
                                    other_aggregator.total_votes()
                                );
                                self.metrics.effects_digest_mismatches.inc();
                            }
                        }
                        return Ok(effects_digest);
                    }
                }
                Ok(WaitForEffectsResponse::Rejected { error }) => {
                    let error = TransactionRequestError::RejectedAtValidator(error);
                    if error.is_submission_retriable() {
                        retriable_errors_aggregator.insert(name, error);
                    } else {
                        non_retriable_errors_aggregator.insert(name, error);
                    }
                    self.metrics.rejection_acks.inc();
                }
                Ok(WaitForEffectsResponse::Expired { epoch, round }) => {
                    let error = TransactionRequestError::StatusExpired(epoch, round.unwrap_or(0));
                    // Expired status is submission retriable.
                    retriable_errors_aggregator.insert(name, error);
                    self.metrics.expiration_acks.inc();
                }
                Err(error) => {
                    let error = TransactionRequestError::Aborted(error);
                    if error.is_submission_retriable() {
                        retriable_errors_aggregator.insert(name, error);
                    } else {
                        non_retriable_errors_aggregator.insert(name, error);
                    }
                }
            };

            // Adding vote up between different StatusAggregators without de-duplication is ok,
            // because each authority only returns one response.
            let executed_weight: u64 = effects_digest_aggregators
                .values()
                .map(|agg| agg.total_votes())
                .sum();
            let non_retriable_weight = non_retriable_errors_aggregator.total_votes();
            let retriable_weight = retriable_errors_aggregator.total_votes();
            let total_weight = executed_weight + non_retriable_weight + retriable_weight;

            // Quorum threshold is used here to gather as many responses as possible for summary,
            // while making sure the loop can still exit with < 1/3 malicious stake.
            if total_weight >= committee.quorum_threshold()
                && non_retriable_weight + retriable_weight >= committee.validity_threshold()
            {
                if non_retriable_errors_aggregator.reached_validity_threshold() {
                    return Err(TransactionDriverError::InvalidTransaction {
                        submission_non_retriable_errors: aggregate_request_errors(
                            non_retriable_errors_aggregator.status_by_authority(),
                        ),
                        submission_retriable_errors: aggregate_request_errors(
                            retriable_errors_aggregator.status_by_authority(),
                        ),
                    });
                } else {
                    let mut observed_effects_digests =
                        Vec::<(TransactionEffectsDigest, Vec<AuthorityName>, StakeUnit)>::new();
                    for (effects_digest, aggregator) in effects_digest_aggregators {
                        observed_effects_digests.push((
                            effects_digest,
                            aggregator.authorities(),
                            aggregator.total_votes(),
                        ));
                    }
                    return Err(TransactionDriverError::Aborted {
                        submission_non_retriable_errors: aggregate_request_errors(
                            non_retriable_errors_aggregator.status_by_authority(),
                        ),
                        submission_retriable_errors: aggregate_request_errors(
                            retriable_errors_aggregator.status_by_authority(),
                        ),
                        observed_effects_digests: AggregatedEffectsDigests {
                            digests: observed_effects_digests,
                        },
                    });
                }
            }
        }

        // At this point, no effects digest has reached quorum. But there is not a validity threshold
        // of failed responses either.
        let retriable_weight = retriable_errors_aggregator.total_votes();
        let mut observed_effects_digests =
            Vec::<(TransactionEffectsDigest, Vec<AuthorityName>, StakeUnit)>::new();
        let mut submission_retriable = false;
        for (effects_digest, aggregator) in effects_digest_aggregators {
            // An effects digest can still get certified, so the transaction is retriable.
            if aggregator.total_votes() + retriable_weight >= committee.quorum_threshold() {
                submission_retriable = true;
            }
            observed_effects_digests.push((
                effects_digest,
                aggregator.authorities(),
                aggregator.total_votes(),
            ));
        }
        if observed_effects_digests.len() <= 1 {
            debug_fatal!(
                "Expect at least 2 effects digests, but got {:?}",
                observed_effects_digests
            );
        }
        if submission_retriable {
            Err(TransactionDriverError::Aborted {
                submission_non_retriable_errors: aggregate_request_errors(
                    non_retriable_errors_aggregator.status_by_authority(),
                ),
                submission_retriable_errors: aggregate_request_errors(
                    retriable_errors_aggregator.status_by_authority(),
                ),
                observed_effects_digests: AggregatedEffectsDigests {
                    digests: observed_effects_digests,
                },
            })
        } else {
            Err(TransactionDriverError::ForkedExecution {
                observed_effects_digests: AggregatedEffectsDigests {
                    digests: observed_effects_digests,
                },
                submission_non_retriable_errors: aggregate_request_errors(
                    non_retriable_errors_aggregator.status_by_authority(),
                ),
                submission_retriable_errors: aggregate_request_errors(
                    retriable_errors_aggregator.status_by_authority(),
                ),
            })
        }
    }

    /// Creates the final full response.
    fn get_quorum_transaction_response(
        &self,
        effects_digest: TransactionEffectsDigest,
        executed_data: Box<ExecutedData>,
    ) -> QuorumTransactionResponse {
        self.metrics.executed_transactions.inc();

        tracing::debug!("Transaction executed with effects digest: {effects_digest}",);

        let epoch = executed_data.effects.executed_epoch();
        let details = FinalizedEffects {
            effects: executed_data.effects,
            finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
        };

        QuorumTransactionResponse {
            effects: details,
            events: executed_data.events,
            input_objects: if !executed_data.input_objects.is_empty() {
                Some(executed_data.input_objects)
            } else {
                None
            },
            output_objects: if !executed_data.output_objects.is_empty() {
                Some(executed_data.output_objects)
            } else {
                None
            },
            auxiliary_data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
        authority_client::AuthorityAPI,
        wait_for_effects_request::{
            ExecutedData, RejectReason, WaitForEffectsRequest, WaitForEffectsResponse,
        },
    };
    use async_trait::async_trait;
    use consensus_types::block::BlockRef;
    use std::{
        collections::{BTreeMap, HashMap},
        net::SocketAddr,
        sync::{Arc, Mutex as StdMutex},
    };
    use sui_types::{
        base_types::AuthorityName,
        committee::Committee,
        digests::{TransactionDigest, TransactionEffectsDigest},
        effects::TransactionEffects,
        error::SuiError,
        messages_checkpoint::{
            CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
        },
        messages_consensus::ConsensusPosition,
        messages_grpc::{
            HandleCertificateRequestV3, HandleCertificateResponseV2, HandleCertificateResponseV3,
            HandleSoftBundleCertificatesRequestV3, HandleSoftBundleCertificatesResponseV3,
            HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse, RawSubmitTxRequest,
            RawSubmitTxResponse, RawWaitForEffectsRequest, RawWaitForEffectsResponse,
            SystemStateRequest, TransactionInfoRequest, TransactionInfoResponse,
        },
        sui_system_state::SuiSystemState,
        transaction::{CertifiedTransaction, Transaction},
    };
    use tokio::time::{sleep, Duration};

    // Mock AuthorityAPI for testing
    #[derive(Clone)]
    struct MockAuthority {
        _name: AuthorityName,
        ack_responses: Arc<StdMutex<HashMap<TransactionDigest, WaitForEffectsResponse>>>,
        full_responses: Arc<StdMutex<HashMap<TransactionDigest, WaitForEffectsResponse>>>,
        delay: Option<Duration>,
        should_timeout: bool,
    }

    impl MockAuthority {
        fn new(name: AuthorityName) -> Self {
            Self {
                _name: name,
                ack_responses: Arc::new(StdMutex::new(HashMap::new())),
                full_responses: Arc::new(StdMutex::new(HashMap::new())),
                delay: None,
                should_timeout: false,
            }
        }

        fn set_ack_response(&self, tx_digest: TransactionDigest, response: WaitForEffectsResponse) {
            self.ack_responses
                .lock()
                .unwrap()
                .insert(tx_digest, response);
        }

        fn set_full_response(
            &self,
            tx_digest: TransactionDigest,
            response: WaitForEffectsResponse,
        ) {
            self.full_responses
                .lock()
                .unwrap()
                .insert(tx_digest, response);
        }
    }

    #[async_trait]
    impl AuthorityAPI for MockAuthority {
        async fn wait_for_effects(
            &self,
            request: RawWaitForEffectsRequest,
            _client_addr: Option<SocketAddr>,
        ) -> Result<RawWaitForEffectsResponse, SuiError> {
            if self.should_timeout {
                sleep(Duration::from_secs(10)).await;
            }

            if let Some(delay) = self.delay {
                sleep(delay).await;
            }

            let wait_request: WaitForEffectsRequest = request.try_into()?;

            // Choose the right response based on include_details flag
            let responses = if wait_request.include_details {
                &self.full_responses
            } else {
                &self.ack_responses
            };

            let responses = responses.lock().unwrap();

            if let Some(response) = responses.get(&wait_request.transaction_digest) {
                let raw_response: RawWaitForEffectsResponse = response
                    .clone()
                    .try_into()
                    .map_err(|_| SuiError::Unknown("Conversion failed".to_string()))?;
                Ok(raw_response)
            } else {
                Err(SuiError::Unknown("No response configured".to_string()))
            }
        }

        async fn submit_transaction(
            &self,
            _request: RawSubmitTxRequest,
            _client_addr: Option<SocketAddr>,
        ) -> Result<RawSubmitTxResponse, SuiError> {
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
    }

    fn create_test_authority_aggregator() -> AuthorityAggregator<MockAuthority> {
        let (committee, _) = Committee::new_simple_test_committee_of_size(4);
        let mut authority_clients = BTreeMap::new();

        for (name, _) in committee.members() {
            let mock_authority = MockAuthority::new(*name);
            authority_clients.insert(*name, mock_authority);
        }

        AuthorityAggregatorBuilder::from_committee(committee)
            .build_custom_clients(authority_clients)
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
        let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
        let certifier = EffectsCertifier::new(metrics);

        let tx_digest = create_test_transaction_digest(1);
        let effects_digest = create_test_effects_digest(2);
        let executed_data = create_test_executed_data();

        // Set up successful responses from all authorities
        let executed_response_full = WaitForEffectsResponse::Executed {
            effects_digest,
            details: Some(Box::new(executed_data.clone())),
        };

        let executed_response_ack = WaitForEffectsResponse::Executed {
            effects_digest,
            details: None,
        };

        for (_, safe_client) in authority_aggregator.authority_clients.iter() {
            let client = safe_client.authority_client();
            client.set_ack_response(tx_digest, executed_response_ack.clone());
            client.set_full_response(tx_digest, executed_response_full.clone());
        }

        let consensus_position = ConsensusPosition {
            block: BlockRef::MIN,
            index: 0,
        };
        let epoch = 1;
        let options = SubmitTransactionOptions::default();

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &tx_digest,
                consensus_position,
                epoch,
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
    async fn test_transaction_rejected() {
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
        let certifier = EffectsCertifier::new(metrics);

        let tx_digest = create_test_transaction_digest(1);

        // Set up rejected responses from all authorities
        let rejected_response = WaitForEffectsResponse::Rejected {
            reason: RejectReason::None,
        };

        for (_, safe_client) in authority_aggregator.authority_clients.iter() {
            let client = safe_client.authority_client();
            client.set_ack_response(tx_digest, rejected_response.clone());
        }

        let consensus_position = ConsensusPosition {
            block: BlockRef::MIN,
            index: 0,
        };
        let epoch = 1;
        let options = SubmitTransactionOptions::default();

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &tx_digest,
                consensus_position,
                epoch,
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::TransactionRejected(reason) => {
                assert!(reason.contains("Rejected with no reason"));
            }
            e => panic!("Expected TransactionRejected error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_transaction_expired() {
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
        let certifier = EffectsCertifier::new(metrics);

        let tx_digest = create_test_transaction_digest(1);

        // Set up expired responses from all authorities
        let expired_response = WaitForEffectsResponse::Expired(42);

        for (_, safe_client) in authority_aggregator.authority_clients.iter() {
            let client = safe_client.authority_client();
            client.set_ack_response(tx_digest, expired_response.clone());
        }

        let consensus_position = ConsensusPosition {
            block: BlockRef::MIN,
            index: 0,
        };
        let epoch = 1;
        let options = SubmitTransactionOptions::default();

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &tx_digest,
                consensus_position,
                epoch,
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::TransactionExpired(errors) => {
                assert!(errors.contains("42"));
            }
            e => panic!("Expected TransactionExpired error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_mixed_rejected_and_expired() {
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
        let certifier = EffectsCertifier::new(metrics);

        let tx_digest = create_test_transaction_digest(1);

        let rejected_response = WaitForEffectsResponse::Rejected {
            reason: RejectReason::None,
        };
        let expired_response = WaitForEffectsResponse::Expired(42);

        // Set up mixed responses
        let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
        for (i, authority_name) in authorities.iter().enumerate() {
            let client = authority_aggregator
                .authority_clients
                .get(authority_name)
                .unwrap()
                .authority_client();
            if i % 2 == 0 {
                client.set_ack_response(tx_digest, rejected_response.clone());
            } else {
                client.set_ack_response(tx_digest, expired_response.clone());
            }
        }

        let consensus_position = ConsensusPosition {
            block: BlockRef::MIN,
            index: 0,
        };
        let epoch = 1;
        let options = SubmitTransactionOptions::default();

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &tx_digest,
                consensus_position,
                epoch,
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::TransactionRejectedOrExpired(
                rejected_errors,
                expired_errors,
            ) => {
                assert!(expired_errors.contains("42"));
                assert!(rejected_errors.contains("Rejected with no reason"));
            }
            e => panic!("Unexpected error type: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_forked_execution() {
        let authority_aggregator = Arc::new(create_test_authority_aggregator());
        let metrics = Arc::new(TransactionDriverMetrics::new_for_tests());
        let certifier = EffectsCertifier::new(metrics);

        let tx_digest = create_test_transaction_digest(1);
        let effects_digest1 = create_test_effects_digest(2);
        let effects_digest2 = create_test_effects_digest(3);

        // Set up conflicting effects digests
        let authorities: Vec<_> = authority_aggregator.authority_clients.keys().collect();
        for (i, authority_name) in authorities.iter().enumerate() {
            let client = authority_aggregator
                .authority_clients
                .get(authority_name)
                .unwrap()
                .authority_client();
            let digest = if i % 2 == 0 {
                effects_digest1
            } else {
                effects_digest2
            };
            let response = WaitForEffectsResponse::Executed {
                effects_digest: digest,
                details: None,
            };
            client.set_ack_response(tx_digest, response);
        }

        let consensus_position = ConsensusPosition {
            block: BlockRef::MIN,
            index: 0,
        };
        let epoch = 1;
        let options = SubmitTransactionOptions::default();

        let result = certifier
            .get_certified_finalized_effects(
                &authority_aggregator,
                &tx_digest,
                consensus_position,
                epoch,
                &options,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionDriverError::ForkedExecution {
                total_responses_weight,
                executed_weight,
                rejected_weight: _,
                expired_weight: _,
                errors: _,
            } => {
                assert_eq!(total_responses_weight, 10000);
                assert_eq!(executed_weight, 10000);
            }
            e => panic!("Expected ForkedExecution error, got: {:?}", e),
        }
    }
}
