// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;
use sui_rpc::proto::sui::rpc::v2beta2::Checkpoint as ProtoCheckpoint;
use sui_sdk::SuiClient;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::{CheckpointDigest, CheckpointSequenceNumber};

use crate::grpc_client::GrpcClient;
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
    pub client: GrpcClient,
    // TODO: Remove sui_client once all operations are migrated to GRPC
    // Currently kept for: account balance queries, coin operations, and staking queries
    pub sui_client: SuiClient,
    pub coin_metadata_cache: CoinMetadataCache,
    block_provider: Arc<dyn BlockProvider + Send + Sync>,
}

impl OnlineServerContext {
    pub fn new(
        client: GrpcClient,
        sui_client: SuiClient,
        block_provider: Arc<dyn BlockProvider + Send + Sync>,
        coin_metadata_cache: CoinMetadataCache,
    ) -> Self {
        Self {
            client: client.clone(),
            sui_client,
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
    client: GrpcClient,
    coin_metadata_cache: CoinMetadataCache,
}

#[async_trait]
impl BlockProvider for CheckpointBlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error> {
        let checkpoint = self
            .client
            .get_checkpoint_with_transactions_by_sequence(index)
            .await?;
        self.create_block_response(checkpoint).await
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        let digest = hash;
        let checkpoint = self
            .client
            .get_checkpoint_with_transactions_by_digest(digest)
            .await?;
        self.create_block_response(checkpoint).await
    }

    async fn current_block(&self) -> Result<BlockResponse, Error> {
        let checkpoint_sequence = self.client.get_latest_checkpoint().await?;
        self.get_block_by_index(checkpoint_sequence).await
    }

    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        let checkpoint_sequence = self.client.get_latest_checkpoint().await?;
        self.create_block_identifier(checkpoint_sequence).await
    }

    async fn create_block_identifier(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(checkpoint).await
    }
}

impl CheckpointBlockProvider {
    pub fn new(client: GrpcClient, coin_metadata_cache: CoinMetadataCache) -> Self {
        Self {
            client,
            coin_metadata_cache,
        }
    }

    async fn create_block_response(
        &self,
        checkpoint: ProtoCheckpoint,
    ) -> Result<BlockResponse, Error> {
        let index = checkpoint.sequence_number.unwrap_or(0);
        let hash_str = checkpoint.digest.unwrap_or_default();

        // Parse the checkpoint digest
        let hash: CheckpointDigest = hash_str
            .parse()
            .map_err(|e| Error::DataError(format!("Invalid checkpoint digest: {}", e)))?;

        // Process transactions from the proto checkpoint
        let mut transactions = Vec::new();
        for proto_tx in checkpoint.transactions {
            let digest_str = proto_tx.digest.clone().unwrap_or_default();
            let tx_digest = digest_str
                .parse::<TransactionDigest>()
                .map_err(|e| Error::DataError(format!("Invalid transaction digest: {}", e)))?;

            let transaction = Transaction {
                transaction_identifier: TransactionIdentifier { hash: tx_digest },
                operations: Operations::try_from_proto_transaction(
                    proto_tx,
                    &self.coin_metadata_cache,
                )
                .await?,
                related_transactions: vec![],
                metadata: None,
            };
            transactions.push(transaction);
        }

        // Get previous digest from summary if available
        let previous_digest = checkpoint
            .summary
            .as_ref()
            .and_then(|s| s.previous_digest.as_ref())
            .and_then(|d| d.parse::<CheckpointDigest>().ok());

        // previous digest should only be None for genesis block.
        if previous_digest.is_none() && index != 0 {
            return Err(Error::DataError(format!(
                "Previous digest is None for checkpoint [{index}], digest: [{hash:?}]"
            )));
        }

        let parent_block_identifier = previous_digest
            .map(|hash| BlockIdentifier {
                index: index - 1,
                hash,
            })
            .unwrap_or_else(|| BlockIdentifier { index, hash });

        // Extract timestamp from summary
        let timestamp = checkpoint
            .summary
            .as_ref()
            .and_then(|s| s.timestamp.as_ref())
            .map(|ts| ts.seconds as u64 * 1000 + ts.nanos as u64 / 1_000_000)
            .unwrap_or(0);

        Ok(BlockResponse {
            block: Block {
                block_identifier: BlockIdentifier { index, hash },
                parent_block_identifier,
                timestamp,
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
        let checkpoint_summary = self.client.get_checkpoint_by_sequence(seq_number).await?;
        Ok(BlockIdentifier {
            index: checkpoint_summary.sequence_number,
            hash: *checkpoint_summary.digest(),
        })
    }
}
