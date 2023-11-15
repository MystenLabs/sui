// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove when integrated
#![allow(unused)]

use ethers::types::{Address, U256};
use serde::{Deserialize, Serialize};
use sui_sdk::{SuiClient as SuiClientInner, SuiClientBuilder};
use sui_types::base_types::SuiAddress;

use crate::error::BridgeResult;

pub(crate) struct SuiClient {
    inner: SuiClientInner,
}

impl SuiClient {
    pub async fn new(rpc_url: &str) -> anyhow::Result<Self> {
        let inner = SuiClientBuilder::default().build(rpc_url).await?;
        let self_ = Self { inner };
        self_.describe().await?;
        Ok(self_)
    }

    // TODO assert chain identifier
    async fn describe(&self) -> anyhow::Result<()> {
        let chain_id = self.inner.read_api().get_chain_identifier().await?;
        let block_number = self
            .inner
            .read_api()
            .get_latest_checkpoint_sequence_number()
            .await?;
        tracing::info!(
            "SuiClient is connected to chain {chain_id}, current block number: {block_number}"
        );
        Ok(())
    }

    pub async fn get_bridge_events_maybe(
        &self,
        tx_digest: &str,
    ) -> BridgeResult<Vec<SuiBridgeEvent>> {
        unimplemented!()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuiToEthBridgeEvent {
    pub source_address: SuiAddress,
    pub destination_address: Address,
    pub coin_name: String,
    pub amount: U256,
}

pub enum SuiBridgeEvent {
    SuiToEthBridge(SuiToEthBridgeEvent),
}
