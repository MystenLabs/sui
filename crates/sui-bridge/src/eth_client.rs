// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    abi::EthBridgeEvent,
    error::{BridgeError, BridgeResult},
    metered_eth_provider::new_metered_eth_provider,
    metrics::BridgeMetrics,
    types::{BridgeAction, EthLog, RawEthLog},
    utils::EthProvider,
};
use alloy::{
    primitives::{Address as EthAddress, TxHash},
    providers::Provider,
    rpc::types::{Block, Filter, Log},
};
use std::{collections::HashSet, sync::Arc};
use tap::TapFallible;

#[cfg(test)]
use crate::eth_mock_provider::EthMockService;

pub struct EthClient {
    provider: EthProvider,
    contract_addresses: HashSet<EthAddress>,
}

impl EthClient {
    pub async fn new(
        provider_url: &str,
        contract_addresses: HashSet<EthAddress>,
        metrics: Arc<BridgeMetrics>,
    ) -> anyhow::Result<Self> {
        let provider = new_metered_eth_provider(provider_url, metrics)?;
        let self_ = Self {
            provider,
            contract_addresses,
        };
        self_.describe().await?;
        Ok(self_)
    }

    pub fn provider(&self) -> EthProvider {
        self.provider.clone()
    }
}

#[cfg(test)]
impl EthClient {
    pub fn new_mocked(
        mock_service: EthMockService,
        contract_addresses: HashSet<EthAddress>,
    ) -> Self {
        let provider = mock_service.as_provider();
        Self {
            provider,
            contract_addresses,
        }
    }
}

impl EthClient {
    pub async fn get_chain_id(&self) -> Result<u64, anyhow::Error> {
        let chain_id = self.provider.get_chain_id().await?;
        Ok(chain_id)
    }

    // TODO assert chain identifier
    async fn describe(&self) -> anyhow::Result<()> {
        let chain_id = self.get_chain_id().await?;
        let block_number = self.provider.get_block_number().await?;
        tracing::info!(
            "EthClient is connected to chain {chain_id}, current block number: {block_number}"
        );
        Ok(())
    }

    /// Returns BridgeAction from an Eth Transaction with transaction hash
    /// and the event index. If event is declared in an unrecognized
    /// contract, return error.
    pub async fn get_finalized_bridge_action_maybe(
        &self,
        tx_hash: TxHash,
        event_idx: u16,
    ) -> BridgeResult<BridgeAction> {
        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(BridgeError::from)?
            .ok_or(BridgeError::TxNotFound)?;
        let receipt_block_num = receipt.block_number.ok_or(BridgeError::ProviderError(
            "Provider returns log without block_number".into(),
        ))?;
        // TODO: save the latest finalized block id so we don't have to query it every time
        let last_finalized_block_id = self.get_last_finalized_block_id().await?;
        if receipt_block_num > last_finalized_block_id {
            return Err(BridgeError::TxNotFinalized);
        }
        let log = receipt
            .logs()
            .get(event_idx as usize)
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;

        // Ignore events emitted from unrecognized contracts
        if !self.contract_addresses.contains(&log.address()) {
            return Err(BridgeError::BridgeEventInUnrecognizedEthContract);
        }

        let eth_log = EthLog {
            block_number: receipt_block_num,
            tx_hash,
            log_index_in_tx: event_idx,
            log: log.clone(),
        };
        let bridge_event = EthBridgeEvent::try_from_eth_log(&eth_log)
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;
        bridge_event
            .try_into_bridge_action(tx_hash, event_idx)?
            .ok_or(BridgeError::BridgeEventNotActionable)
    }

    pub async fn get_last_finalized_block_id(&self) -> BridgeResult<u64> {
        let block: Option<Block> = self
            .provider
            .raw_request("eth_getBlockByNumber".into(), ("finalized", false))
            .await?;
        let block = block.ok_or(BridgeError::TransientProviderError(
            "Provider fails to return last finalized block".into(),
        ))?;
        Ok(block.number())
    }

