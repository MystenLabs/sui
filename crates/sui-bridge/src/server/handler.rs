// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use crate::crypto::{BridgeAuthorityKeyPair, BridgeAuthoritySignInfo};
use crate::error::BridgeError;
use crate::eth_client::EthClient;
use crate::sui_client::SuiClient;
use crate::types::SignedBridgeAction;
use async_trait::async_trait;
use axum::Json;
use ethers::types::TxHash;
use sui_sdk::SuiClient as SuiSdkClient;
use sui_types::digests::TransactionDigest;

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

pub struct BridgeRequestHandler {
    signer: BridgeAuthorityKeyPair,
    eth_client: Arc<EthClient<ethers::providers::Http>>,
    sui_client: Arc<SuiClient<SuiSdkClient>>,
}

impl BridgeRequestHandler {
    pub fn new(
        signer: BridgeAuthorityKeyPair,
        sui_client: Arc<SuiClient<SuiSdkClient>>,
        eth_client: Arc<EthClient<ethers::providers::Http>>,
    ) -> Self {
        Self {
            signer,
            eth_client,
            sui_client,
        }
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
        tx_digest_base58: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        // TODO add caching and avoid simultaneous requests
        let tx_digest = TransactionDigest::from_str(&tx_digest_base58)
            .map_err(|_e| BridgeError::InvalidTxHash)?;
        let bridge_action = self
            .sui_client
            .get_bridge_action_by_tx_digest_and_event_idx(&tx_digest, event_idx)
            .await?;
        let sig = BridgeAuthoritySignInfo::new(&bridge_action, &self.signer);
        Ok(Json(SignedBridgeAction::new_from_data_and_sig(
            bridge_action,
            sig,
        )))
    }
}
