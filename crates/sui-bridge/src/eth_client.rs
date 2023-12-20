// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove when integrated
#![allow(unused)]

use std::sync::Arc;

use crate::abi::example_contract::ExampleContractEvents;
use crate::abi::EthBridgeEvent;
use crate::error::{BridgeError, BridgeResult};
use crate::types::EthLog;
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

    // TODO: this needs some pagination if the range is too big
    pub async fn get_events_in_range(
        &self,
        address: ethers::types::Address,
        start_block: u64,
        end_block: u64,
    ) -> BridgeResult<Vec<EthLog>> {
        let filter = Filter::new()
            .from_block(start_block)
            .to_block(end_block)
            .address(address);
        let logs = self
            .provider
            .get_logs(&filter)
            .await
            .map_err(BridgeError::from)
            .tap_err(|e| {
                tracing::error!(
                    "get_events_in_range failed. Filter: {:?}. Error {:?}",
                    filter,
                    e
                )
            })?;
        if logs.is_empty() {
            return Ok(vec![]);
        }
        let tasks = logs.into_iter().map(|log| self.get_log_tx_details(log));
        let results = futures::future::join_all(tasks)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| {
                tracing::error!(
                    "get_log_tx_details failed. Filter: {:?}. Error {:?}",
                    filter,
                    e
                )
            })?;
        Ok(results)
    }

    /// This function converts a `Log` to `EthLog`, to make sure the `block_num`, `tx_hash` and `log_index_in_tx`
    /// are available for downstream.
    // It's frustratingly ugly because of the nulliability of many fields in `Log`.
    async fn get_log_tx_details(&self, log: ethers::types::Log) -> BridgeResult<EthLog> {
        let block_number = log
            .block_number
            .ok_or(BridgeError::ProviderError(
                "Provider returns log without block_number".into(),
            ))?
            .as_u64();
        let tx_hash = log.transaction_hash.ok_or(BridgeError::ProviderError(
            "Provider returns log without transaction_hash".into(),
        ))?;
        // This is the log index in the block, rather than transaction.
        let log_index = log.log_index.ok_or(BridgeError::ProviderError(
            "Provider returns log without log_index".into(),
        ))?;

        // Now get the log's index in the transaction. There is `transaction_log_index` field in
        // `Log`, but I never saw it populated.

        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(BridgeError::from)?
            .ok_or(BridgeError::ProviderError(format!(
                "Provide cannot find eth transaction for log: {:?})",
                log
            )))?;

        let receipt_block_num = receipt.block_number.ok_or(BridgeError::ProviderError(
            "Provider returns log without block_number".into(),
        ))?;
        if receipt_block_num.as_u64() != block_number {
            return Err(BridgeError::ProviderError(format!("Provider returns receipt with different block number from log. Receipt: {:?}, Log: {:?}", receipt, log)));
        }

        // Find the log index in the transaction
        let mut log_index_in_tx = None;
        for (idx, receipt_log) in receipt.logs.iter().enumerate() {
            // match log index (in the block)
            if receipt_log.log_index == Some(log_index) {
                // make sure the topics and data match
                if receipt_log.topics != log.topics || receipt_log.data != log.data {
                    return Err(BridgeError::ProviderError(format!("Provider returns receipt with different log from log. Receipt: {:?}, Log: {:?}", receipt, log)));
                }
                log_index_in_tx = Some(idx);
            }
        }
        let log_index_in_tx = log_index_in_tx.ok_or(BridgeError::ProviderError(format!(
            "Couldn't find matches log {:?} in transaction {}",
            log, tx_hash
        )))?;

        Ok(EthLog {
            block_number,
            tx_hash,
            log_index_in_tx: log_index_in_tx as u16,
            log,
        })
    }
}
