// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::crypto::{BridgeAuthorityKeyPair, BridgeAuthoritySignInfo};
use crate::error::BridgeError;
use crate::eth_client::EthClient;
use crate::sui_client::SuiClient;
use crate::types::SignedBridgeAction;
use async_trait::async_trait;
use axum::Json;
use ethers::types::TxHash;
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
    signer: BridgeAuthorityKeyPair,
    eth_client: EthClient<ethers::providers::Http>,
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
        tx_hash_hex: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        // TODO add caching and avoid simalutaneous requests
        let tx_hash = TxHash::from_str(&tx_hash_hex).map_err(|_| BridgeError::InvalidTxHash)?;
        let bridge_action = self
            .eth_client
            .get_finalized_bridge_action_maybe(tx_hash, event_idx)
            .await?;
        let sig = BridgeAuthoritySignInfo::new(&bridge_action, &self.signer);
        Ok(Json(SignedBridgeAction::new_from_data_and_sig(
            bridge_action,
            sig,
        )))
    }

    async fn handle_sui_tx_digest(
        &self,
        _tx_digest_base58: String,
        _event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        unimplemented!()
    }
}
