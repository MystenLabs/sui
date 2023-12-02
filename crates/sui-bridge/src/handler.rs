// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::BridgeError;
use crate::eth_client::EthClient;
use crate::sui_client::SuiClient;
use axum::Json;
use sui_sdk::SuiClient as SuiSdkClient;

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

    pub async fn handle_eth_tx_hash(
        &self,
        _tx_hash_hex: String,
    ) -> Result<Json<String>, BridgeError> {
        unimplemented!()
    }

    pub async fn handle_sui_tx_digest(
        &self,
        _tx_digest_base58: String,
    ) -> Result<Json<String>, BridgeError> {
        unimplemented!()
    }
}
