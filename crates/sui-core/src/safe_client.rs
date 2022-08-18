// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::{AuthorityAPI, BatchInfoResponseItemStream};
use crate::epoch::epoch_store::EpochStore;
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
    AuthenticatedCheckpoint, AuthorityCheckpointInfo, CheckpointContents, CheckpointRequest,
    CheckpointRequestType, CheckpointResponse, CheckpointSequenceNumber,
};
use sui_types::{base_types::*, committee::*, fp_ensure};
use sui_types::{
    error::{SuiError, SuiResult},
    messages::*,
};
use tap::TapFallible;
use tracing::info;

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone, Debug)]
pub struct SafeClientMetrics {
    pub(crate) total_requests_by_address_method: IntCounterVec,
    pub(crate) total_responses_by_address_method: IntCounterVec,
    pub(crate) follower_streaming_from_seq_number_by_address: IntGaugeVec,
    pub(crate) follower_streaming_reconnect_times_by_address: IntCounterVec,
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
    epoch_store: Arc<EpochStore>,
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
}

impl<C> SafeClient<C> {
    pub fn new(
        authority_client: C,
        epoch_store: Arc<EpochStore>,
        address: AuthorityPublicKeyBytes,
        safe_client_metrics: SafeClientMetrics,
    ) -> Self {
        // Cache counters for efficiency
        let validator_address = address.to_string();
        let requests_metrics_vec = safe_client_metrics.total_requests_by_address_method;
        let responses_metrics_vec = safe_client_metrics.total_responses_by_address_method;

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

        Self {
            authority_client,
            epoch_store,
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
        match self.epoch_store.get_authenticated_epoch(epoch_id)? {
            Some(epoch_info) => Ok(epoch_info.into_epoch_info().into_committee()),
            None => Err(SuiError::InvalidAuthenticatedEpoch(format!(
                "Epoch info not found in the store for epoch {:?}",
                epoch_id
            ))),
        }
    }

    // Here we centralize all checks for transaction info responses
    fn check_transaction_response(
        &self,
        digest: &TransactionDigest,
        effects_digest: Option<&TransactionEffectsDigest>,
        response: &TransactionInfoResponse,
    ) -> SuiResult {
        let mut committee = None;
        if let Some(signed_transaction) = &response.signed_transaction {
            committee = Some(self.get_committee(&signed_transaction.auth_sig().epoch)?);
            // Check the transaction signature
            signed_transaction.verify(committee.as_ref().unwrap())?;
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
        }

        if let Some(certificate) = &response.certified_transaction {
            if committee.is_none() {
                committee = Some(self.get_committee(&certificate.auth_sig().epoch)?);
            }
            // Check signatures and quorum
            certificate.verify(committee.as_ref().unwrap())?;
            // Check it's the right transaction
            fp_ensure!(
                certificate.digest() == digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: "Unexpected digest in the certified tx".to_string()
                }
            );
        }

        if let Some(signed_effects) = &response.signed_effects {
            if committee.is_none() {
                committee = Some(self.get_committee(&signed_effects.auth_signature.epoch)?);
            }
            // Check signature
            signed_effects.verify(committee.as_ref().unwrap())?;
            // Check it has the right signer
            fp_ensure!(
                signed_effects.auth_signature.authority == self.address,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address,
                    reason: "Unexpected validator address in the signed effects signature"
                        .to_string()
                }
            );
            // Checks it concerns the right tx
            fp_ensure!(
                signed_effects.effects().transaction_digest == *digest,
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

        Ok(())
    }

    fn check_object_response(
        &self,
        request: &ObjectInfoRequest,
        response: &ObjectInfoResponse,
    ) -> SuiResult {
        // If we get a certificate make sure it is a valid certificate
        if let Some(certificate) = &response.parent_certificate {
            certificate.verify(&self.get_committee(&certificate.auth_sig().epoch)?)?;
        }

        // Check the right object ID and version is returned
        if let Some((object_id, version, _)) = &response.requested_object_reference {
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

        if let Some(object_and_lock) = &response.object_and_lock {
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

            match response.requested_object_reference {
                Some(obj_ref) => {
                    // Since we are requesting the latest version, we should validate that if the object's
                    // reference actually match with the one from the responded object reference.
                    fp_ensure!(
                        object_and_lock.object.compute_object_reference() == obj_ref,
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

            if let Some(signed_transaction) = &object_and_lock.lock {
                // We cannot reuse the committee fetched above since they may not be from the same
                // epoch.
                signed_transaction
                    .verify(&self.get_committee(&signed_transaction.auth_sig().epoch)?)?;
                // Check it has the right signer
                fp_ensure!(
                    signed_transaction.auth_sig().authority == self.address,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                        reason: "Unexpected validator address in the signed tx signature"
                            .to_string()
                    }
                );
            }
        }

        Ok(())
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
        signed_batch.verify(&self.get_committee(&signed_batch.auth_sig().epoch)?)?;

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
                    reason: "Inconsistent batch".to_string()
                }
            );
        }

        Ok(())
    }

    /// This function is used by the higher level authority logic to report an
    /// error that could be due to this authority.
    /// TODO: Get rid of this. https://github.com/MystenLabs/sui/issues/3740
    pub fn report_client_error(&self, error: &SuiError) {
        info!(?error, authority =? self.address, "Client error");
    }
}

impl<C> SafeClient<C>
where
    C: AuthorityAPI + Send + Sync + Clone + 'static,
{
    /// Initiate a new transfer to a Sui or Primary account.
    pub async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let digest = *transaction.digest();
        let transaction_info = self
            .authority_client
            .handle_transaction(transaction)
            .await?;
        if let Err(err) = self.check_transaction_response(&digest, None, &transaction_info) {
            self.report_client_error(&err);
            return Err(err);
        }
        Ok(transaction_info)
    }

