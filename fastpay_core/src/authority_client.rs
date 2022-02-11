// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use fastx_network::network::NetworkClient;
use fastx_types::{error::SuiError, messages::*, serialize::*};

#[async_trait]
pub trait AuthorityAPI {
    /// Initiate a new order to a Sui or Primary account.
    async fn handle_order(&self, order: Order) -> Result<OrderInfoResponse, SuiError>;

    /// Confirm an order to a Sui or Primary account.
    async fn handle_confirmation_order(
        &self,
        order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, SuiError>;

    /// Handle Account information requests for this account.
    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError>;

    /// Handle Object information requests for this account.
    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError>;

    /// Handle Object information requests for this account.
    async fn handle_order_info_request(
        &self,
        request: OrderInfoRequest,
    ) -> Result<OrderInfoResponse, SuiError>;
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
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_order(&self, order: Order) -> Result<OrderInfoResponse, SuiError> {
        let response = self.0.send_recv_bytes(serialize_order(&order)).await?;
        deserialize_order_info(response)
    }

    /// Confirm a transfer to a Sui or Primary account.
    async fn handle_confirmation_order(
        &self,
        order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_cert(&order.certificate))
            .await?;
        deserialize_order_info(response)
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_account_info_request(&request))
            .await?;
        deserialize_account_info(response)
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
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
    ) -> Result<OrderInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_order_info_request(&request))
            .await?;
        deserialize_order_info(response)
    }
}
