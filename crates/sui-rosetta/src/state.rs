// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use itertools::Itertools;
use sha3::{Digest, Sha3_256};
use tokio::sync::RwLock;
use tracing::{debug, error};

use sui_sdk::SuiClient;
use sui_types::base_types::TRANSACTION_DIGEST_LENGTH;

use crate::types::{Block, BlockHash, BlockIdentifier, BlockResponse, TransactionIdentifier};
use crate::ErrorType::BlockNotFound;
use crate::{
    Error, ErrorType, NetworkIdentifier, SuiEnv, UnsupportedBlockchain, UnsupportedNetwork,
};

#[derive(Default)]
pub struct ApiState {
    clients: BTreeMap<SuiEnv, (Arc<SuiClient>, Box<dyn BlockProvider + Send + Sync>)>,
}

impl ApiState {
    pub fn add_env(&mut self, env: SuiEnv, client: SuiClient) {
        let client = Arc::new(client);
        let block_fabricator = PseudoBlockProvider::spawn(env, client.clone());
        self.clients
            .insert(env, (client, Box::new(block_fabricator)));
    }

    pub async fn get_client(&self, env: SuiEnv) -> Result<&SuiClient, Error> {
        let (client, _) = self
            .clients
            .get(&env)
            .ok_or_else(|| Error::new(ErrorType::UnsupportedNetwork))?;
        Ok(client)
    }

    pub fn get_envs(&self) -> Vec<SuiEnv> {
        self.clients.keys().cloned().collect()
    }

    pub fn checks_network_identifier(
        &self,
        network_identifier: &NetworkIdentifier,
    ) -> Result<(), Error> {
        if &network_identifier.blockchain != "sui" {
            return Err(Error::new(UnsupportedBlockchain));
        }
        if !self.clients.keys().contains(&network_identifier.network) {
            return Err(Error::new(UnsupportedNetwork));
        }
        Ok(())
    }

    pub fn blocks(&self, env: SuiEnv) -> Result<&(dyn BlockProvider + Sync + Send), Error> {
        let (_, block_fabricator) = self
            .clients
            .get(&env)
            .ok_or_else(|| Error::new(ErrorType::UnsupportedNetwork))?;
        Ok(&**block_fabricator)
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
}

#[derive(Clone)]
pub struct PseudoBlockProvider {
    blocks: Arc<RwLock<Vec<BlockResponse>>>,
}

#[async_trait]
impl BlockProvider for PseudoBlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error> {
        self.blocks
            .read()
            .await
            .iter()
            .find(|b| b.block.block_identifier.index == index)
            .cloned()
            .ok_or_else(|| Error::new(BlockNotFound))
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        self.blocks
            .read()
            .await
            .iter()
            .find(|b| b.block.block_identifier.hash == hash)
            .cloned()
            .ok_or_else(|| Error::new(BlockNotFound))
    }

    async fn current_block(&self) -> Result<BlockResponse, Error> {
        self.blocks
            .read()
            .await
            .last()
            .ok_or_else(|| {
                Error::new_with_msg(
                    BlockNotFound,
                    "Unexpected error, cannot find the latest block.",
                )
            })
            .cloned()
    }

    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        Ok(BlockIdentifier {
            index: 0,
            hash: BlockHash([0u8; TRANSACTION_DIGEST_LENGTH]),
        })
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.blocks
            .read()
            .await
            .first()
            .map(|b| b.block.block_identifier.clone())
            .ok_or_else(|| {
                Error::new_with_msg(
                    BlockNotFound,
                    "Unexpected error, cannot find the oldest block.",
                )
            })
    }

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.current_block().await.map(|b| b.block.block_identifier)
    }
}

impl PseudoBlockProvider {
    fn spawn(env: SuiEnv, client: Arc<SuiClient>) -> Self {
        let blocks = Self {
            blocks: Arc::new(RwLock::new(Vec::new())),
        };

        let block_interval = option_env!("SUI_BLOCK_INTERVAL")
            .map(|i| u64::from_str(i).ok())
            .flatten()
            .unwrap_or(10000);
        let block_interval = Duration::from_millis(block_interval);

        let f = blocks.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = f.create_next_block(client.clone()).await {
                    error!("Error creating block for env [{env:?}], cause: {e:?}")
                }
                tokio::time::sleep(block_interval).await;
            }
        });

        blocks
    }

    async fn create_next_block(&self, client: Arc<SuiClient>) -> Result<(), anyhow::Error> {
        let current_block = self.current_block_identifier().await.ok();
        let current_index = current_block.as_ref().map(|b| b.index).unwrap_or_default();
        let total_tx = client.read_api().get_total_transaction_number().await?;

        if current_index < total_tx {
            let tx_digests = client
                .read_api()
                .get_transactions_in_range(current_index, total_tx)
                .await?;

            // Create block hash using all transaction hashes
            let hasher = tx_digests
                .iter()
                .fold(Sha3_256::default(), |mut hasher, (_, digest)| {
                    hasher.update(digest.as_ref());
                    hasher
                });
            let hash = hasher.finalize();
            let hash = BlockHash(hash.into());

            let block_identifier = BlockIdentifier {
                index: total_tx,
                hash,
            };

            let parent_block_identifier = if let Some(parent) = current_block {
                parent
            } else {
                block_identifier.clone()
            };

            let other_transactions = tx_digests
                .iter()
                .map(|(_, digest)| TransactionIdentifier { hash: *digest })
                .collect();

            let new_block = BlockResponse {
                block: Block {
                    block_identifier,
                    parent_block_identifier,
                    timestamp: 0,
                    transactions: vec![],
                    metadata: None,
                },
                other_transactions,
            };
            self.blocks.write().await.push(new_block);
        } else {
            debug!("No new transactions.")
        };

        Ok(())
    }
}
