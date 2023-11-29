// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove when integrated
#![allow(unused)]

use crate::abi::example_contract::ExampleContractEvents;
use crate::abi::EthBridgeEvent;
use crate::error::{BridgeError, BridgeResult};
use ethers::providers::{Http, Middleware, Provider};

pub(crate) struct EthClient {
    provider: Provider<Http>,
}

impl EthClient {
    pub async fn new(provider_url: &str) -> anyhow::Result<Self> {
        let provider = Provider::<Http>::try_from(provider_url)?;
        let self_ = Self { provider };
        self_.describe().await?;
        Ok(self_)
    }

    // TODO assert chain identifier
    async fn describe(&self) -> anyhow::Result<()> {
        let chain_id = self.provider.get_chainid().await?;
        let block_number = self.provider.get_block_number().await?;
        tracing::info!(
            "EthClient is connected to chain {chain_id}, current block number: {block_number}"
        );
        Ok(())
    }

    pub async fn get_bridge_events_maybe(
        &self,
        tx_hash: &str,
    ) -> BridgeResult<Vec<EthBridgeEvent>> {
        unimplemented!()
    }
}
