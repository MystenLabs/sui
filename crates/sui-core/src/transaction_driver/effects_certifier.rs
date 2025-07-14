// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::{join, stream::FuturesUnordered, StreamExt as _};
use mysten_common::debug_fatal;
use rand::{seq::SliceRandom as _, Rng as _};
use sui_types::{
    base_types::ConciseableName,
    committee::EpochId,
    digests::{TransactionDigest, TransactionEffectsDigest},
    messages_consensus::ConsensusPosition,
    messages_grpc::RawWaitForEffectsRequest,
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
};
use tokio::time::{sleep, timeout};
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    stake_aggregator::{InsertResult, StakeAggregator},
    transaction_driver::{
        error::TransactionDriverError, metrics::TransactionDriverMetrics,
        QuorumTransactionResponse, SubmitTransactionOptions,
    },
    wait_for_effects_request::{ExecutedData, WaitForEffectsRequest, WaitForEffectsResponse},
};

const WAIT_FOR_EFFECTS_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct EffectsCertifier {
    metrics: Arc<TransactionDriverMetrics>,
}

impl EffectsCertifier {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn get_certified_finalized_effects<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let (acknowledgments_result, mut full_effects_result) = join!(
            self.wait_for_acknowledgments_with_retry(
                authority_aggregator,
                tx_digest,
                consensus_position,
                epoch,
                options,
            ),
            self.get_full_effects_with_retry(
                authority_aggregator,
                tx_digest,
                consensus_position,
                epoch,
                options,
            ),
        );
        let certified_digest = acknowledgments_result?;

        // Retry until full effects digest matches the certified digest.
        // TODO(fastpath): send backup requests to get full effects before timeout or failure.
        loop {
            match full_effects_result {
                Ok((effects_digest, executed_data)) => {
                    if effects_digest != certified_digest {
                        tracing::warn!(
                            "Full effects digest mismatch ({} vs certified {})",
                            effects_digest,
                            certified_digest
                        );
                    } else {
                        return Ok(self.get_effects_response(
                            effects_digest,
                            executed_data,
                            epoch,
                            tx_digest,
                        ));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get full effects: {e}");
                }
            };
            full_effects_result = self
                .get_full_effects_with_retry(
                    authority_aggregator,
                    tx_digest,
                    consensus_position,
                    epoch,
                    options,
                )
                .await;
        }
    }

