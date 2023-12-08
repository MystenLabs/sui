// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove when integrated
#![allow(unused)]

use std::sync::Arc;

use crate::abi::example_contract::ExampleContractEvents;
use crate::abi::EthBridgeEvent;
use crate::error::{BridgeError, BridgeResult};
use ethers::providers::{Http, JsonRpcClient, Middleware, Provider, ProviderError};
use ethers::types::{Block, BlockId, Filter};
use std::str::FromStr;
use tap::{Tap, TapFallible};

#[cfg(test)]
use crate::eth_mock_provider::EthMockProvider;
use ethers::{
    providers::MockProvider,
    types::{U256, U64},
};

pub struct EthClient<P> {
    provider: Provider<P>,
}

impl EthClient<Http> {
    pub async fn new(provider_url: &str) -> anyhow::Result<Self> {
        let provider = Provider::try_from(provider_url)?;
        let self_ = Self { provider };
        self_.describe().await?;
        Ok(self_)
    }
}

#[cfg(test)]
impl EthClient<EthMockProvider> {
    pub async fn new_mocked(provider: EthMockProvider) -> anyhow::Result<Self> {
        let provider = Provider::new(provider);
        let self_ = Self { provider };
        Ok(self_)
    }
}

impl<P> EthClient<P>
where
    P: JsonRpcClient,
{
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

    pub async fn get_last_finalized_block_id(&self) -> BridgeResult<u64> {
        let block: Result<Option<Block<ethers::types::TxHash>>, ethers::prelude::ProviderError> =
            self.provider
                .request("eth_getBlockByNumber", ("finalized", false))
                .await;
        let block = block?.ok_or(BridgeError::TransientProviderError(
            "Provider fails to return last finalized block".into(),
        ))?;
        let number = block.number.ok_or(BridgeError::TransientProviderError(
            "Provider returns block without number".into(),
        ))?;
        Ok(number.as_u64())
    }

    pub async fn get_events_in_range(
        &self,
        address: ethers::types::Address,
        start_block: u64,
        end_block: u64,
    ) -> BridgeResult<Vec<ethers::types::Log>> {
        let filter = Filter::new()
            .from_block(start_block)
            .to_block(end_block)
            .address(address);
        self.provider
            .get_logs(&filter)
            .await
            .map_err(BridgeError::from)
            .tap_err(|e| {
                tracing::error!(
                    "get_events_in_range failed. Filter: {:?}. Error {:?}",
                    filter,
                    e
                )
            })
    }
}
