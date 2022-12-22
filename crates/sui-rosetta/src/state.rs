// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::operations::Operation;
use crate::types::{
    AccountIdentifier, Amount, Block, BlockHash, BlockHeight, BlockIdentifier, BlockResponse,
    CoinAction, CoinChange, CoinID, CoinIdentifier, OperationStatus, OperationType, SignedValue,
    Transaction, TransactionIdentifier,
};
use crate::{Error, SUI};
use anyhow::anyhow;
use async_trait::async_trait;
use mysten_metrics::spawn_monitored_task;
use rocksdb::Options;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sui_config::genesis::Genesis;
use sui_sdk::SuiClient;
use sui_storage::default_db_options;
use sui_types::base_types::{
    SequenceNumber, SuiAddress, TransactionDigest, TRANSACTION_DIGEST_LENGTH,
};
use sui_types::gas_coin::GasCoin;
use sui_types::query::TransactionQuery;
use tracing::{debug, error, info};
use typed_store::rocks::{DBMap, DBOptions};
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
    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error>;
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
    genesis: Genesis,
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
        BlockIdentifier {
            index: 0,
            hash: BlockHash::new([0u8; TRANSACTION_DIGEST_LENGTH]),
        }
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
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

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.current_block().await.map(|b| b.block.block_identifier)
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
    pub fn spawn(client: SuiClient, genesis: Genesis, db_path: &Path) -> Self {
        let blocks = Self {
            database: Arc::new(BlockProviderTables::open(db_path, None)),
            client: client.clone(),
            genesis,
        };

        let block_interval = option_env!("SUI_BLOCK_INTERVAL")
            .map(|i| u64::from_str(i).ok())
            .flatten()
            .unwrap_or(2000);
        let block_interval = Duration::from_millis(block_interval);

        let f = blocks.clone();
        spawn_monitored_task!(async move {
            if f.database.is_empty() {
                info!("Database is empty, indexing genesis block...");
                let genesis = genesis_block(&f.genesis);
                let genesis_txs = genesis
                    .block
                    .transactions
                    .iter()
                    .flat_map(|tx| tx.operations.clone())
                    .collect();

                f.add_block_index(
                    &genesis.block.block_identifier,
                    &genesis.block.parent_block_identifier,
                )
                .unwrap();
                if let Err(e) = f.update_balance(0, genesis_txs).await {
                    error!("Error updating balance, cause: {e:?}")
                }
            } else {
                let current_block = f.current_block_identifier().await.unwrap();
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
        let current_block = self.current_block_identifier().await?;
        let total_tx = client.read_api().get_total_transaction_number().await?;
        if total_tx == 0 {
            return Ok(());
        }
        if current_block.index < total_tx {
            let tx_digests = if current_block.index == 0 {
                client
                    .read_api()
                    .get_transactions(TransactionQuery::All, None, None, false)
                    .await?
                    .data
            } else {
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
                tx_digests
            };

            let mut index = current_block.index;
            let mut parent_block_identifier = current_block;

            for digest in tx_digests {
                index += 1;
                let block_identifier = BlockIdentifier {
                    index,
                    hash: digest.into(),
                };

                // update balance
                let response = client.read_api().get_transaction(digest).await?;

                let operations = Operation::from_data_and_events(
                    &response.certificate.data,
                    &response.effects.status,
                    &response.effects.events,
                )?;

                self.add_block_index(&block_identifier, &parent_block_identifier)?;
                self.update_balance(index, operations).await.map_err(|e| {
                    anyhow!(
                        "Failed to update balance, tx: {}, effect:{}, cause : {e}",
                        response.certificate,
                        response.effects
                    )
                })?;
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
        ops: Vec<Operation>,
    ) -> Result<(), anyhow::Error> {
        let balance_changes = extract_balance_changes_from_ops(ops)?;
        for (addr, value) in balance_changes {
            let current_balance = self.get_balance_at_block(addr, block_height).await?;
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
                    balance: new_balance,
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
        if block_identifier.index == 0 {
            return Ok(genesis_block(&self.genesis));
        }

        let tx = self
            .client
            .read_api()
            .get_transaction(block_identifier.hash)
            .await?;

        let digest = tx.certificate.transaction_digest;
        let operations = Operation::from_data_and_events(
            &tx.certificate.data,
            &tx.effects.status,
            &tx.effects.events,
        )?;

        let transaction = Transaction {
            transaction_identifier: TransactionIdentifier { hash: digest },
            operations: operations.clone(),
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
    ops: Vec<Operation>,
) -> Result<BTreeMap<SuiAddress, SignedValue>, anyhow::Error> {
    let mut changes: BTreeMap<SuiAddress, SignedValue> = BTreeMap::new();
    for op in ops {
        match op.type_ {
            OperationType::SuiBalanceChange
            | OperationType::GasSpent
            | OperationType::Genesis
            | OperationType::PaySui => {
                let addr = op
                    .account
                    .ok_or_else(|| anyhow!("Account address cannot be null for {:?}", op.type_))?
                    .address;
                let amount = op
                    .amount
                    .ok_or_else(|| anyhow!("Amount cannot be null for {:?}", op.type_))?;
                changes.entry(addr).or_default().add(&amount.value)
            }
            _ => {}
        }
    }
    Ok(changes)
}

fn genesis_block(genesis: &Genesis) -> BlockResponse {
    let id = BlockIdentifier {
        index: 0,
        hash: BlockHash::new([0u8; TRANSACTION_DIGEST_LENGTH]),
    };

    let operations = genesis
        .objects()
        .iter()
        .flat_map(|o| {
            GasCoin::try_from(o)
                .ok()
                .and_then(|coin| o.owner.get_owner_address().ok().map(|addr| (addr, coin)))
        })
        .enumerate()
        .map(|(index, (address, coin))| Operation {
            operation_identifier: (index as u64).into(),
            related_operations: vec![],
            type_: OperationType::Genesis,
            status: Some(OperationStatus::Success),
            account: Some(AccountIdentifier { address }),
            amount: Some(Amount {
                value: SignedValue::from(coin.value()),
                currency: SUI.clone(),
            }),
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: CoinID {
                        id: *coin.id(),
                        version: SequenceNumber::new(),
                    },
                },
                coin_action: CoinAction::CoinCreated,
            }),
            metadata: None,
        })
        .collect();

    let transaction = Transaction {
        transaction_identifier: TransactionIdentifier {
            hash: TransactionDigest::new([0; 32]),
        },
        operations,
        related_transactions: vec![],
        metadata: None,
    };

    BlockResponse {
        block: Block {
            block_identifier: id.clone(),
            parent_block_identifier: id,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            transactions: vec![transaction],
            metadata: None,
        },
        other_transactions: vec![],
    }
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