    async fn get_full_effects_with_retry<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
        options: &SubmitTransactionOptions,
    ) -> Result<(TransactionEffectsDigest, ExecutedData), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let mut attempts = 0;
        // TODO(fastpath): Remove MAX_ATTEMPTS. Retry until unretriable error.
        const MAX_ATTEMPTS: usize = 10;
        let clients = authority_aggregator
            .authority_clients
            .iter()
            .collect::<Vec<_>>();

        let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            epoch,
            transaction_digest: *tx_digest,
            transaction_position: consensus_position,
            include_details: true,
        })
        .map_err(TransactionDriverError::SerializationError)?;

        // TODO(fastpath): only retry transient (RPC) errors. aggregate permanent errors on a higher level.
        loop {
            attempts += 1;
            // TODO(fastpath): pick target with performance metrics.
            let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();

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
                        // All error cases are retryable until max attempt due to the chance
                        // of the status being returned from a byzantine validator.
                        if let Some(details) = details {
                            return Ok((effects_digest, *details));
                        } else {
                            if attempts >= MAX_ATTEMPTS {
                                return Err(TransactionDriverError::ExecutionDataNotFound(
                                    tx_digest.to_string(),
                                ));
                            }
                            tracing::debug!("Execution data not found, retrying...");
                        }
                    }
                    WaitForEffectsResponse::Rejected { ref reason } => {
                        if attempts >= MAX_ATTEMPTS {
                            return Err(TransactionDriverError::TransactionRejected(
                                reason.to_string(),
                            ));
                        }
                        tracing::debug!("Transaction rejected, retrying... Reason: {}", reason);
                    }
                    WaitForEffectsResponse::Expired(round) => {
                        if attempts >= MAX_ATTEMPTS {
                            return Err(TransactionDriverError::TransactionExpired(
                                round.to_string(),
                            ));
                        }
                        tracing::debug!("Transaction expired at round {}, retrying...", round);
                    }
                },
                Ok(Err(e)) => {
                    if attempts >= MAX_ATTEMPTS {
                        return Err(TransactionDriverError::RpcFailure(
                            name.concise().to_string(),
                            e.to_string(),
                        ));
                    }
                    tracing::debug!(
                        "Full effects request failed from {}: {}, retrying...",
                        name.concise(),
                        e
                    );
                }
                Err(_) => {
                    if attempts >= MAX_ATTEMPTS {
                        return Err(TransactionDriverError::TimeoutGettingFullEffects);
                    }
                    tracing::debug!("Full effects request timed out, retrying...");
                }
            }
        }
    }

    async fn wait_for_acknowledgments_with_retry<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
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
            epoch,
            transaction_digest: *tx_digest,
            transaction_position: consensus_position,
            include_details: false,
        })
        .map_err(TransactionDriverError::SerializationError)?;

        // Create futures for all validators (digest-only requests)
        let mut futures = FuturesUnordered::new();
        for (name, client) in clients {
            let client = client.clone();
            let name = *name;
            let raw_request = raw_request.clone();
            let future = async move {
                // Keep retrying transient errors until cancellation.
                loop {
                    if let Ok(Ok(response)) = timeout(
                        WAIT_FOR_EFFECTS_TIMEOUT,
                        client.wait_for_effects(raw_request.clone(), options.forwarded_client_addr),
                    )
                    .await
                    {
                        return (name, response);
                    };
                    let delay_ms = rand::thread_rng().gen_range(1000..2000);
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            };

            futures.push(future);
        }

        let mut effects_digest_aggregators: HashMap<
            TransactionEffectsDigest,
            StakeAggregator<(), true>,
        > = HashMap::new();
        let mut rejected_aggregator = StakeAggregator::<(), true>::new(committee.clone());
        let mut expired_aggregator = StakeAggregator::<(), true>::new(committee.clone());
        let mut rejected_errors = Vec::new();
        let mut expired_errors = Vec::new();

        // Every validator returns at most one WaitForEffectsResponse.
        while let Some((name, response)) = futures.next().await {
            match response {
                WaitForEffectsResponse::Executed {
                    effects_digest,
                    details: _,
                } => {
                    let aggregator = effects_digest_aggregators
                        .entry(effects_digest)
                        .or_insert_with(|| StakeAggregator::<(), true>::new(committee.clone()));

                    match aggregator.insert_generic(name, ()) {
                        InsertResult::QuorumReached(_) => {
                            let quorum_weight = aggregator.total_votes();
                            for (other_digest, other_aggregator) in effects_digest_aggregators {
                                if other_digest != effects_digest
                                    && other_aggregator.total_votes() > 0
                                {
                                    tracing::warn!(
                                        "Effects digest inconsistency detected: quorum digest {effects_digest:?} (weight {quorum_weight}), other digest {other_digest:?} (weight {})",
                                        other_aggregator.total_votes()
                                    );
                                    self.metrics.effects_digest_mismatches.inc();
                                }
                            }
                            return Ok(effects_digest);
                        }
                        InsertResult::NotEnoughVotes { .. } => {}
                        InsertResult::Failed { error } => {
                            debug_fatal!(
                                "Failed to insert vote for digest {}: {:?}",
                                effects_digest,
                                error
                            );
                        }
                    }
                }
                WaitForEffectsResponse::Rejected { reason } => {
                    rejected_errors.push(format!("{}: {}", name.concise(), reason));
                    self.metrics.rejection_acks.inc();
                    if let InsertResult::Failed { error } =
                        rejected_aggregator.insert_generic(name, ())
                    {
                        debug_fatal!("Failed to insert rejection vote: {:?}", error);
                    }
                }
                WaitForEffectsResponse::Expired(round) => {
                    expired_errors.push(format!("{}: {}", name.concise(), round));
                    self.metrics.expiration_acks.inc();
                    if let InsertResult::Failed { error } =
                        expired_aggregator.insert_generic(name, ())
                    {
                        debug_fatal!("Failed to insert expiration vote: {:?}", error);
                    }
                }
            };

            let executed_weight: u64 = effects_digest_aggregators
                .values()
                .map(|agg| agg.total_votes())
                .sum();
            let rejected_weight = rejected_aggregator.total_votes();
            let expired_weight = expired_aggregator.total_votes();
            let total_weight = executed_weight + rejected_weight + expired_weight;

            if total_weight >= committee.quorum_threshold() {
                // Abort as early as possible because there is no guarantee that another response will be received.
                if rejected_weight + expired_weight >= committee.validity_threshold() {
                    return Err(TransactionDriverError::TransactionRejectedOrExpired(
                        rejected_errors.join(", "),
                        expired_errors.join(", "),
                    ));
                }
                // Check if quorum can still be reached with remaining responses.
                let remaining_weight = committee.total_votes().saturating_sub(total_weight);
                let quorum_feasible = effects_digest_aggregators.values().any(|agg| {
                    agg.total_votes() + remaining_weight >= committee.quorum_threshold()
                });
                if !quorum_feasible {
                    break;
                }
            } else {
                // Abort less eagerly for clearer error message.
                // More responses are available when the network is live.
                if rejected_weight >= committee.validity_threshold() {
                    return Err(TransactionDriverError::TransactionRejected(
                        rejected_errors.join(", "),
                    ));
                } else if expired_weight >= committee.validity_threshold() {
                    return Err(TransactionDriverError::TransactionExpired(
                        expired_errors.join(", "),
                    ));
                }
            }
        }

        // No quorum is reached or can be reached for any effects digest.
        let executed_weight: u64 = effects_digest_aggregators
            .values()
            .map(|agg| agg.total_votes())
            .sum();
        let rejected_weight = rejected_aggregator.total_votes();
        let expired_weight = expired_aggregator.total_votes();

        Err(TransactionDriverError::ForkedExecution {
            total_responses_weight: executed_weight + rejected_weight + expired_weight,
            executed_weight,
            rejected_weight,
            expired_weight,
            // TODO(fastpath): Aggregate and summarize forked effects and errors.
            errors: vec![],
        })
    }

    /// Creates the final full response.
    fn get_effects_response(
        &self,
        effects_digest: TransactionEffectsDigest,
        executed_data: ExecutedData,
        epoch: EpochId,
        tx_digest: &TransactionDigest,
    ) -> QuorumTransactionResponse {
        self.metrics.executed_transactions.inc();

        tracing::debug!("Transaction {tx_digest} executed with effects digest: {effects_digest}",);

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
