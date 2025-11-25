// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::DateTime;
use futures::stream::{self, StreamExt, TryStreamExt};
use prost_types::FieldMask;
use std::str::FromStr;
use std::sync::Arc;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, GetCheckpointRequest, get_checkpoint_request};
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::operations::Operations;
use crate::types::{
    Block, BlockHash, BlockIdentifier, BlockResponse, Transaction, TransactionIdentifier,
};
use crate::{CoinMetadataCache, Error};

#[derive(Clone)]
pub struct OnlineServerContext {
    pub client: GrpcClient,
    pub coin_metadata_cache: CoinMetadataCache,
    block_provider: Arc<dyn BlockProvider + Send + Sync>,
}

impl OnlineServerContext {
    pub fn new(
        client: GrpcClient,
        block_provider: Arc<dyn BlockProvider + Send + Sync>,
        coin_metadata_cache: CoinMetadataCache,
    ) -> Self {
        Self {
            client,
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
        let request = GetCheckpointRequest::by_sequence_number(index).with_read_mask(
            FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary.sequence_number",
                "summary.previous_digest",
                "summary.timestamp",
                "transactions.digest",
                "transactions.transaction.sender",
                "transactions.transaction.gas_payment",
                "transactions.transaction.kind",
                "transactions.effects.gas_object",
                "transactions.effects.gas_used",
                "transactions.effects.status",
                "transactions.balance_changes",
                "transactions.events.events.event_type",
                "transactions.events.events.json",
            ]),
        );

        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(request)
            .await
            .map_err(|e| Error::from(anyhow::anyhow!("Failed to get checkpoint: {}", e)))?
            .into_inner();

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| Error::DataError("Checkpoint not found".to_string()))?;

        self.create_block_response(checkpoint).await
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        let mut request = GetCheckpointRequest::default().with_read_mask(FieldMask::from_paths([
            "sequence_number",
            "digest",
            "summary.sequence_number",
            "summary.previous_digest",
            "summary.timestamp",
            "transactions.digest",
            "transactions.transaction.sender",
            "transactions.transaction.gas_payment",
            "transactions.transaction.kind",
            "transactions.effects.gas_object",
            "transactions.effects.gas_used",
            "transactions.effects.status",
            "transactions.balance_changes",
            "transactions.events.events.event_type",
            "transactions.events.events.json",
        ]));
        request.checkpoint_id = Some(get_checkpoint_request::CheckpointId::Digest(
            hash.to_string(),
        ));

        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_inner();
        let checkpoint = response
            .checkpoint
            .ok_or_else(|| Error::DataError("Checkpoint not found".to_string()))?;

        self.create_block_response(checkpoint).await
    }

    async fn current_block(&self) -> Result<BlockResponse, Error> {
        let request = GetCheckpointRequest::latest()
            .with_read_mask(FieldMask::from_paths(["sequence_number"]));

        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_inner();

        let sequence_number = response.checkpoint().sequence_number();
        self.get_block_by_index(sequence_number).await
    }

    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.create_block_identifier(0).await
    }

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        let request = GetCheckpointRequest::latest()
            .with_read_mask(FieldMask::from_paths(["sequence_number"]));

        let response = self
            .client
            .clone()
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_inner();

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| Error::DataError("Missing checkpoint".to_string()))?;

        let sequence_number = checkpoint.sequence_number();

        self.create_block_identifier(sequence_number).await
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

    async fn create_block_response(&self, checkpoint: Checkpoint) -> Result<BlockResponse, Error> {
        let summary = checkpoint.summary();
        let index = summary.sequence_number();
        let hash = CheckpointDigest::from_str(checkpoint.digest())?;
        // Genesis checkpoint (index 0) has no previous digest
        let previous_hash = if index == 0 {
            hash
        } else {
            CheckpointDigest::from_str(summary.previous_digest())?
        };
        let timestamp_ms = summary
            .timestamp
            .ok_or_else(|| Error::DataError("Checkpoint timestamp is missing".to_string()))
            .and_then(|ts| {
                DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                    .ok_or_else(|| Error::DataError(format!("Invalid timestamp: {}", ts)))
            })?
            .timestamp_millis() as u64;

        let transactions: Vec<Transaction> = stream::iter(checkpoint.transactions)
            .map(|executed_tx| async move {
                let digest = TransactionDigest::from_str(executed_tx.digest())?;
                Ok::<_, Error>(Transaction {
                    transaction_identifier: TransactionIdentifier { hash: digest },
                    // This is async because it makes a GetCoinMetadata call if the coin metadata
                    // isn't already cached.
                    operations: Operations::try_from_executed_transaction(
                        executed_tx,
                        &self.coin_metadata_cache,
                    )
                    .await?,
                    related_transactions: vec![],
                    metadata: None,
                })
            })
            // A checkpoint can have thousands of transactions so
            // limit the amount of work a single rosetta request can generate
            // concurrently to prevent resource starvation issues.
            .buffer_unordered(10)
            .try_collect()
            .await?;

        let parent_block_identifier = if index == 0 {
            // Genesis block is its own parent
            BlockIdentifier { index, hash }
        } else {
            BlockIdentifier {
                index: index - 1,
                hash: previous_hash,
            }
        };

        Ok(BlockResponse {
            block: Block {
                block_identifier: BlockIdentifier { index, hash },
                parent_block_identifier,
                timestamp: timestamp_ms,
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
        let grpc_request = GetCheckpointRequest::by_sequence_number(seq_number)
            .with_read_mask(FieldMask::from_paths(["sequence_number", "digest"]));
        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(grpc_request)
            .await?
            .into_inner();

        let checkpoint = response.checkpoint();
        let index = checkpoint.sequence_number();
        let hash = checkpoint.digest();

        Ok(BlockIdentifier {
            index,
            hash: CheckpointDigest::from_str(hash)?,
        })
    }
}
