// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::operations::Operations;
use crate::types::{
    Block, BlockHash, BlockIdentifier, BlockResponse, OperationStatus, OperationType, Transaction,
    TransactionIdentifier,
};
use crate::Error;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mysten_metrics::spawn_monitored_task;
use rocksdb::Options;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::rpc_types::Checkpoint;
use sui_sdk::SuiClient;
use sui_types::base_types::{EpochId, SuiAddress};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tracing::{debug, error, info, warn};
use typed_store::rocks::{point_lookup_db_options, DBMap, DBOptions, MetricConf};
use typed_store::traits::TableSummary;
use typed_store::traits::TypedStoreDebug;
use typed_store::Map;
use typed_store_derive::DBMapUtils;

#[cfg(test)]
#[path = "unit_tests/balance_changing_tx_tests.rs"]
mod balance_changing_tx_tests;

#[derive(Clone)]
pub struct OnlineServerContext {
    pub client: SuiClient,
    block_provider: Arc<dyn BlockProvider + Send + Sync>,
}

impl OnlineServerContext {
    pub fn new(client: SuiClient, block_provider: Arc<dyn BlockProvider + Send + Sync>) -> Self {
        Self {
            client,
            block_provider,
        }
    }

    pub fn blocks(&self) -> &(dyn BlockProvider + Sync + Send) {
        &*self.block_provider
    }
}

#[async_trait]
pub trait BlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error>;
    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error>;
    async fn current_block(&self) -> Result<BlockResponse, Error>;
    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn get_balance_at_block(
        &self,
        addr: SuiAddress,
        block_height: u64,
    ) -> Result<i128, Error>;
}

#[derive(Clone)]
pub struct CheckpointBlockProvider {
    index_store: Arc<CheckpointIndexStore>,
    client: SuiClient,
}

#[async_trait]
impl BlockProvider for CheckpointBlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error> {
        let checkpoint = self.client.read_api().get_checkpoint(index.into()).await?;
        self.create_block_response(checkpoint).await
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        let checkpoint = self.client.read_api().get_checkpoint(hash.into()).await?;
        self.create_block_response(checkpoint).await
    }

    async fn current_block(&self) -> Result<BlockResponse, Error> {
        self.get_block_by_index(self.last_indexed_checkpoint()?)
            .await
    }

    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(self.last_indexed_checkpoint()?)
            .await
    }

    async fn get_balance_at_block(
        &self,
        addr: SuiAddress,
        block_height: u64,
    ) -> Result<i128, Error> {
        Ok(self
            .index_store
            .balances
            .iter()
            .skip_prior_to(&(addr, block_height))?
            .next()
            .and_then(
                |((address, _), balance)| {
                    if address == addr {
                        Some(balance)
                    } else {
                        None
                    }
                },
            )
            .unwrap_or_default())
    }
}

impl CheckpointBlockProvider {
    pub fn spawn(client: SuiClient, db_path: &Path) -> Self {
        let blocks = Self {
            index_store: Arc::new(CheckpointIndexStore::open(db_path, None)),
            client,
        };

        let update_interval = option_env!("CHECKPOINT_UPDATE_INTERVAL")
            .map(|i| u64::from_str(i).ok())
            .flatten()
            .unwrap_or(2000);
        let update_interval = Duration::from_millis(update_interval);

        let f = blocks.clone();
        spawn_monitored_task!(async move {
            if f.index_store.is_empty() {
                info!("Index Store is empty, indexing genesis block.");
                let mut checkpoint = None;
                while checkpoint.is_none() {
                    checkpoint = f.client.read_api().get_checkpoint(0.into()).await.ok();
                    if checkpoint.is_none() {
                        info!("Genesis checkpoint not available, retry in 10 seconds.");
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    }
                }
                let resp = f.create_block_response(checkpoint.unwrap()).await.unwrap();
                f.update_balance(resp.block).await.unwrap();
            } else {
                let current_block = f.current_block_identifier().await.unwrap();
                info!("Resuming from block {}", current_block.index);
            };
            loop {
                if let Err(e) = f.index_checkpoints().await {
                    error!("Error indexing checkpoint, cause: {e:?}")
                }
                tokio::time::sleep(update_interval).await;
            }
        });

        blocks
    }

    async fn index_checkpoints(&self) -> Result<(), Error> {
        let last_checkpoint = self.last_indexed_checkpoint()?;
        let head = self
            .client
            .read_api()
            .get_latest_checkpoint_sequence_number()
            .await?;
        if last_checkpoint < head {
            for seq in last_checkpoint + 1..=head {
                let checkpoint = self.client.read_api().get_checkpoint(seq.into()).await?;
                let timestamp = UNIX_EPOCH + Duration::from_millis(checkpoint.timestamp_ms);
                info!(
                    "indexing checkpoint {seq} with {} txs, timestamp: {}",
                    checkpoint.transactions.len(),
                    DateTime::<Utc>::from(timestamp).format("%Y-%m-%d %H:%M:%S")
                );
                let resp = self.create_block_response(checkpoint).await?;
                self.update_balance(resp.block).await?;
                self.index_store.last_checkpoint.insert(&true, &seq)?;
            }
        } else {
            debug!("No new checkpoints.")
        };
        Ok(())
    }

