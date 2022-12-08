// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::{AuthorityAPI, BatchInfoResponseItemStream};
use crate::epoch::committee_store::CommitteeStore;
use crate::histogram::{Histogram, HistogramVec};
use futures::StreamExt;
use prometheus::core::{GenericCounter, GenericGauge};
use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec,
};
use std::sync::Arc;
use sui_types::batch::{AuthorityBatch, SignedBatch, TxSequenceNumber, UpdateItem};
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::messages_checkpoint::{
    AuthenticatedCheckpoint, CheckpointRequest, CheckpointRequestType, CheckpointResponse,
    CheckpointSequenceNumber,
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
            if matches!(err, SuiError::ValidatorHaltedAtEpochEnd) {
                debug!(?err, authority=?$address, "Not a real client error");
            } else {
                error!(?err, authority=?$address, $msg);
            }
        })
    }
}

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct SafeClientMetrics {
    pub(crate) total_requests_by_address_method: IntCounterVec,
    pub(crate) total_responses_by_address_method: IntCounterVec,
    pub(crate) follower_streaming_from_seq_number_by_address: IntGaugeVec,
    pub(crate) follower_streaming_reconnect_times_by_address: IntCounterVec,
    latency: HistogramVec,
}

impl SafeClientMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
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
            follower_streaming_from_seq_number_by_address: register_int_gauge_vec_with_registry!(
                "safe_client_follower_streaming_from_seq_number_by_address",
                "The seq number with which to request follower streaming, group by address",
                &["address"],
                registry,
            )
            .unwrap(),
            follower_streaming_reconnect_times_by_address: register_int_counter_vec_with_registry!(
                "safe_client_follower_streaming_reconnect_times_by_address",
                "Total times that a follower stream is closed and reconnected, group by address",
                &["address"],
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

    pub fn new_for_tests() -> Self {
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}

/// See `SafeClientMetrics::new` for description of each metrics.
/// The metrics are per validator client.
#[derive(Clone)]
pub struct SafeClient<C> {
    authority_client: C,
    committee_store: Arc<CommitteeStore>,
    address: AuthorityPublicKeyBytes,
    metrics_total_requests_handle_transaction_and_effects_info_request:
        GenericCounter<prometheus::core::AtomicU64>,
    metrics_total_ok_responses_handle_transaction_and_effects_info_request:
        GenericCounter<prometheus::core::AtomicU64>,
    metrics_total_requests_handle_transaction_info_request:
        GenericCounter<prometheus::core::AtomicU64>,
    metrics_total_ok_responses_handle_transaction_info_request:
        GenericCounter<prometheus::core::AtomicU64>,
    metrics_total_requests_handle_object_info_request: GenericCounter<prometheus::core::AtomicU64>,
    metrics_total_ok_responses_handle_object_info_request:
        GenericCounter<prometheus::core::AtomicU64>,
    metrics_total_requests_handle_batch_stream: GenericCounter<prometheus::core::AtomicU64>,
    metrics_total_ok_responses_handle_batch_stream: GenericCounter<prometheus::core::AtomicU64>,
    pub(crate) metrics_seq_number_to_handle_batch_stream: GenericGauge<prometheus::core::AtomicI64>,
    pub(crate) metrics_total_times_reconnect_follower_stream:
        GenericCounter<prometheus::core::AtomicU64>,
    metrics_handle_transaction_latency: Histogram,
    metrics_handle_certificate_latency: Histogram,
    metrics_handle_obj_info_latency: Histogram,
    metrics_handle_tx_info_latency: Histogram,
}

impl<C> SafeClient<C> {
    pub fn new(
        authority_client: C,
        committee_store: Arc<CommitteeStore>,
        address: AuthorityPublicKeyBytes,
        safe_client_metrics: Arc<SafeClientMetrics>,
    ) -> Self {
        // Cache counters for efficiency
        let validator_address = address.to_string();
        let requests_metrics_vec = &safe_client_metrics.total_requests_by_address_method;
        let responses_metrics_vec = &safe_client_metrics.total_responses_by_address_method;

        let metrics_total_requests_handle_transaction_and_effects_info_request =
            requests_metrics_vec.with_label_values(&[
                &validator_address,
                "handle_transaction_and_effects_info_request",
            ]);
        let metrics_total_ok_responses_handle_transaction_and_effects_info_request =
            responses_metrics_vec.with_label_values(&[
                &validator_address,
                "handle_transaction_and_effects_info_request",
            ]);

        let metrics_total_requests_handle_transaction_info_request = requests_metrics_vec
            .with_label_values(&[&validator_address, "handle_transaction_info_request"]);
        let metrics_total_ok_responses_handle_transaction_info_request = responses_metrics_vec
            .with_label_values(&[&validator_address, "handle_transaction_info_request"]);

        let metrics_total_requests_handle_object_info_request = requests_metrics_vec
            .with_label_values(&[&validator_address, "handle_object_info_request"]);
        let metrics_total_ok_responses_handle_object_info_request = responses_metrics_vec
            .with_label_values(&[&validator_address, "handle_object_info_request"]);

        let metrics_total_requests_handle_batch_stream =
            requests_metrics_vec.with_label_values(&[&validator_address, "handle_batch_stream"]);
        let metrics_total_ok_responses_handle_batch_stream =
            responses_metrics_vec.with_label_values(&[&validator_address, "handle_batch_stream"]);

        let metrics_seq_number_to_handle_batch_stream = safe_client_metrics
            .follower_streaming_from_seq_number_by_address
            .with_label_values(&[&validator_address]);
        let metrics_total_times_reconnect_follower_stream = safe_client_metrics
            .follower_streaming_reconnect_times_by_address
            .with_label_values(&[&validator_address]);

        let metrics_handle_transaction_latency = safe_client_metrics
            .latency
            .with_label_values(&[&validator_address, "handle_transaction"]);
        let metrics_handle_certificate_latency = safe_client_metrics
            .latency
            .with_label_values(&[&validator_address, "handle_certificate"]);
        let metrics_handle_obj_info_latency = safe_client_metrics
            .latency
            .with_label_values(&[&validator_address, "handle_object_info_request"]);
        let metrics_handle_tx_info_latency = safe_client_metrics
            .latency
            .with_label_values(&[&validator_address, "handle_transaction_info_request"]);

        Self {
            authority_client,
            committee_store,
            address,
            metrics_total_requests_handle_transaction_and_effects_info_request,
            metrics_total_ok_responses_handle_transaction_and_effects_info_request,
            metrics_total_requests_handle_transaction_info_request,
            metrics_total_ok_responses_handle_transaction_info_request,
            metrics_total_requests_handle_object_info_request,
            metrics_total_ok_responses_handle_object_info_request,
            metrics_total_requests_handle_batch_stream,
            metrics_total_ok_responses_handle_batch_stream,
            metrics_seq_number_to_handle_batch_stream,
            metrics_total_times_reconnect_follower_stream,
            metrics_handle_transaction_latency,
            metrics_handle_certificate_latency,
            metrics_handle_obj_info_latency,
            metrics_handle_tx_info_latency,
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

    // Here we centralize all checks for transaction info responses
    fn check_transaction_response(
        &self,
        digest: &TransactionDigest,
        effects_digest: Option<&TransactionEffectsDigest>,
        response: TransactionInfoResponse,
    ) -> SuiResult<VerifiedTransactionInfoResponse> {
        let mut committee = None;

        let TransactionInfoResponse {
            signed_transaction,
            certified_transaction,
            signed_effects,
        } = response;

        let signed_transaction = if let Some(signed_transaction) = signed_transaction {
            committee = Some(self.get_committee(&signed_transaction.epoch())?);
            // Check the transaction signature
            let signed_transaction = signed_transaction.verify(committee.as_ref().unwrap())?;
            // Check it has the right signer
            fp_ensure!(
                signed_transaction.auth_sig().authority == self.address,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: "Unexpected validator address in the signed tx signature".to_string()
                }
            );
            // Check it's the right transaction
            fp_ensure!(
                signed_transaction.digest() == digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: "Unexpected digest in the signed tx".to_string()
                }
            );
            Some(signed_transaction)
        } else {
            None
        };

        let certified_transaction = match certified_transaction {
            Some(certificate) => {
                if committee.is_none() {
                    committee = Some(self.get_committee(&certificate.epoch())?);
                }
                // Check signatures and quorum
                let certificate = certificate.verify(committee.as_ref().unwrap())?;
                // Check it's the right transaction
                fp_ensure!(
                    certificate.digest() == digest,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                        reason: "Unexpected digest in the certified tx".to_string()
                    }
                );
                Some(certificate)
            }
            None => None,
        };

        if let Some(signed_effects) = &signed_effects {
            if committee.is_none() {
                committee = Some(self.get_committee(&signed_effects.epoch())?);
            }
            // Check signature
            signed_effects.verify_signature(committee.as_ref().unwrap())?;
            // Check it has the right signer
            fp_ensure!(
                signed_effects.auth_sig().authority == self.address,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: "Unexpected validator address in the signed effects signature"
                        .to_string()
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
            if let Some(effects_digest) = effects_digest {
                fp_ensure!(
                    signed_effects.digest() == effects_digest,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                        reason: "Effects digest does not match with expected digest".to_string()
                    }
                );
            }
        }

        Ok(VerifiedTransactionInfoResponse {
            signed_transaction,
            certified_transaction,
            signed_effects,
        })
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

    fn check_update_item_batch_response(
        &self,
        _request: BatchInfoRequest,
        signed_batch: &SignedBatch,
        transactions_and_last_batch: &Option<(
            Vec<(TxSequenceNumber, ExecutionDigests)>,
            AuthorityBatch,
        )>,
    ) -> SuiResult {
        // check the signature of the batch
        let epoch = signed_batch.epoch();
        signed_batch.verify_signature(&self.get_committee(&epoch)?)?;

        // ensure transactions enclosed match requested range

        // TODO: check that the batch is within bounds given that the
        //      bounds may now not be known by the requester.
        //
        // if let Some(start) = &request.start {
        //    fp_ensure!(
        //        signed_batch.batch.initial_sequence_number >= *start
        //            && signed_batch.batch.next_sequence_number
        //                <= (*start + request.length + signed_batch.batch.size),
        //        SuiError::ByzantineAuthoritySuspicion {
        //            authority: self.address
        //        }
        //    );
        // }

        // If we have seen a previous batch, use it to make sure the next batch
        // is constructed correctly:

        if let Some((transactions, prev_batch)) = transactions_and_last_batch {
            fp_ensure!(
                !transactions.is_empty(),
                SuiError::GenericAuthorityError {
                    error: "Safe Client: Batches must have some contents.".to_string()
                }
            );
            let reconstructed_batch = AuthorityBatch::make_next(prev_batch, transactions)?;

            fp_ensure!(
                &reconstructed_batch == signed_batch.data(),
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: format!(
                        "Inconsistent batch. signed: {:?}, reconstructed: {:?}",
                        signed_batch.data(),
                        reconstructed_batch
                    )
                }
            );
        }

        Ok(())
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
        let digest = *transaction.digest();
        let _timer = self.metrics_handle_transaction_latency.start_timer();
        let transaction_info = self
            .authority_client
            .handle_transaction(transaction.into_inner())
            .await?;
        let transaction_info = check_error!(
            self.address,
            self.check_transaction_response(&digest, None, transaction_info),
            "Client error in handle_transaction"
        )?;
        Ok(transaction_info)
    }

    fn verify_certificate_response(
        &self,
        digest: &TransactionDigest,
        response: TransactionInfoResponse,
    ) -> SuiResult<VerifiedTransactionInfoResponse> {
        fp_ensure!(
            response.signed_effects.is_some(),
            SuiError::ByzantineAuthoritySuspicion {
                authority: self.address,
                reason: "An Ok response from handle_certificate must contain signed effects"
                    .to_string()
            }
        );
        self.check_transaction_response(digest, None, response)
    }

    /// Execute a certificate.
    pub async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<VerifiedTransactionInfoResponse, SuiError> {
        let digest = *certificate.digest();
        let _timer = self.metrics_handle_certificate_latency.start_timer();
        let transaction_info = self
            .authority_client
            .handle_certificate(certificate)
            .await?;

        let transaction_info = check_error!(
            self.address,
            self.verify_certificate_response(&digest, transaction_info),
            "Client error in handle_certificate"
        )?;
        Ok(transaction_info)
    }

    pub async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        self.authority_client
            .handle_account_info_request(request)
            .await
    }

    /// Pass `skip_committee_check_during_reconfig = true` during reconfiguration, so that
    /// we can tolerate missing committee information when processing the object data.
    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
        skip_committee_check_during_reconfig: bool,
    ) -> Result<VerifiedObjectInfoResponse, SuiError> {
        self.metrics_total_requests_handle_object_info_request.inc();

        let _timer = self.metrics_handle_obj_info_latency.start_timer();
        let response = self
            .authority_client
            .handle_object_info_request(request.clone())
            .await?;
        let response = self
            .check_object_response(&request, response, skip_committee_check_during_reconfig)
            .tap_err(|err|
                error!(?err, authority=?self.address, "Client error in handle_object_info_request")


                )?;

        self.metrics_total_ok_responses_handle_object_info_request
            .inc();
        Ok(response)
    }

    /// Handle Transaction information requests for this account.
    pub async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<VerifiedTransactionInfoResponse, SuiError> {
        self.metrics_total_requests_handle_transaction_info_request
            .inc();
        let digest = request.transaction_digest;

        let _timer = self.metrics_handle_tx_info_latency.start_timer();

        let transaction_info = self
            .authority_client
            .handle_transaction_info_request(request)
            .await?;

        let transaction_info = match self.check_transaction_response(
            &digest,
            None,
            transaction_info,
        ) {
            Err(err) => {
                error!(?err, authority=?self.address, "Client error in handle_transaction_info_request");
                return Err(err);
            }
            Ok(i) => i,
        };
        self.metrics_total_ok_responses_handle_transaction_info_request
            .inc();
        Ok(transaction_info)
    }

    /// Handle Transaction + Effects information requests for this account.
    pub async fn handle_transaction_and_effects_info_request(
        &self,
        digests: &ExecutionDigests,
    ) -> Result<VerifiedTransactionInfoResponse, SuiError> {
        self.metrics_total_requests_handle_transaction_and_effects_info_request
            .inc();
        let transaction_info = self
            .authority_client
            .handle_transaction_info_request(digests.transaction.into())
            .await?;

        let transaction_info = match self.check_transaction_response(
            &digests.transaction,
            Some(&digests.effects),
            transaction_info,
        ) {
            Err(err) => {
                error!(?err, authority=?self.address, "Client error in handle_transaction_and_effects_info_request");
                return Err(err);
            }
            Ok(info) => info,
        };
        self.metrics_total_ok_responses_handle_transaction_and_effects_info_request
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
        checkpoint: &Option<AuthenticatedCheckpoint>,
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
        match &request.request_type {
            CheckpointRequestType::AuthenticatedCheckpoint(seq) => {
                let CheckpointResponse::AuthenticatedCheckpoint {
                    checkpoint,
                    contents,
                } = &response;
                // Checks that the sequence number is correct.
                self.verify_checkpoint_sequence(*seq, checkpoint)?;
                self.verify_contents_exist(request.detail, checkpoint, contents)?;
                // Verify signature.
                match checkpoint {
                    Some(c) => {
                        let epoch_id = c.summary().epoch;
                        c.verify(&self.get_committee(&epoch_id)?, contents.as_ref())
                    }
                    None => Ok(()),
                }
            }
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

    /// Handle Batch information requests for this authority.
    pub async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, SuiError> {
        self.metrics_total_requests_handle_batch_stream.inc();
        let batch_info_items = self
            .authority_client
            .handle_batch_stream(request.clone())
            .await?;
        self.metrics_total_ok_responses_handle_batch_stream.inc();
        let client = self.clone();
        let address = self.address;
        let count: u64 = 0;
        let stream = Box::pin(batch_info_items.scan(
            (None, count),
            move |(txs_and_last_batch, count), batch_info_item| {
                let req_clone = request.clone();
                let client = client.clone();

                // We check if we have exceeded the batch boundary for this request.
                // This is to protect against server DoS
                if *count > 10 * request.length {
                    // If we exceed it return None to end stream
                    return futures::future::ready(None);
                }
                let result = match &batch_info_item {
                    Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch))) => {
                        if let Err(err) = client.check_update_item_batch_response(
                            req_clone,
                            signed_batch,
                            txs_and_last_batch,
                        ) {
                            error!(?err, authority=?address, "Client error in handle_batch_stream");
                            Some(Err(err))
                        } else {
                            // Insert a fresh vector for the new batch of transactions
                            let _ = txs_and_last_batch
                                .insert((Vec::new(), signed_batch.data().clone()));
                            Some(batch_info_item)
                        }
                    }
                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digest)))) => {
                        // A stream always starts with a batch, so the previous should have initialized it.
                        // And here we insert the tuple into the batch.
                        match txs_and_last_batch {
                            None => {
                                let err = SuiError::ByzantineAuthoritySuspicion {
                                    authority: address,
                                    reason: "Stream does not start with a batch".to_string(),
                                };
                                error!(?err, authority=?address, "Client error in handle_batch_stream");
                                Some(Err(err))
                            }
                            Some(txs) => {
                                txs.0.push((*seq, *digest));

                                *count += 1;
                                Some(batch_info_item)
                            }
                        }
                    }
                    Err(e) => Some(Err(e.clone())),
                };

                futures::future::ready(result)
            },
        ));
        Ok(Box::pin(stream))
    }
}
