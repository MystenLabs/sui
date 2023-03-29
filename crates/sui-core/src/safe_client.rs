// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use crate::epoch::committee_store::CommitteeStore;
use fastcrypto::encoding::Encoding;
use mysten_metrics::histogram::{Histogram, HistogramVec};
use prometheus::core::GenericCounter;
use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry, IntCounter,
    IntCounterVec, Registry,
};
use std::sync::Arc;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{base_types::*, committee::*, fp_ensure};
use sui_types::{
    error::{SuiError, SuiResult},
    messages::*,
};
use tap::TapFallible;
use tracing::{debug, error, warn};

macro_rules! check_error {
    ($address:expr, $cond:expr, $msg:expr) => {
        $cond.tap_err(|err| {
            if err.individual_error_indicates_epoch_change() {
                debug!(?err, authority=?$address, "Not a real client error");
            } else {
                error!(?err, authority=?$address, $msg);
            }
        })
    }
}

#[derive(Clone)]
pub struct SafeClientMetricsBase {
    total_requests_by_address_method: IntCounterVec,
    total_responses_by_address_method: IntCounterVec,
    latency: HistogramVec,
    potentially_temporarily_invalid_signatures: IntCounter,
}

impl SafeClientMetricsBase {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_requests_by_address_method: register_int_counter_vec_with_registry!(
                "safe_client_total_requests_by_address_method",
                "Total requests to validators group by address and method",
                &["address", "method"],
                registry,
            )
            .unwrap(),
            total_responses_by_address_method: register_int_counter_vec_with_registry!(
                "safe_client_total_responses_by_address_method",
                "Total good (OK) responses from validators group by address and method",
                &["address", "method"],
                registry,
            )
            .unwrap(),
            latency: HistogramVec::new_in_registry(
                "safe_client_latency",
                "RPC latency observed by safe client aggregator, group by address and method",
                &["address", "method"],
                registry,
            ),
            potentially_temporarily_invalid_signatures: register_int_counter_with_registry!(
                "safe_client_potentially_temporarily_invalid_signatures",
                "Number of PotentiallyTemporarilyInvalidSignature errors",
                registry,
            )
            .unwrap(),
        }
    }
}

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct SafeClientMetrics {
    total_requests_handle_transaction_info_request: GenericCounter<prometheus::core::AtomicU64>,
    total_ok_responses_handle_transaction_info_request: GenericCounter<prometheus::core::AtomicU64>,
    total_requests_handle_object_info_request: GenericCounter<prometheus::core::AtomicU64>,
    total_ok_responses_handle_object_info_request: GenericCounter<prometheus::core::AtomicU64>,
    handle_transaction_latency: Histogram,
    handle_certificate_latency: Histogram,
    handle_obj_info_latency: Histogram,
    handle_tx_info_latency: Histogram,
    potentially_temporarily_invalid_signatures: IntCounter,
}

impl SafeClientMetrics {
    pub fn new(metrics_base: &SafeClientMetricsBase, validator_address: AuthorityName) -> Self {
        let validator_address = validator_address.to_string();

        let total_requests_handle_transaction_info_request = metrics_base
            .total_requests_by_address_method
            .with_label_values(&[&validator_address, "handle_transaction_info_request"]);
        let total_ok_responses_handle_transaction_info_request = metrics_base
            .total_responses_by_address_method
            .with_label_values(&[&validator_address, "handle_transaction_info_request"]);

        let total_requests_handle_object_info_request = metrics_base
            .total_requests_by_address_method
            .with_label_values(&[&validator_address, "handle_object_info_request"]);
        let total_ok_responses_handle_object_info_request = metrics_base
            .total_responses_by_address_method
            .with_label_values(&[&validator_address, "handle_object_info_request"]);

        let handle_transaction_latency = metrics_base
            .latency
            .with_label_values(&[&validator_address, "handle_transaction"]);
        let handle_certificate_latency = metrics_base
            .latency
            .with_label_values(&[&validator_address, "handle_certificate"]);
        let handle_obj_info_latency = metrics_base
            .latency
            .with_label_values(&[&validator_address, "handle_object_info_request"]);
        let handle_tx_info_latency = metrics_base
            .latency
            .with_label_values(&[&validator_address, "handle_transaction_info_request"]);
        let potentially_temporarily_invalid_signatures = metrics_base
            .potentially_temporarily_invalid_signatures
            .clone();

        Self {
            total_requests_handle_transaction_info_request,
            total_ok_responses_handle_transaction_info_request,
            total_requests_handle_object_info_request,
            total_ok_responses_handle_object_info_request,
            handle_transaction_latency,
            handle_certificate_latency,
            handle_obj_info_latency,
            handle_tx_info_latency,
            potentially_temporarily_invalid_signatures,
        }
    }

