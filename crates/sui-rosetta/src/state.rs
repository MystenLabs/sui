// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::future::try_join_all;
use std::sync::Arc;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::rpc_types::Checkpoint;
use sui_sdk::SuiClient;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::operations::Operations;
use crate::types::{
    Block, BlockHash, BlockIdentifier, BlockResponse, Transaction, TransactionIdentifier,
};
use crate::{CoinMetadataCache, Error};

#[cfg(test)]
#[path = "unit_tests/balance_changing_tx_tests.rs"]
mod balance_changing_tx_tests;

#[derive(Clone)]
pub struct OnlineServerContext {
    pub client: SuiClient,
    pub coin_metadata_cache: CoinMetadataCache,
    block_provider: Arc<dyn BlockProvider + Send + Sync>,
}

impl OnlineServerContext {
    pub fn new(
        client: SuiClient,
        block_provider: Arc<dyn BlockProvider + Send + Sync>,
        coin_metadata_cache: CoinMetadataCache,
    ) -> Self {
        Self {
            client: client.clone(),
            block_provider,
            coin_metadata_cache,
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
    async fn create_block_identifier(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<BlockIdentifier, Error>;
}

#[derive(Clone)]
pub struct CheckpointBlockProvider {
    client: SuiClient,
    coin_metadata_cache: CoinMetadataCache,
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
        let checkpoint = self
            .client
            .read_api()
            .get_latest_checkpoint_sequence_number()
            .await?;
        self.get_block_by_index(checkpoint).await
    }

    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        let checkpoint = self
            .client
            .read_api()
            .get_latest_checkpoint_sequence_number()
            .await?;

        self.create_block_identifier(checkpoint).await
    }

    async fn create_block_identifier(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(checkpoint).await
    }
}

impl CheckpointBlockProvider {
    pub fn new(client: SuiClient, coin_metadata_cache: CoinMetadataCache) -> Self {
        Self {
            client,
            coin_metadata_cache,
        }
    }

    async fn create_block_response(&self, checkpoint: Checkpoint) -> Result<BlockResponse, Error> {
        let index = checkpoint.sequence_number;
        let hash = checkpoint.digest;

        let chunks = checkpoint
            .transactions
            .chunks(5)
            .map(|batch| async {
                let transaction_responses = self
                    .client
                    .read_api()
                    .multi_get_transactions_with_options(
                        batch.to_vec(),
                        SuiTransactionBlockResponseOptions::new()
                            .with_input()
                            .with_effects()
                            .with_balance_changes()
                            .with_events(),
                    )
                    .await?;

                let mut transactions = vec![];
                for tx in transaction_responses.into_iter() {
                    transactions.push(Transaction {
                        transaction_identifier: TransactionIdentifier { hash: tx.digest },
                        operations: Operations::try_from_response(tx, &self.coin_metadata_cache)
                            .await?,
                        related_transactions: vec![],
                        metadata: None,
                    })
                }
                Ok::<Vec<_>, anyhow::Error>(transactions)
            })
            .collect::<Vec<_>>();

        let transactions = try_join_all(chunks)
            .await?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

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
}
