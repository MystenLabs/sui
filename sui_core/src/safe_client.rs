// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use crate::authority_client::{AuthorityAPI, AuthorityClient, BUFFER_SIZE};
use async_trait::async_trait;
use futures::channel::mpsc::{channel, Receiver};
use futures::Stream;
use futures::{SinkExt, StreamExt};
use std::io;
use std::io::Error;
use std::ops::Deref;
use std::rc::Rc;
use sui_types::crypto::PublicKeyBytes;
use sui_types::{base_types::*, committee::*, fp_ensure};

use sui_types::batch::{SignedBatch, TxSequenceNumber, UpdateItem};
use sui_types::{
    error::{SuiError, SuiResult},
    messages::*,
};

#[derive(Clone)]
pub struct SafeClient{
    authority_client: AuthorityClient,
    committee: Committee,
    address: PublicKeyBytes,
}

impl SafeClient {
    pub fn new(authority_client: AuthorityClient, committee: Committee, address: PublicKeyBytes) -> Self {
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
                signed_transaction.auth_signature.authority == self.address,
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
                    signed_transaction.auth_signature.authority == self.address,
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
    ) -> SuiResult {
        signed_batch
            .signature
            .check(signed_batch, signed_batch.authority)?;

        // ensure transactions enclosed match requested range
        fp_ensure!(
            signed_batch.batch.initial_sequence_number >= request.start &&
            signed_batch.batch.next_sequence_number <= (request.end + signed_batch.batch.size),
            SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address
                    }
        );
        // todo: ensure signature valid over the set of transactions in the batch
        // todo: ensure signature valid over the hash of the previous batch
        Ok(())
    }

    fn check_update_item_transaction_response(
        &self,
        _request: BatchInfoRequest,
        _seq: &TxSequenceNumber,
        _digest: &TransactionDigest,
    ) -> SuiResult {

        todo!();
    }

    /// This function is used by the higher level authority logic to report an
    /// error that could be due to this authority.
    pub fn report_client_error(&self, _error: SuiError) {
        // TODO: At a minimum we log this error along the authority name, and potentially
        // in case of strong evidence of byzantine behaviour we could share this error
        // with the rest of the network, or de-prioritize requests to this authority given
        // weaker evidence.
    }


    /// Handle Batch information requests for this authority.
    async fn handle_batch_streaming(
        &self,
        request: BatchInfoRequest,
    ) -> Result<impl Stream<Item = Result<BatchInfoResponseItem, SuiError>> + '_, io::Error> {
        let mut batch_info_items = self
            .authority_client
            .handle_batch_streaming(request.clone())
            .await?;

        let stream = batch_info_items
            .then( move |batch_info_item| {
                let req_clone = request.clone();
                async move {
                    match &batch_info_item {
                        Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch))) => {
                            if let Err(err) = self.check_update_item_batch_response(
                                req_clone,
                                &signed_batch,
                            ) {
                                self.report_client_error(err.clone());
                                return Err(err);
                            }
                            batch_info_item
                        }
                        Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digest)))) => {
                            if let Err(err) = self.check_update_item_transaction_response(
                                req_clone,
                                seq,
                                digest,
                            ) {
                                self.report_client_error(err.clone());
                                return Err(err);
                            }
                            batch_info_item
                        }
                        Err(e) => {
                            Err(e.clone())
                        }
                    }
                }
            });
        Ok(stream)
    }
}

#[async_trait]
impl AuthorityAPI for SafeClient {
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
}