    pub fn new_for_tests(validator_address: AuthorityName) -> Self {
        let registry = Registry::new();
        let metrics_base = SafeClientMetricsBase::new(&registry);
        Self::new(&metrics_base, validator_address)
    }
}

/// See `SafeClientMetrics::new` for description of each metrics.
/// The metrics are per validator client.
#[derive(Clone)]
pub struct SafeClient<C> {
    authority_client: C,
    committee_store: Arc<CommitteeStore>,
    address: AuthorityPublicKeyBytes,
    metrics: SafeClientMetrics,
}

impl<C> SafeClient<C> {
    pub fn new(
        authority_client: C,
        committee_store: Arc<CommitteeStore>,
        address: AuthorityPublicKeyBytes,
        metrics: SafeClientMetrics,
    ) -> Self {
        Self {
            authority_client,
            committee_store,
            address,
            metrics,
        }
    }
}

impl<C> SafeClient<C> {
    pub fn authority_client(&self) -> &C {
        &self.authority_client
    }

    #[cfg(test)]
    pub fn authority_client_mut(&mut self) -> &mut C {
        &mut self.authority_client
    }

    fn get_committee(&self, epoch_id: &EpochId) -> SuiResult<Arc<Committee>> {
        self.committee_store
            .get_committee(epoch_id)?
            .ok_or(SuiError::MissingCommitteeAtEpoch(*epoch_id))
    }

