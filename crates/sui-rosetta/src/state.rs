// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::operations::Operations;
use crate::types::{
    Block, BlockHash, BlockHeight, BlockIdentifier, BlockResponse, OperationType, Transaction,
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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sui_sdk::rpc_types::SuiTransactionKind;
use sui_sdk::SuiClient;
use sui_storage::default_db_options;
use sui_types::base_types::SuiAddress;
use sui_types::query::TransactionQuery;
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
    fn genesis_block_identifier(&self) -> BlockIdentifier;
    fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    fn current_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn get_balance_at_block(
        &self,
        addr: SuiAddress,
        block_height: u64,
    ) -> Result<u128, Error>;
}

#[derive(Clone)]
pub struct PseudoBlockProvider {
    database: Arc<BlockProviderTables>,
    client: SuiClient,
}

#[async_trait]
impl BlockProvider for PseudoBlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error> {
        let (block_id, parent, timestamp) =
            &self
                .database
                .blocks
                .get(&index)?
                .ok_or(Error::BlockNotFound {
                    index: Some(index),
                    hash: None,
                })?;

        Ok(self
            .create_block_response(*block_id, *parent, *timestamp)
            .await?)
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        let height = self
            .database
            .block_heights
            .get(&hash)?
            .ok_or(Error::BlockNotFound {
                index: None,
                hash: Some(hash),
            })?;
        self.get_block_by_index(height).await
    }

    async fn current_block(&self) -> Result<BlockResponse, Error> {
        let (_, (block_id, parent, timestamp)) = self
            .database
            .blocks
            .iter()
            .skip_prior_to(&BlockHeight::MAX)?
            .next()
            .ok_or(Error::BlockNotFound {
                index: None,
                hash: None,
            })?;
        Ok(self
            .create_block_response(block_id, parent, timestamp)
            .await?)
    }

    fn genesis_block_identifier(&self) -> BlockIdentifier {
        self.oldest_block_identifier().unwrap()
    }

    fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.database
            .blocks
            .iter()
            .next()
            .map(|(_, (id, _, _))| id)
            .ok_or(Error::BlockNotFound {
                index: None,
                hash: None,
            })
    }

    fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        let (_, (block_id, ..)) =
            self.database
                .blocks
                .iter()
                .last()
                .ok_or(Error::BlockNotFound {
                    index: None,
                    hash: None,
                })?;
        Ok(block_id)
    }

    async fn get_balance_at_block(
        &self,
        addr: SuiAddress,
        block_height: u64,
    ) -> Result<u128, Error> {
        Ok(self
            .database
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

impl PseudoBlockProvider {
    pub fn spawn(client: SuiClient, db_path: &Path) -> Self {
        let blocks = Self {
            database: Arc::new(BlockProviderTables::open(db_path, None)),
            client: client.clone(),
        };

        let block_interval = option_env!("SUI_BLOCK_INTERVAL")
            .map(|i| u64::from_str(i).ok())
            .flatten()
            .unwrap_or(2000);
        let block_interval = Duration::from_millis(block_interval);

        let f = blocks.clone();
        spawn_monitored_task!(async move {
            if f.database.is_empty() {
                // We expect creating genesis block to success.
                info!("Datastore is empty, processing genesis block.");
                process_genesis_block(&client, &f).await.unwrap()
            } else {
                let current_block = f.current_block_identifier().unwrap();
                info!("Resuming from block {}", current_block.index);
            };
            loop {
                if let Err(e) = f.create_next_block(&client).await {
                    error!("Error creating block, cause: {e:?}")
                }
                tokio::time::sleep(block_interval).await;
            }
        });

        blocks
    }

    async fn create_next_block(&self, client: &SuiClient) -> Result<(), Error> {
        let current_block = self.current_block_identifier()?;
        // Sui get_total_transaction_number starts from 1.
        let total_tx = client.read_api().get_total_transaction_number().await? - 1;
        if total_tx == 0 {
            return Ok(());
        }
        if current_block.index < total_tx {
            let cursor = current_block.hash;
            let mut tx_digests = client
                .read_api()
                .get_transactions(TransactionQuery::All, Some(cursor), None, false)
                .await?
                .data;
            if tx_digests.remove(0) != cursor {
                return Err(Error::DataError(
                    "Incorrect transaction data returned from Sui.".to_string(),
                ));
            }

            let mut index = current_block.index;
            let mut parent_block_identifier = current_block;

            for digest in tx_digests {
                index += 1;
                let block_identifier = BlockIdentifier {
                    index,
                    hash: digest,
                };

                // update balance
                let response = client.read_api().get_transaction(digest).await?;

                let operations = response.try_into()?;

                self.add_block_index(&block_identifier, &parent_block_identifier)?;
                self.update_balance(index, operations)
                    .await
                    .map_err(|e| anyhow!("Failed to update balance, cause : {e}",))?;
                parent_block_identifier = block_identifier
            }
        } else {
            debug!("No new transactions.")
        };

        Ok(())
    }

    fn add_block_index(
        &self,
        block_id: &BlockIdentifier,
        parent: &BlockIdentifier,
    ) -> Result<(), Error> {
        let index = &block_id.index;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.database.block_heights.insert(&block_id.hash, index)?;
        self.database
            .blocks
            .insert(index, &(*block_id, *parent, timestamp))?;
        Ok(())
    }

    async fn update_balance(
        &self,
        block_height: u64,
        ops: Operations,
    ) -> Result<(), anyhow::Error> {
        for (addr, value) in extract_balance_changes_from_ops(ops)? {
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

            self.database.balances.insert(
                &(addr, block_height),
                &HistoricBalance {
                    block_height,
                    balance: new_balance as u128,
                },
            )?;
        }
        Ok(())
    }

    async fn create_block_response(
        &self,
        block_identifier: BlockIdentifier,
        parent_block_identifier: BlockIdentifier,
        timestamp: u64,
    ) -> Result<BlockResponse, Error> {
        let tx = self
            .client
            .read_api()
            .get_transaction(block_identifier.hash)
            .await?;

        let digest = tx.certificate.transaction_digest;
        let operations = tx.try_into()?;

        let transaction = Transaction {
            transaction_identifier: TransactionIdentifier { hash: digest },
            operations,
            related_transactions: vec![],
            metadata: None,
        };

        Ok(BlockResponse {
            block: Block {
                block_identifier,
                parent_block_identifier,
                timestamp,
                transactions: vec![transaction],
                metadata: None,
            },
            other_transactions: vec![],
        })
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

async fn process_genesis_block(client: &SuiClient, f: &PseudoBlockProvider) -> Result<(), Error> {
    let digest = *client
        .read_api()
        .get_transactions(TransactionQuery::All, None, Some(1), false)
        .await?
        .data
        .first()
        .ok_or_else(|| Error::InternalError(anyhow!("Cannot find genesis transaction.")))?;

    let response = client.read_api().get_transaction(digest).await?;
    if !response
        .certificate
        .data
        .transactions
        .iter()
        .any(|tx| matches!(tx, SuiTransactionKind::Genesis(_)))
    {
        return Err(Error::InternalError(anyhow!(
            "Transaction [{digest:?}] is not a Genesis transaction."
        )));
    }
    let operations = response.try_into()?;
    let block_identifier = BlockIdentifier {
        index: 0,
        hash: digest,
    };

    f.add_block_index(&block_identifier, &block_identifier)?;
    f.update_balance(0, operations)
        .await
        .map_err(|e| anyhow!("Failed to update balance, cause : {e}",))?;

    Ok(())
}

#[derive(DBMapUtils)]
pub struct BlockProviderTables {
    #[default_options_override_fn = "default_config"]
    blocks: DBMap<BlockHeight, (BlockIdentifier, BlockIdentifier, u64)>,
    #[default_options_override_fn = "default_config"]
    block_heights: DBMap<BlockHash, BlockHeight>,
    #[default_options_override_fn = "default_config"]
    balances: DBMap<(SuiAddress, u64), HistoricBalance>,
}

impl BlockProviderTables {
    pub fn open(db_dir: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(db_dir.to_path_buf(), db_options, None)
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

fn default_config() -> DBOptions {
    default_db_options(None, None).1
}
