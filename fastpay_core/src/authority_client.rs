// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use fastx_network::network::NetworkClient;
use fastx_types::{error::FastPayError, messages::*, serialize::*};

#[async_trait]
pub trait AuthorityAPI {
    /// Initiate a new order to a FastPay or Primary account.
    async fn handle_order(&mut self, order: Order) -> Result<OrderInfoResponse, FastPayError>;

    /// Confirm an order to a FastPay or Primary account.
    async fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, FastPayError>;

    /// Handle Account information requests for this account.
    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError>;

    /// Handle Object information requests for this account.
    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, FastPayError>;

    /// Handle Object information requests for this account.
    async fn handle_order_info_request(
        &self,
        request: OrderInfoRequest,
    ) -> Result<OrderInfoResponse, FastPayError>;
}

#[derive(Clone)]
pub struct AuthorityClient(NetworkClient);

impl AuthorityClient {
    pub fn new(network_client: NetworkClient) -> Self {
        Self(network_client)
    }
}

#[async_trait]
impl AuthorityAPI for AuthorityClient {
    /// Initiate a new transfer to a FastPay or Primary account.
    async fn handle_order(&mut self, order: Order) -> Result<OrderInfoResponse, FastPayError> {
        let response = self.0.send_recv_bytes(serialize_order(&order)).await?;
        deserialize_order_info(response)
    }

    /// Confirm a transfer to a FastPay or Primary account.
    async fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, FastPayError> {
        let response = self
            .0
            .send_recv_bytes(serialize_cert(&order.certificate))
            .await?;
        deserialize_order_info(response)
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError> {
        let response = self
            .0
            .send_recv_bytes(serialize_account_info_request(&request))
            .await?;
        deserialize_account_info(response)
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, FastPayError> {
        let response = self
            .0
            .send_recv_bytes(serialize_object_info_request(&request))
            .await?;
        deserialize_object_info(response)
    }

    /// Handle Object information requests for this account.
    async fn handle_order_info_request(
        &self,
        request: OrderInfoRequest,
    ) -> Result<OrderInfoResponse, FastPayError> {
        let response = self
            .0
            .send_recv_bytes(serialize_order_info_request(&request))
            .await?;
        deserialize_order_info(response)
    }
}