    fn check_signed_effects_plain(
        &self,
        digest: &TransactionDigest,
        signed_effects: SignedTransactionEffects,
        expected_effects_digest: Option<&TransactionEffectsDigest>,
    ) -> SuiResult<SignedTransactionEffects> {
        // Check it has the right signer
        fp_ensure!(
            signed_effects.auth_sig().authority == self.address,
            SuiError::ByzantineAuthoritySuspicion {
                authority: self.address,
                reason: format!(
                    "Unexpected validator address in the signed effects signature: {:?}",
                    signed_effects.auth_sig().authority
                ),
            }
        );
        // Checks it concerns the right tx
        fp_ensure!(
            signed_effects.data().transaction_digest() == digest,
            SuiError::ByzantineAuthoritySuspicion {
                authority: self.address,
                reason: "Unexpected tx digest in the signed effects".to_string()
            }
        );
        // check that the effects digest is correct.
        if let Some(effects_digest) = expected_effects_digest {
            fp_ensure!(
                signed_effects.digest() == effects_digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: "Effects digest does not match with expected digest".to_string()
                }
            );
        }
        self.get_committee(&signed_effects.epoch())?;
        Ok(signed_effects)
    }

    fn check_transaction_info(
        &self,
        digest: &TransactionDigest,
        transaction: VerifiedTransaction,
        status: TransactionStatus,
    ) -> SuiResult<PlainTransactionInfoResponse> {
        fp_ensure!(
            digest == transaction.digest(),
            SuiError::ByzantineAuthoritySuspicion {
                authority: self.address,
                reason: "Signed transaction digest does not match with expected digest".to_string()
            }
        );
        match status {
            TransactionStatus::Signed(signed) => {
                self.get_committee(&signed.epoch)?;
                Ok(PlainTransactionInfoResponse::Signed(
                    SignedTransaction::new_from_data_and_sig(transaction.into_message(), signed),
                ))
            }
            TransactionStatus::Executed(cert_opt, effects, events) => {
                let signed_effects = self.check_signed_effects_plain(digest, effects, None)?;
                match cert_opt {
                    Some(cert) => {
                        let committee = self.get_committee(&cert.epoch)?;
                        let ct = CertifiedTransaction::new_from_data_and_sig(
                            transaction.into_message(),
                            cert,
                        );
                        ct.verify_signature(&committee)
                            .tap_err(|e| {
                                // TODO: We show the below messages for debugging purposes re. incident #267. When this is fixed, we should remove them again.
                                warn!(?digest, ?ct, "Received invalid tx cert: {}", e);
                                let ct_bytes = fastcrypto::encoding::Base64::encode(
                                    bcs::to_bytes(&ct).unwrap(),
                                );
                                warn!(
                                    ?digest,
                                    ?ct_bytes,
                                    "Received invalid tx cert (serialized): {}",
                                    e
                                );
                            })
                            .map_err(|e| match e {
                                // TODO: Remove as well once incident #267 is resolved.
                                SuiError::InvalidSignature { error } => {
                                    self.metrics
                                        .potentially_temporarily_invalid_signatures
                                        .inc();
                                    SuiError::PotentiallyTemporarilyInvalidSignature { error }
                                }
                                _ => e,
                            })?;
                        let ct = VerifiedCertificate::new_unchecked(ct);
                        Ok(PlainTransactionInfoResponse::ExecutedWithCert(
                            ct,
                            signed_effects,
                            events,
                        ))
                    }
                    None => Ok(PlainTransactionInfoResponse::ExecutedWithoutCert(
                        transaction,
                        signed_effects,
                        events,
                    )),
                }
            }
        }
    }

    fn check_object_response(
        &self,
        request: &ObjectInfoRequest,
        response: ObjectInfoResponse,
    ) -> SuiResult<VerifiedObjectInfoResponse> {
        let ObjectInfoResponse {
            object,
            layout: _,
            lock_for_debugging: _,
        } = response;

        fp_ensure!(
            request.object_id == object.id(),
            SuiError::ByzantineAuthoritySuspicion {
                authority: self.address,
                reason: "Object id mismatch in the response".to_string()
            }
        );

        Ok(VerifiedObjectInfoResponse { object })
    }

    pub fn address(&self) -> &AuthorityPublicKeyBytes {
        &self.address
    }
}