    // Note: query may fail if range is too big. Callsite is responsible
    // for chunking the query.
    pub async fn get_events_in_range(
        &self,
        address: alloy::primitives::Address,
        start_block: u64,
        end_block: u64,
    ) -> BridgeResult<Vec<EthLog>> {
        let filter = Filter::new()
            .from_block(start_block)
            .to_block(end_block)
            .address(address);
        let logs = self
            .provider
            // TODO use get_logs_paginated?
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

        // Safeguard check that all events are emitted from requested contract address
        if logs.iter().any(|log| log.address() != address) {
            return Err(BridgeError::ProviderError(format!(
                "Provider returns logs from different contract address (expected: {:?}): {:?}",
                address, logs
            )));
        }
        if logs.is_empty() {
            return Ok(vec![]);
        }

        let tasks = logs.into_iter().map(|log| self.get_log_tx_details(log));
        futures::future::join_all(tasks)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| {
                tracing::error!(
                    "get_log_tx_details failed. Filter: {:?}. Error {:?}",
                    filter,
                    e
                )
            })
    }

    // Note: query may fail if range is too big. Callsite is responsible
    // for chunking the query.
    pub async fn get_raw_events_in_range(
        &self,
        addresses: Vec<EthAddress>,
        start_block: u64,
        end_block: u64,
    ) -> BridgeResult<Vec<RawEthLog>> {
        let filter = Filter::new()
            .from_block(start_block)
            .to_block(end_block)
            .address(addresses.clone());
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
        // Safeguard check that all events are emitted from requested contract addresses
        logs.into_iter().map(
            |log| {
                if !addresses.contains(&log.address()) {
                    return Err(BridgeError::ProviderError(format!("Provider returns logs from different contract address (expected: {:?}): {:?}", addresses, log)));
                }
                Ok(RawEthLog {
                block_number: log.block_number.ok_or(BridgeError::ProviderError("Provider returns log without block_number".into()))?,
                tx_hash: log.transaction_hash.ok_or(BridgeError::ProviderError("Provider returns log without transaction_hash".into()))?,
                log,
            })}
        ).collect::<Result<Vec<_>, _>>()
    }

    /// This function converts a `Log` to `EthLog`, to make sure the `block_num`, `tx_hash` and `log_index_in_tx`
    /// are available for downstream.
    // It's frustratingly ugly because of the nulliability of many fields in `Log`.
    async fn get_log_tx_details(&self, log: Log) -> BridgeResult<EthLog> {
        let block_number = log.block_number.ok_or(BridgeError::ProviderError(
            "Provider returns log without block_number".into(),
        ))?;
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
        if receipt_block_num != block_number {
            return Err(BridgeError::ProviderError(format!(
                "Provider returns receipt with different block number from log. Receipt: {:?}, Log: {:?}",
                receipt, log
            )));
        }

        // Find the log index in the transaction
        let mut log_index_in_tx = None;
        for (idx, receipt_log) in receipt.logs().iter().enumerate() {
            // match log index (in the block)
            if receipt_log.log_index == Some(log_index) {
                // make sure the topics and data match
                if receipt_log.topics() != log.topics() || receipt_log.data() != log.data() {
                    return Err(BridgeError::ProviderError(format!(
                        "Provider returns receipt with different log from log. Receipt: {:?}, Log: {:?}",
                        receipt, log
                    )));
                }
                log_index_in_tx = Some(idx);
            }
        }
        let log_index_in_tx = log_index_in_tx.ok_or(BridgeError::ProviderError(format!(
            "Couldn't find matching log: {:?} in transaction {}",
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

#[cfg(test)]
mod tests {
    use alloy::rpc::types::TransactionReceipt;
    use prometheus::Registry;

    use super::*;
    use crate::test_utils::{
        get_test_log_and_action, make_transaction_receipt, mock_last_finalized_block,
    };

    #[tokio::test]
    async fn test_get_finalized_bridge_action_maybe() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let mock_service = EthMockService::new();
        mock_last_finalized_block(&mock_service, 777);

        let client = EthClient::new_mocked(
            mock_service.clone(),
            HashSet::from_iter(vec![EthAddress::default()]),
        );
        let result = client.get_last_finalized_block_id().await.unwrap();
        assert_eq!(result, 777);

        let eth_tx_hash = TxHash::random();
        let log = Log {
            transaction_hash: Some(eth_tx_hash),
            block_number: Some(778),
            ..Default::default()
        };
        let (good_log, bridge_action) =
            get_test_log_and_action(EthAddress::default(), eth_tx_hash, 1);
        // Mocks `eth_getTransactionReceipt` to return `log` and `good_log` in order
        mock_service
            .add_response::<[TxHash; 1], TransactionReceipt, TransactionReceipt>(
                "eth_getTransactionReceipt",
                [log.transaction_hash.unwrap()],
                make_transaction_receipt(
                    EthAddress::default(),
                    log.block_number,
                    vec![log, good_log],
                ),
            )
            .unwrap();

        let error = client
            .get_finalized_bridge_action_maybe(eth_tx_hash, 0)
            .await
            .unwrap_err();
        match error {
            BridgeError::TxNotFinalized => {}
            _ => panic!("expected TxNotFinalized"),
        };

        // 778 is now finalized
        mock_last_finalized_block(&mock_service, 778);

        let error = client
            .get_finalized_bridge_action_maybe(eth_tx_hash, 2)
            .await
            .unwrap_err();
        // Receipt only has 2 logs
        match error {
            BridgeError::NoBridgeEventsInTxPosition => {}
            _ => panic!("expected NoBridgeEventsInTxPosition"),
        };

        let error = client
            .get_finalized_bridge_action_maybe(eth_tx_hash, 0)
            .await
            .unwrap_err();
        // Same, `log` is not a BridgeEvent
        match error {
            BridgeError::NoBridgeEventsInTxPosition => {}
            _ => panic!("expected NoBridgeEventsInTxPosition"),
        };

        let action = client
            .get_finalized_bridge_action_maybe(eth_tx_hash, 1)
            .await
            .unwrap();
        assert_eq!(action, bridge_action);
    }

    #[tokio::test]
    async fn test_get_finalized_bridge_action_maybe_unrecognized_contract() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let mock_service = EthMockService::new();
        mock_last_finalized_block(&mock_service, 777);

        let client = EthClient::new_mocked(
            mock_service.clone(),
            HashSet::from_iter(vec![
                EthAddress::repeat_byte(5),
                EthAddress::repeat_byte(6),
                EthAddress::repeat_byte(7),
            ]),
        );
        let result = client.get_last_finalized_block_id().await.unwrap();
        assert_eq!(result, 777);

        let eth_tx_hash = TxHash::random();
        // Event emitted from a different contract address
        let (log, _bridge_action) =
            get_test_log_and_action(EthAddress::repeat_byte(4), eth_tx_hash, 0);
        mock_service
            .add_response::<[TxHash; 1], TransactionReceipt, TransactionReceipt>(
                "eth_getTransactionReceipt",
                [log.transaction_hash.unwrap()],
                make_transaction_receipt(EthAddress::default(), log.block_number, vec![log]),
            )
            .unwrap();

        let error = client
            .get_finalized_bridge_action_maybe(eth_tx_hash, 0)
            .await
            .unwrap_err();
        match error {
            BridgeError::BridgeEventInUnrecognizedEthContract => {}
            _ => panic!("expected TxNotFinalized"),
        };

        // Ok if emitted from the right contract
        let (log, bridge_action) =
            get_test_log_and_action(EthAddress::repeat_byte(6), eth_tx_hash, 0);
        mock_service
            .add_response::<[TxHash; 1], TransactionReceipt, TransactionReceipt>(
                "eth_getTransactionReceipt",
                [log.transaction_hash.unwrap()],
                make_transaction_receipt(EthAddress::default(), log.block_number, vec![log]),
            )
            .unwrap();
        let action = client
            .get_finalized_bridge_action_maybe(eth_tx_hash, 0)
            .await
            .unwrap();
        assert_eq!(action, bridge_action);
    }
}
