// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::BridgeError;
use crate::eth_client::EthClient;
use crate::sui_client::SuiClient;
use crate::types::SignedBridgeAction;
use async_trait::async_trait;
use axum::Json;
use sui_sdk::SuiClient as SuiSdkClient;

#[async_trait]
pub trait BridgeRequestHandlerTrait {
    /// Handles a request to sign a BridgeAction that bridges assets
    /// from Ethereum to Sui. The inputs are a transaction hash on Ethereum
    /// that emitted the bridge event and the Event index in that transaction
    async fn handle_eth_tx_hash(
        &self,
        tx_hash_hex: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError>;
    /// Handles a request to sign a BridgeAction that bridges assets
    /// from Sui to Ethereum. The inputs are a transaction digest on Sui
    /// that emitted the bridge event and the Event index in that transaction
    async fn handle_sui_tx_digest(
        &self,
        tx_digest_base58: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError>;
}

// TODO: reconfig?
pub struct BridgeRequestHandler {
    _eth_client: EthClient<ethers::providers::Http>,
    _sui_client: SuiClient<SuiSdkClient>,
}

#[allow(clippy::new_without_default)]
impl BridgeRequestHandler {
    pub fn new() -> Self {
        unimplemented!()
    }
}

#[async_trait]
impl BridgeRequestHandlerTrait for BridgeRequestHandler {
    async fn handle_eth_tx_hash(
        &self,
        _tx_hash_hex: String,
        _event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        unimplemented!()
    }

    async fn handle_sui_tx_digest(
        &self,
        _tx_digest_base58: String,
        _event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        unimplemented!()
    }
}