impl<C> SafeClient<C>
where
    C: AuthorityAPI + Send + Sync + Clone + 'static,
{
    /// Initiate a new transfer to a Sui or Primary account.
    pub async fn handle_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> Result<PlainTransactionInfoResponse, SuiError> {
        let _timer = self.metrics.handle_transaction_latency.start_timer();
        let digest = *transaction.digest();
        let response = self
            .authority_client
            .handle_transaction(transaction.clone().into_inner())
            .await?;
        let response = check_error!(
            self.address,
            self.check_transaction_info(&digest, transaction, response.status),
            "Client error in handle_transaction"
        )?;
        Ok(response)
    }

    fn verify_certificate_response(
        &self,
        digest: &TransactionDigest,
        response: HandleCertificateResponse,
    ) -> SuiResult<HandleCertificateResponse> {
        Ok(HandleCertificateResponse {
            signed_effects: self.check_signed_effects_plain(
                digest,
                response.signed_effects,
                None,
            )?,
            events: response.events,
        })
    }

    /// Execute a certificate.
    pub async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponse, SuiError> {
        let digest = *certificate.digest();
        let _timer = self.metrics.handle_certificate_latency.start_timer();
        let response = self
            .authority_client
            .handle_certificate(certificate)
            .await?;

        let verified = check_error!(
            self.address,
            self.verify_certificate_response(&digest, response),
            "Client error in handle_certificate"
        )?;
        Ok(verified)
    }

    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<VerifiedObjectInfoResponse, SuiError> {
        self.metrics.total_requests_handle_object_info_request.inc();

        let _timer = self.metrics.handle_obj_info_latency.start_timer();
        let response = self
            .authority_client
            .handle_object_info_request(request.clone())
            .await?;
        let response = self
            .check_object_response(&request, response)
            .tap_err(|err| error!(?err, authority=?self.address, "Client error in handle_object_info_request"))?;

        self.metrics
            .total_ok_responses_handle_object_info_request
            .inc();
        Ok(response)
    }

    /// Handle Transaction information requests for a given digest.
    pub async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<PlainTransactionInfoResponse, SuiError> {
        self.metrics
            .total_requests_handle_transaction_info_request
            .inc();

        let _timer = self.metrics.handle_tx_info_latency.start_timer();

        let transaction_info = self
            .authority_client
            .handle_transaction_info_request(request.clone())
            .await?;

        let transaction_info = Transaction::new(transaction_info.transaction)
            .verify()
            .and_then(|verified_tx| {
                self.check_transaction_info(
                    &request.transaction_digest,
                    verified_tx,
                    transaction_info.status,
                )
            }).tap_err(|err| {
                error!(?err, authority=?self.address, "Client error in handle_transaction_info_request");
            })?;
        self.metrics
            .total_ok_responses_handle_transaction_info_request
            .inc();
        Ok(transaction_info)
    }

    fn verify_checkpoint_sequence(
        &self,
        expected_seq: Option<CheckpointSequenceNumber>,
        checkpoint: &Option<CertifiedCheckpointSummary>,
    ) -> SuiResult {
        let observed_seq = checkpoint.as_ref().map(|c| c.sequence_number);

        if let (Some(e), Some(o)) = (expected_seq, observed_seq) {
            fp_ensure!(
                e == o,
                SuiError::from("Expected checkpoint number doesn't match with returned")
            );
        }
        Ok(())
    }

    fn verify_contents_exist<T, O>(
        &self,
        request_content: bool,
        checkpoint: &Option<T>,
        contents: &Option<O>,
    ) -> SuiResult {
        match (request_content, checkpoint, contents) {
            // If content is requested, checkpoint is not None, but we are not getting any content,
            // it's an error.
            // If content is not requested, or checkpoint is None, yet we are still getting content,
            // it's an error.
            (true, Some(_), None) | (false, _, Some(_)) | (_, None, Some(_)) => Err(
                SuiError::from("Checkpoint contents inconsistent with request"),
            ),
            _ => Ok(()),
        }
    }

    fn verify_checkpoint_response(
        &self,
        request: &CheckpointRequest,
        response: &CheckpointResponse,
    ) -> SuiResult {
        // Verify response data was correct for request
        let CheckpointResponse {
            checkpoint,
            contents,
        } = &response;
        // Checks that the sequence number is correct.
        self.verify_checkpoint_sequence(request.sequence_number, checkpoint)?;
        self.verify_contents_exist(request.request_content, checkpoint, contents)?;
        // Verify signature.
        match checkpoint {
            Some(c) => {
                let epoch_id = c.epoch;
                c.verify_with_contents(&*self.get_committee(&epoch_id)?, contents.as_ref())
            }
            None => Ok(()),
        }
    }

    pub async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        let resp = self
            .authority_client
            .handle_checkpoint(request.clone())
            .await?;
        self.verify_checkpoint_response(&request, &resp)
            .tap_err(|err| {
                error!(?err, authority=?self.address, "Client error in handle_checkpoint");
            })?;
        Ok(resp)
    }

    pub async fn handle_system_state_object(&self) -> Result<SuiSystemState, SuiError> {
        self.authority_client
            .handle_system_state_object(SystemStateRequest { _unused: false })
            .await
    }
}
