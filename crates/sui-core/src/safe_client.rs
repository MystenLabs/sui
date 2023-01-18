// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use crate::epoch::committee_store::CommitteeStore;
use crate::histogram::{Histogram, HistogramVec};
use prometheus::core::GenericCounter;
use prometheus::{register_int_counter_vec_with_registry, IntCounterVec, Registry};
use std::sync::Arc;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber,
};
use sui_types::{base_types::*, committee::*, fp_ensure};
use sui_types::{
    error::{SuiError, SuiResult},
    messages::*,
};
use tap::TapFallible;
use tracing::{debug, error};

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
        Self {
            total_requests_handle_transaction_info_request,
            total_ok_responses_handle_transaction_info_request,
            total_requests_handle_object_info_request,
            total_ok_responses_handle_object_info_request,
            handle_transaction_latency,
            handle_certificate_latency,
            handle_obj_info_latency,
            handle_tx_info_latency,
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

    pub fn authority_client(&self) -> &C {
        &self.authority_client
    }

    #[cfg(test)]
    pub fn authority_client_mut(&mut self) -> &mut C {
        &mut self.authority_client
    }

    fn get_committee(&self, epoch_id: &EpochId) -> SuiResult<Committee> {
        self.committee_store
            .get_committee(epoch_id)?
            .ok_or(SuiError::MissingCommitteeAtEpoch(*epoch_id))
    }

    fn check_signed_effects(
        &self,
        digest: &TransactionDigest,
        signed_effects: SignedTransactionEffects,
        expected_effects_digest: Option<&TransactionEffectsDigest>,
    ) -> SuiResult<VerifiedSignedTransactionEffects> {
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
            signed_effects.data().transaction_digest == *digest,
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
        let committee = self.get_committee(&signed_effects.epoch())?;
        signed_effects.verify(&committee)
    }

    fn check_transaction_info(
        &self,
        digest: &TransactionDigest,
        response: TransactionInfoResponse,
    ) -> SuiResult<VerifiedTransactionInfoResponse> {
        match response {
            TransactionInfoResponse::Signed(signed) => {
                fp_ensure!(
                    digest == signed.digest(),
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                        reason: "Signed transaction digest does not match with expected digest"
                            .to_string()
                    }
                );
                let committee = self.get_committee(&signed.epoch())?;
                Ok(VerifiedTransactionInfoResponse::Signed(
                    signed.verify(&committee)?,
                ))
            }
            TransactionInfoResponse::Executed(cert, effects) => {
                Ok(VerifiedTransactionInfoResponse::Executed(
                    self.check_certificate(cert, digest)?,
                    self.check_signed_effects(digest, effects, None)?,
                ))
            }
        }
    }

    fn check_certificate(
        &self,
        certificate: CertifiedTransaction,
        expected_digest: &TransactionDigest,
    ) -> SuiResult<VerifiedCertificate> {
        // Check it's the right transaction
        fp_ensure!(
            certificate.digest() == expected_digest,
            SuiError::ByzantineAuthoritySuspicion {
                authority: self.address,
                reason: "Unexpected digest in the certified tx".to_string()
            }
        );
        let committee = self.get_committee(&certificate.epoch())?;
        // Check signatures and quorum
        certificate.verify(&committee)
    }

    fn check_object_response(
        &self,
        request: &ObjectInfoRequest,
        response: ObjectInfoResponse,
        // We skip the signature check when there's potentially an epoch change.
        // In this case we don't have the latest committee info locally until reconfig finishes.
        skip_committee_check_during_reconfig: bool,
    ) -> SuiResult<VerifiedObjectInfoResponse> {
        let ObjectInfoResponse {
            parent_certificate,
            requested_object_reference,
            object_and_lock,
        } = response;

        // If we get a certificate make sure it is a valid certificate
        let parent_certificate = if skip_committee_check_during_reconfig {
            parent_certificate.map(VerifiedCertificate::new_unchecked)
        } else if let Some(certificate) = parent_certificate {
            let epoch = certificate.epoch();
            Some(certificate.verify(&self.get_committee(&epoch)?)?)
        } else {
            None
        };

        // Check the right object ID and version is returned
        if let Some((object_id, version, _)) = &requested_object_reference {
            fp_ensure!(
                object_id == &request.object_id,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: "Object ID mismatch".to_string()
                }
            );
            if let ObjectInfoRequestKind::PastObjectInfo(requested_version) = &request.request_kind
            {
                fp_ensure!(
                    version == requested_version,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                        reason: "Object version mismatch".to_string()
                    }
                );
            }
        }

        let object_and_lock = if let Some(object_and_lock) = object_and_lock {
            let ObjectResponse {
                object,
                lock,
                layout,
            } = object_and_lock;
            // We should only be returning the object and lock data if requesting the latest object info.
            fp_ensure!(
                matches!(
                    request.request_kind,
                    ObjectInfoRequestKind::LatestObjectInfo(_)
                ),
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason:
                        "Object and lock data returned when request kind is not LatestObjectInfo"
                            .to_string()
                }
            );

            match requested_object_reference {
                Some(obj_ref) => {
                    // Since we are requesting the latest version, we should validate that if the object's
                    // reference actually match with the one from the responded object reference.
                    fp_ensure!(
                        object.compute_object_reference() == obj_ref,
                        SuiError::ByzantineAuthoritySuspicion {
                            authority: self.address,
                            reason: "Requested object reference mismatch with returned object"
                                .to_string()
                        }
                    );
                }
                None => {
                    // Since we are returning the object for the latest version,
                    // we must also have the requested object reference in the response.
                    // Otherwise the authority has inconsistent data.
                    return Err(SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                        reason: "Object returned without the object reference in response"
                            .to_string(),
                    });
                }
            };

            let signed_transaction = if let Some(signed_transaction) = lock {
                // We cannot reuse the committee fetched above since they may not be from the same
                // epoch.
                let epoch = signed_transaction.epoch();
                let signed_transaction = signed_transaction.verify(&self.get_committee(&epoch)?)?;
                // Check it has the right signer
                fp_ensure!(
                    signed_transaction.auth_sig().authority == self.address,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                        reason: "Unexpected validator address in the signed tx signature"
                            .to_string()
                    }
                );
                Some(signed_transaction)
            } else {
                None
            };

            Some(ObjectResponse {
                object,
                lock: signed_transaction,
                layout,
            })
        } else {
            None
        };

        Ok(VerifiedObjectInfoResponse {
            parent_certificate,
            requested_object_reference,
            object_and_lock,
        })
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
    ) -> Result<VerifiedTransactionInfoResponse, SuiError> {
        let _timer = self.metrics.handle_transaction_latency.start_timer();
        let digest = *transaction.digest();
        let response = self
            .authority_client
            .handle_transaction(transaction.into_inner())
            .await?;
        let response = check_error!(
            self.address,
            self.check_transaction_info(&digest, response),
            "Client error in handle_transaction"
        )?;
        Ok(response)
    }

    fn verify_certificate_response(
        &self,
        digest: &TransactionDigest,
        response: HandleCertificateResponse,
    ) -> SuiResult<VerifiedHandleCertificateResponse> {
        Ok(VerifiedHandleCertificateResponse {
            signed_effects: self.check_signed_effects(digest, response.signed_effects, None)?,
        })
    }

    /// Execute a certificate.
    pub async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<VerifiedHandleCertificateResponse, SuiError> {
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

    /// Pass `skip_committee_check_during_reconfig = true` during reconfiguration, so that
    /// we can tolerate missing committee information when processing the object data.
    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
        skip_committee_check_during_reconfig: bool,
    ) -> Result<VerifiedObjectInfoResponse, SuiError> {
        self.metrics.total_requests_handle_object_info_request.inc();

        let _timer = self.metrics.handle_obj_info_latency.start_timer();
        let response = self
            .authority_client
            .handle_object_info_request(request.clone())
            .await?;
        let response = self
            .check_object_response(&request, response, skip_committee_check_during_reconfig)
            .tap_err(|err|
                error!(?err, authority=?self.address, "Client error in handle_object_info_request")


                )?;

        self.metrics
            .total_ok_responses_handle_object_info_request
            .inc();
        Ok(response)
    }

    /// Handle Transaction information requests for a given digest.
    pub async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<VerifiedTransactionInfoResponse, SuiError> {
        self.metrics
            .total_requests_handle_transaction_info_request
            .inc();

        let _timer = self.metrics.handle_tx_info_latency.start_timer();

        let transaction_info = self
            .authority_client
            .handle_transaction_info_request(request.clone())
            .await?;

        let transaction_info = self.check_transaction_info(&request.transaction_digest, transaction_info).tap_err(|err| {
            error!(?err, authority=?self.address, "Client error in handle_transaction_info_request");
        })?;
        self.metrics
            .total_ok_responses_handle_transaction_info_request
            .inc();
        Ok(transaction_info)
    }

    pub async fn handle_committee_info_request(
        &self,
        request: CommitteeInfoRequest,
    ) -> SuiResult<CommitteeInfoResponse> {
        let requested_epoch = request.epoch;
        let committee_info = self
            .authority_client
            .handle_committee_info_request(request)
            .await?;
        self.verify_committee_info_response(requested_epoch, &committee_info)?;
        Ok(committee_info)
    }

    fn verify_committee_info_response(
        &self,
        requested_epoch: Option<EpochId>,
        committee_info: &CommitteeInfoResponse,
    ) -> SuiResult {
        match requested_epoch {
            Some(epoch) => {
                fp_ensure!(
                    committee_info.epoch == epoch,
                    SuiError::from("Committee info response epoch doesn't match requested epoch")
                );
            }
            None => {
                fp_ensure!(
                    committee_info.committee_info.is_some(),
                    SuiError::from("A valid latest committee must exist")
                );
            }
        }
        Ok(())
    }

    fn verify_checkpoint_sequence(
        &self,
        expected_seq: Option<CheckpointSequenceNumber>,
        checkpoint: &Option<CertifiedCheckpointSummary>,
    ) -> SuiResult {
        let observed_seq = checkpoint.as_ref().map(|c| c.summary().sequence_number);

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
                let epoch_id = c.summary().epoch;
                c.verify(&self.get_committee(&epoch_id)?, contents.as_ref())
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
}
