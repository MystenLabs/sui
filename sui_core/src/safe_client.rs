// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::{AuthorityAPI, BatchInfoResponseItemStream};
use async_trait::async_trait;
use futures::StreamExt;
use std::io;
use sui_types::crypto::PublicKeyBytes;
use sui_types::{base_types::*, committee::*, fp_ensure};

use sui_types::batch::{AuthorityBatch, SignedBatch, TxSequenceNumber, UpdateItem};
use sui_types::{
    error::{SuiError, SuiResult},
    messages::*,
};

#[derive(Clone)]
pub struct SafeClient<C> {
    authority_client: C,
    committee: Committee,
    address: PublicKeyBytes,
}

impl<C> SafeClient<C> {
    pub fn new(authority_client: C, committee: Committee, address: PublicKeyBytes) -> Self {
        Self {
            authority_client,
            committee,
            address,
        }
    }

    // Here we centralize all checks for transaction info responses
    fn check_transaction_response(
        &self,
        digest: TransactionDigest,
        response: &TransactionInfoResponse,
    ) -> SuiResult {
        if let Some(signed_transaction) = &response.signed_transaction {
            // Check the transaction signature
            signed_transaction.check(&self.committee)?;
            // Check it has the right signer
            fp_ensure!(
                signed_transaction.auth_sign_info.authority == self.address,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
            // Check it's the right transaction
            fp_ensure!(
                signed_transaction.digest() == digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
        }

        if let Some(certificate) = &response.certified_transaction {
            // Check signatures and quorum
            certificate.check(&self.committee)?;
            // Check it's the right transaction
            fp_ensure!(
                certificate.transaction.digest() == digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
        }

        if let Some(signed_effects) = &response.signed_effects {
            // Check signature
            signed_effects
                .auth_signature
                .signature
                .check(&signed_effects.effects, self.address)?;
            // Checks it concerns the right tx
            fp_ensure!(
                signed_effects.effects.transaction_digest == digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
            // Check it has the right signer
            fp_ensure!(
                signed_effects.auth_signature.authority == self.address,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
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
            certificate.check(&self.committee)?;
        }

        // Check the right object ID and version is returned
        if let Some((object_id, version, _)) = &response.requested_object_reference {
            fp_ensure!(
                object_id == &request.object_id,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
            if let ObjectInfoRequestKind::PastObjectInfo(requested_version) = &request.request_kind
            {
                fp_ensure!(
                    version == requested_version,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address
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
                    authority: self.address
                }
            );

            match response.requested_object_reference {
                Some(obj_ref) => {
                    // Since we are requesting the latest version, we should validate that if the object's
                    // reference actually match with the one from the responded object reference.
                    fp_ensure!(
                        object_and_lock.object.compute_object_reference() == obj_ref,
                        SuiError::ByzantineAuthoritySuspicion {
                            authority: self.address
                        }
                    );
                }
                None => {
                    // Since we are returning the object for the latest version,
                    // we must also have the requested object reference in the response.
                    // Otherwise the authority has inconsistent data.
                    return Err(SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address,
                    });
                }
            };

            if let Some(signed_transaction) = &object_and_lock.lock {
                signed_transaction.check(&self.committee)?;
                // Check it has the right signer
                fp_ensure!(
                    signed_transaction.auth_sign_info.authority == self.address,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address
                    }
                );
            }
        }

        Ok(())
    }

    fn check_update_item_batch_response(
        &self,
        request: BatchInfoRequest,
        signed_batch: &SignedBatch,
        transactions_and_last_batch: &Option<(
            Vec<(TxSequenceNumber, TransactionDigest)>,
            AuthorityBatch,
        )>,
    ) -> SuiResult {
        // check the signature of the batch
        signed_batch
            .signature
            .check(&signed_batch.batch, signed_batch.authority)?;

        // ensure transactions enclosed match requested range
        fp_ensure!(
            signed_batch.batch.initial_sequence_number >= request.start
                && signed_batch.batch.next_sequence_number
                    <= (request.end + signed_batch.batch.size),
            SuiError::ByzantineAuthoritySuspicion {
                authority: self.address
            }
        );

        // If we have seen a previous batch, use it to make sure the next batch
        // is constructed correctly:

        if let Some((transactions, prev_batch)) = transactions_and_last_batch {
            let reconstructed_batch = AuthorityBatch::make_next(prev_batch, transactions)?;

            fp_ensure!(
                reconstructed_batch == signed_batch.batch,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
        }

        Ok(())
    }

    /// This function is used by the higher level authority logic to report an
    /// error that could be due to this authority.
    pub fn report_client_error(&self, _error: SuiError) {
        // TODO: At a minimum we log this error along the authority name, and potentially
        // in case of strong evidence of byzantine behaviour we could share this error
        // with the rest of the network, or de-prioritize requests to this authority given
        // weaker evidence.
    }
}

impl<C> SafeClient<C>
where
    C: AuthorityAPI + Send + Sync + Clone + 'static,
{
    /// Uses the follower API and augments each digest received with a full transactions info structure.
    pub async fn handle_transaction_info_request_to_transaction_info(
        &self,
        request: BatchInfoRequest,
    ) -> Result<
        impl futures::Stream<Item = Result<(u64, TransactionInfoResponse), SuiError>> + '_,
        SuiError,
    > {
        let new_stream = self
            .handle_batch_stream(request)
            .await
            .map_err(|err| SuiError::GenericAuthorityError {
                error: format!("Stream error: {:?}", err),
            })?
            .filter_map(|item| {
                let _client = self.clone();
                async move {
                    match &item {
                        Ok(BatchInfoResponseItem(UpdateItem::Batch(_signed_batch))) => None,
                        Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digest)))) => {
                            // Download the full transaction info
                            let transaction_info_request = TransactionInfoRequest::from(*digest);
                            let res = _client
                                .handle_transaction_info_request(transaction_info_request)
                                .await
                                .map(|v| (*seq, v));
                            Some(res)
                        }
                        Err(err) => Some(Err(err.clone())),
                    }
                }
            });

        Ok(new_stream)
    }
}

#[async_trait]
impl<C> AuthorityAPI for SafeClient<C>
where
    C: AuthorityAPI + Send + Sync + Clone + 'static,
{
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let digest = transaction.digest();
        let transaction_info = self
            .authority_client
            .handle_transaction(transaction)
            .await?;
        if let Err(err) = self.check_transaction_response(digest, &transaction_info) {
            self.report_client_error(err.clone());
            return Err(err);
        }
        Ok(transaction_info)
    }

