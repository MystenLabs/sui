// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::operations::Operations;
use crate::types::{
    Block, BlockHash, BlockIdentifier, BlockResponse, OperationType, Transaction,
    TransactionIdentifier,
};
use crate::Error;
use anyhow::anyhow;
use async_trait::async_trait;
use mysten_metrics::spawn_monitored_task;
use rocksdb::Options;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use sui_sdk::apis::Checkpoint;
use sui_sdk::SuiClient;
use sui_storage::default_db_options;
use sui_types::base_types::{EpochId, SuiAddress};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tracing::{debug, error, info};
use typed_store::rocks::{DBMap, DBOptions};
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
    ) -> Result<u128, Error>;
}

#[derive(Clone)]
pub struct CheckpointBlockProvider {
    index_store: Arc<CheckpointIndexStore>,
    client: SuiClient,
}

#[async_trait]
impl BlockProvider for CheckpointBlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error> {
        let checkpoint = self.client.read_api().get_checkpoint(index).await?;
        self.create_block_response(checkpoint).await
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        let checkpoint = self
            .client
            .read_api()
            .get_checkpoint_by_digest(hash)
            .await?;
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
    ) -> Result<u128, Error> {
        Ok(self
            .index_store
            .balances
            .iter()
            .skip_prior_to(&(addr, block_height))?
            .next()
            .and_then(|((address, _), balance)| {
                if address == addr {
                    Some(balance.balance)
                } else {
                    None
                }
            })
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
                let checkpoint = f.client.read_api().get_checkpoint(0).await.unwrap();
                let resp = f.create_block_response(checkpoint).await.unwrap();
                f.update_balance(0, resp.block.transactions).await.unwrap();
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
                let checkpoint = self.client.read_api().get_checkpoint(seq).await?;
                let resp = self.create_block_response(checkpoint).await?;
                self.update_balance(seq, resp.block.transactions).await?;
            }
            self.index_store.last_checkpoint.insert(&true, &head)?;
        } else {
            debug!("No new checkpoints.")
        };
        Ok(())
    }

    async fn update_balance(
        &self,
        block_height: u64,
        transactions: Vec<Transaction>,
    ) -> Result<(), anyhow::Error> {
        let balances: HashMap<SuiAddress, i128> =
            transactions
                .into_iter()
                .try_fold(HashMap::new(), |mut changes, tx| {
                    for (address, balance) in extract_balance_changes_from_ops(tx.operations)? {
                        *changes.entry(address).or_default() += balance;
                    }
                    Ok::<HashMap<SuiAddress, i128>, anyhow::Error>(changes)
                })?;

        for (addr, value) in balances {
            let current_balance = self.get_balance_at_block(addr, block_height).await? as i128;
            let new_balance = if value.is_negative() {
                if current_balance < value.abs() {
                    // This can happen due to missing transactions data due to unstable validators, causing balance to
                    // fall below zero temporarily. The problem should go away when we start using checkpoints for event and indexing
                    return Err(anyhow!(
                        "Account gas value fall below 0 at block {}, address: [{}]",
                        block_height,
                        addr
                    ));
                }
                current_balance - value.abs()
            } else {
                current_balance + value.abs()
            };

            self.index_store.balances.insert(
                &(addr, block_height),
                &HistoricBalance {
                    block_height,
                    balance: new_balance as u128,
                },
            )?;
        }
        Ok(())
    }

    async fn create_block_response(&self, checkpoint: Checkpoint) -> Result<BlockResponse, Error> {
        let index = checkpoint.summary.sequence_number;
        let hash = checkpoint.summary.digest();
        let mut transactions = vec![];
        for digest in checkpoint.content.iter() {
            let tx = self
                .client
                .read_api()
                .get_transaction(digest.transaction)
                .await?;
            transactions.push(Transaction {
                transaction_identifier: TransactionIdentifier {
                    hash: tx.certificate.transaction_digest,
                },
                operations: Operations::try_from(tx)?,
                related_transactions: vec![],
                metadata: None,
            })
        }

        // previous digest should only be None for genesis block.
        if checkpoint.summary.previous_digest.is_none() && index != 0 {
            return Err(Error::DataError(format!(
                "Previous digest is None for checkpoint [{index}], digest: [{hash:?}]"
            )));
        }

        let parent_block_identifier = checkpoint
            .summary
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
                timestamp: checkpoint.summary.timestamp_ms,
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
        let checkpoint = self.client.read_api().get_checkpoint(seq_number).await?;
        Ok(BlockIdentifier {
            index: checkpoint.summary.sequence_number,
            hash: checkpoint.summary.digest(),
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoricBalance {
    block_height: u64,
    balance: u128,
}

fn extract_balance_changes_from_ops(
    ops: Operations,
) -> Result<HashMap<SuiAddress, i128>, anyhow::Error> {
    ops.into_iter()
        .try_fold(HashMap::<SuiAddress, i128>::new(), |mut changes, op| {
            match op.type_ {
                OperationType::SuiBalanceChange | OperationType::Gas | OperationType::PaySui => {
                    let addr = op
                        .account
                        .ok_or_else(|| {
                            anyhow!("Account address cannot be null for {:?}", op.type_)
                        })?
                        .address;
                    let amount = op
                        .amount
                        .ok_or_else(|| anyhow!("Amount cannot be null for {:?}", op.type_))?;
                    *changes.entry(addr).or_default() += amount.value
                }
                _ => {}
            };
            Ok(changes)
        })
}

#[derive(DBMapUtils)]
pub struct CheckpointIndexStore {
    #[default_options_override_fn = "default_config"]
    balances: DBMap<(SuiAddress, EpochId), HistoricBalance>,
    #[default_options_override_fn = "default_config"]
    last_checkpoint: DBMap<bool, CheckpointSequenceNumber>,
}

impl CheckpointIndexStore {
    pub fn open(db_dir: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(db_dir.to_path_buf(), db_options, None)
    }

    pub fn is_empty(&self) -> bool {
        self.last_checkpoint.is_empty()
    }
}

fn default_config() -> DBOptions {
    default_db_options(None, None).1
}
