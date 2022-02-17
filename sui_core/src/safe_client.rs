// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use async_trait::async_trait;
use sui_types::crypto::PublicKeyBytes;
use sui_types::{base_types::*, committee::*, fp_ensure};

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

    // Here we centralize all checks for order info responses
    fn check_order_response(
        &self,
        digest: TransactionDigest,
        response: &OrderInfoResponse,
    ) -> SuiResult {
        if let Some(signed_order) = &response.signed_order {
            // Check the order signature
            signed_order.check(&self.committee)?;
            // Check it has the right signer
            fp_ensure!(
                signed_order.authority == self.address,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
            // Check it's the right order
            fp_ensure!(
                signed_order.order.digest() == digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
        }

        if let Some(certificate) = &response.certified_order {
            // Check signatures and quorum
            certificate.check(&self.committee)?;
            // Check it's the right order
            fp_ensure!(
                certificate.order.digest() == digest,
                SuiError::ByzantineAuthoritySuspicion {
                    authority: self.address
                }
            );
        }

        if let Some(signed_effects) = &response.signed_effects {
            // Check signature
            signed_effects
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
                signed_effects.authority == self.address,
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

        // Check the right version is returned
        if let Some(requested_version) = &request.request_sequence_number {
            if let Some(object_ref) = &response.requested_object_reference {
                fp_ensure!(
                    object_ref.1 == *requested_version,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address
                    }
                );
            }
        }

        // If an order lock is returned it is valid.
        if let Some(object_and_lock) = &response.object_and_lock {
            if let Some(signed_order) = &object_and_lock.lock {
                signed_order.check(&self.committee)?;
                // Check it has the right signer
                fp_ensure!(
                    signed_order.authority == self.address,
                    SuiError::ByzantineAuthoritySuspicion {
                        authority: self.address
                    }
                );
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<C> AuthorityAPI for SafeClient<C>
where
    C: AuthorityAPI + Send + Sync + Clone + 'static,
{
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_order(&self, order: Order) -> Result<OrderInfoResponse, SuiError> {
        let digest = order.digest();
        let order_info = self.authority_client.handle_order(order).await?;
        self.check_order_response(digest, &order_info)?;
        Ok(order_info)
    }

    /// Confirm a transfer to a Sui or Primary account.
    async fn handle_confirmation_order(
        &self,
        order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, SuiError> {
        let digest = order.certificate.order.digest();
        let order_info = self
            .authority_client
            .handle_confirmation_order(order)
            .await?;
        self.check_order_response(digest, &order_info)?;
        Ok(order_info)
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
        self.check_object_response(&request, &response)?;
        Ok(response)
    }

    /// Handle Object information requests for this account.
    async fn handle_order_info_request(
        &self,
        request: OrderInfoRequest,
    ) -> Result<OrderInfoResponse, SuiError> {
        let digest = request.transaction_digest;
        let order_info = self
            .authority_client
            .handle_order_info_request(request)
            .await?;
        self.check_order_response(digest, &order_info)?;
        Ok(order_info)
    }
}