    async fn update_balance(&self, block: Block) -> Result<(), anyhow::Error> {
        let block_height = block.block_identifier.index;
        let last_block_height = if block_height == 0 {
            0
        } else {
            block_height - 1
        };
        let balances: HashMap<SuiAddress, i128> =
            block
                .transactions
                .into_iter()
                .fold(HashMap::new(), |mut changes, tx| {
                    for (address, balance) in extract_balance_changes_from_ops(tx.operations) {
                        *changes.entry(address).or_default() += balance;
                    }
                    changes
                });

        for (addr, value) in balances {
            let current_balance = self.get_balance_at_block(addr, last_block_height).await?;
            let new_balance = current_balance + value;
            if new_balance < 0 {
                // This can happen due to missing transactions data due to unstable validators, causing balance to
                // fall below zero temporarily. The problem should go away when we start using checkpoints for event and indexing
                warn!("Account gas value fall below 0 at block {block_height}, address: [{addr}], current balance = {current_balance}, balance change = {value}.");
            }
            self.index_store
                .balances
                .insert(&(addr, block_height), &new_balance)?;
        }
        Ok(())
    }

    async fn create_block_response(&self, checkpoint: Checkpoint) -> Result<BlockResponse, Error> {
        let index = checkpoint.sequence_number;
        let hash = checkpoint.digest;
        let mut transactions = vec![];
        for digest in checkpoint.transactions.iter() {
            let tx = self
                .client
                .read_api()
                .get_transaction_with_options(
                    *digest,
                    SuiTransactionBlockResponseOptions::new()
                        .with_input()
                        .with_effects()
                        .with_balance_changes()
                        .with_events(),
                )
                .await?;
            transactions.push(Transaction {
                transaction_identifier: TransactionIdentifier { hash: tx.digest },
                operations: Operations::try_from(tx)?,
                related_transactions: vec![],
                metadata: None,
            })
        }

        // previous digest should only be None for genesis block.
        if checkpoint.previous_digest.is_none() && index != 0 {
            return Err(Error::DataError(format!(
                "Previous digest is None for checkpoint [{index}], digest: [{hash:?}]"
            )));
        }

        let parent_block_identifier = checkpoint
            .previous_digest
            .map(|hash| BlockIdentifier {
                index: index - 1,
                hash,
            })
            .unwrap_or_else(|| BlockIdentifier { index, hash });

        Ok(BlockResponse {
            block: Block {
                block_identifier: BlockIdentifier { index, hash },
                parent_block_identifier,
                timestamp: checkpoint.timestamp_ms,
                transactions,
                metadata: None,
            },
            other_transactions: vec![],
        })
    }

    async fn create_block_identifier(
        &self,
        seq_number: CheckpointSequenceNumber,
    ) -> Result<BlockIdentifier, Error> {
        let checkpoint = self
            .client
            .read_api()
            .get_checkpoint(seq_number.into())
            .await?;
        Ok(BlockIdentifier {
            index: checkpoint.sequence_number,
            hash: checkpoint.digest,
        })
    }

    fn last_indexed_checkpoint(&self) -> Result<CheckpointSequenceNumber, Error> {
        Ok(self
            .index_store
            .last_checkpoint
            .get(&true)?
            .unwrap_or_default())
    }
}

fn extract_balance_changes_from_ops(ops: Operations) -> HashMap<SuiAddress, i128> {
    ops.into_iter()
        .fold(HashMap::<SuiAddress, i128>::new(), |mut changes, op| {
            if let Some(OperationStatus::Success) = op.status {
                match op.type_ {
                    OperationType::SuiBalanceChange
                    | OperationType::Gas
                    | OperationType::PaySui
                    | OperationType::StakeReward
                    | OperationType::StakePrinciple
                    | OperationType::Stake => {
                        if let (Some(addr), Some(amount)) = (op.account, op.amount) {
                            *changes.entry(addr.address).or_default() += amount.value
                        }
                    }
                    _ => {}
                };
            }
            changes
        })
}

#[derive(DBMapUtils)]
pub struct CheckpointIndexStore {
    #[default_options_override_fn = "default_config"]
    balances: DBMap<(SuiAddress, EpochId), i128>,
    #[default_options_override_fn = "default_config"]
    last_checkpoint: DBMap<bool, CheckpointSequenceNumber>,
}

impl CheckpointIndexStore {
    pub fn open(db_dir: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(
            db_dir.to_path_buf(),
            MetricConf::default(),
            db_options,
            None,
        )
    }

    pub fn is_empty(&self) -> bool {
        self.last_checkpoint.is_empty() && self.balances.is_empty()
    }
}

fn default_config() -> DBOptions {
    point_lookup_db_options()
}