    /// Confirm a transfer to a Sui or Primary account.
    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let digest = transaction.certificate.transaction.digest();
        let transaction_info = self
            .authority_client
            .handle_confirmation_transaction(transaction)
            .await?;

        if let Err(err) = self.check_transaction_response(digest, &transaction_info) {
            self.report_client_error(err.clone());
            return Err(err);
        }
        Ok(transaction_info)
    }

    async fn handle_consensus_transaction(
        &self,
        transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        // TODO: Add safety checks on the response.
        self.authority_client
            .handle_consensus_transaction(transaction)
            .await
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        self.authority_client
            .handle_account_info_request(request)
            .await
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let response = self
            .authority_client
            .handle_object_info_request(request.clone())
            .await?;
        if let Err(err) = self.check_object_response(&request, &response) {
            self.report_client_error(err.clone());
            return Err(err);
        }
        Ok(response)
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let digest = request.transaction_digest;
        let transaction_info = self
            .authority_client
            .handle_transaction_info_request(request)
            .await?;

        if let Err(err) = self.check_transaction_response(digest, &transaction_info) {
            self.report_client_error(err.clone());
            return Err(err);
        }
        Ok(transaction_info)
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, io::Error> {
        let batch_info_items = self
            .authority_client
            .handle_batch_stream(request.clone())
            .await?;

        let client = self.clone();
        let address = self.address;
        let stream = Box::pin(batch_info_items.scan(
            (0u64, None),
            move |(seq, txs_and_last_batch), batch_info_item| {
                let req_clone = request.clone();
                let client = client.clone();

                // We check if we have exceeded the batch boundary for this request.
                if *seq >= request.end {
                    // If we exceed it return None to end stream
                    return futures::future::ready(None);
                }

                let x = match &batch_info_item {
                    Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch))) => {
                        if let Err(err) = client.check_update_item_batch_response(
                            req_clone,
                            signed_batch,
                            txs_and_last_batch,
                        ) {
                            client.report_client_error(err.clone());
                            Some(Err(err))
                        } else {
                            // Save the sequence number of this batch
                            *seq = signed_batch.batch.next_sequence_number;
                            // Insert a fresh vector for the new batch of transactions
                            let _ =
                                txs_and_last_batch.insert((Vec::new(), signed_batch.batch.clone()));
                            Some(batch_info_item)
                        }
                    }
                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digest)))) => {
                        // A stream always starts with a batch, so the previous should have initialized it.
                        // And here we insert the tuple into the batch.
                        if txs_and_last_batch
                            .as_mut()
                            .map(|txs| txs.0.push((*seq, *digest)))
                            .is_none()
                        {
                            let err = SuiError::ByzantineAuthoritySuspicion { authority: address };
                            client.report_client_error(err.clone());
                            Some(Err(err))
                        } else {
                            Some(batch_info_item)
                        }
                    }
                    Err(e) => Some(Err(e.clone())),
                };

                futures::future::ready(x)
            },
        ));
        Ok(Box::pin(stream))
    }
}