    fn verify_certificate_response(
        &self,
        digest: &TransactionDigest,
        response: &TransactionInfoResponse,
    ) -> SuiResult {
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
    ) -> Result<TransactionInfoResponse, SuiError> {
        let digest = *certificate.digest();
        let transaction_info = self
            .authority_client
            .handle_certificate(certificate)
            .await?;

        if let Err(err) = self.verify_certificate_response(&digest, &transaction_info) {
            self.report_client_error(&err);
            return Err(err);
        }
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

    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        self.metrics_total_requests_handle_object_info_request.inc();
        let response = self
            .authority_client
            .handle_object_info_request(request.clone())
            .await?;
        if let Err(err) = self.check_object_response(&request, &response) {
            self.report_client_error(&err);
            return Err(err);
        }
        self.metrics_total_ok_responses_handle_object_info_request
            .inc();
        Ok(response)
    }

    /// Handle Transaction information requests for this account.
    pub async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.metrics_total_requests_handle_transaction_info_request
            .inc();
        let digest = request.transaction_digest;
        let transaction_info = self
            .authority_client
            .handle_transaction_info_request(request)
            .await?;

        if let Err(err) = self.check_transaction_response(&digest, None, &transaction_info) {
            self.report_client_error(&err);
            return Err(err);
        }
        self.metrics_total_ok_responses_handle_transaction_info_request
            .inc();
        Ok(transaction_info)
    }

    /// Handle Transaction + Effects information requests for this account.
    pub async fn handle_transaction_and_effects_info_request(
        &self,
        digests: &ExecutionDigests,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.metrics_total_requests_handle_transaction_and_effects_info_request
            .inc();
        let transaction_info = self
            .authority_client
            .handle_transaction_info_request(digests.transaction.into())
            .await?;

        if let Err(err) = self.check_transaction_response(
            &digests.transaction,
            Some(&digests.effects),
            &transaction_info,
        ) {
            self.report_client_error(&err);
            return Err(err);
        }
        self.metrics_total_ok_responses_handle_transaction_and_effects_info_request
            .inc();
        Ok(transaction_info)
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

    fn verify_contents_exist<T>(
        &self,
        request_content: bool,
        checkpoint: &Option<T>,
        contents: &Option<CheckpointContents>,
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
                if let AuthorityCheckpointInfo::AuthenticatedCheckpoint(checkpoint) = &response.info
                {
                    // Checks that the sequence number is correct.
                    self.verify_checkpoint_sequence(*seq, checkpoint)?;
                    self.verify_contents_exist(request.detail, checkpoint, &response.detail)?;
                    // Verify signature.
                    match checkpoint {
                        Some(c) => {
                            let epoch_id = c.summary().epoch;
                            c.verify(&self.get_committee(&epoch_id)?, response.detail.as_ref())
                        }
                        None => Ok(()),
                    }
                } else {
                    Err(SuiError::from(
                        "Invalid AuthorityCheckpointInfo type in the response",
                    ))
                }
            }
            CheckpointRequestType::CheckpointProposal => {
                if let AuthorityCheckpointInfo::CheckpointProposal {
                    proposal,
                    prev_cert,
                } = &response.info
                {
                    // Verify signature.
                    if let Some(signed_proposal) = proposal {
                        let mut committee =
                            self.get_committee(&signed_proposal.auth_signature.epoch)?;
                        signed_proposal.verify(&committee, response.detail.as_ref())?;
                        if signed_proposal.summary.sequence_number > 0 {
                            let cert = prev_cert.as_ref().ok_or_else(|| {
                                SuiError::from("No checkpoint cert provided along with proposal")
                            })?;
                            if cert.auth_signature.epoch != signed_proposal.auth_signature.epoch {
                                // It's possible that the previous checkpoint cert is from the
                                // previous epoch, and in that case we verify them using different
                                // committee.
                                fp_ensure!(
                                    cert.auth_signature.epoch + 1
                                        == signed_proposal.auth_signature.epoch,
                                    SuiError::from("Unexpected epoch for checkpoint cert")
                                );
                                committee = self.get_committee(&cert.auth_signature.epoch)?;
                            }
                            cert.verify(&committee, None)?;
                            fp_ensure!(
                                signed_proposal.summary.sequence_number - 1 == cert.summary.sequence_number,
                                SuiError::from("Checkpoint proposal sequence number inconsistent with previous cert")
                            );
                        }
                    }
                    self.verify_contents_exist(request.detail, proposal, &response.detail)
                } else {
                    Err(SuiError::from(
                        "Invalid AuthorityCheckpointInfo type in the response",
                    ))
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
            .map_err(|err| {
                self.report_client_error(&err);
                err
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
                            client.report_client_error(&err);
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
                                client.report_client_error(&err);
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

    fn verify_epoch(
        &self,
        requested_epoch_id: Option<EpochId>,
        response: &EpochResponse,
    ) -> SuiResult {
        if let Some(epoch) = &response.epoch_info {
            fp_ensure!(
                requested_epoch_id.is_none() || requested_epoch_id == Some(epoch.epoch()),
                SuiError::InvalidEpochResponse("Responded epoch number mismatch".to_string())
            );
        }
        match (requested_epoch_id, &response.epoch_info) {
            (None, None) => Err(SuiError::InvalidEpochResponse(
                "Latest epoch must not be None".to_string(),
            )),
            (Some(epoch_id), None) => {
                fp_ensure!(
                    epoch_id != 0,
                    SuiError::InvalidEpochResponse("Genesis epoch must be available".to_string())
                );
                Ok(())
            }
            (_, Some(AuthenticatedEpoch::Genesis(g))) => g.verify(&self.get_committee(&0)?),
            (_, Some(AuthenticatedEpoch::Signed(s))) => {
                s.verify(&self.get_committee(&s.auth_signature.epoch)?)
            }
            (_, Some(AuthenticatedEpoch::Certified(c))) => {
                c.verify(&self.get_committee(&c.auth_signature.epoch)?)
            }
        }
    }

    pub async fn handle_epoch(&self, request: EpochRequest) -> Result<EpochResponse, SuiError> {
        let epoch_id = request.epoch_id;
        let authority = self.address;
        let response = self.authority_client.handle_epoch(request).await?;
        self.verify_epoch(epoch_id, &response)
            .map_err(|err| SuiError::ByzantineAuthoritySuspicion {
                authority,
                reason: err.to_string(),
            })
            .tap_err(|err| {
                self.report_client_error(err);
            })?;
        Ok(response)
    }
}
