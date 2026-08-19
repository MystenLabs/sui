// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::proto_to_timestamp_ms;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::digests::TransactionDigest;

use crate::error::Error;
use crate::ledger_grpc_reader::CheckpointedTransaction;
use crate::ledger_grpc_reader::ChunkedLoader;
use crate::ledger_grpc_reader::LedgerGrpcReader;

/// Key for fetching transaction contents (TransactionData, Effects, and Events) by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionKey(pub TransactionDigest);

/// Key for fetching just the checkpoint timestamp of a transaction by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionTimestampKey(pub TransactionDigest);

/// Key for fetching a transaction's effects as rendered by the server, which carries additional
/// information that cannot be derived from the effects BCS client-side (object type annotations,
/// runtime-loaded objects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProtoEffectsKey(pub TransactionDigest);

#[async_trait::async_trait]
impl ChunkedLoader<TransactionKey> for LedgerGrpcReader {
    type Value = CheckpointedTransaction;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        self.max_batch_get_transactions()
    }

    async fn load_chunk(
        &self,
        keys: &[TransactionKey],
    ) -> Result<HashMap<TransactionKey, CheckpointedTransaction>, Error> {
        let digests = keys.iter().map(|key| key.0.to_string()).collect();

        let mut request = proto::BatchGetTransactionsRequest::default();
        request.digests = digests;
        request.read_mask = Some(CheckpointedTransaction::read_mask());

        let batch_response = self.batch_get_transactions(request).await?;

        let mut results = HashMap::new();
        for tx_result in batch_response.transactions {
            if let Some(proto::get_transaction_result::Result::Transaction(executed)) =
                tx_result.result
            {
                let transaction = CheckpointedTransaction::try_from(&executed)?;
                results.insert(
                    TransactionKey(transaction.transaction_data.digest()),
                    transaction,
                );
            }
        }
        Ok(results)
    }
}

#[async_trait::async_trait]
impl ChunkedLoader<TransactionTimestampKey> for LedgerGrpcReader {
    type Value = u64;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        self.max_batch_get_transactions()
    }

    async fn load_chunk(
        &self,
        keys: &[TransactionTimestampKey],
    ) -> Result<HashMap<TransactionTimestampKey, u64>, Error> {
        let digests = keys.iter().map(|key| key.0.to_string()).collect();

        let mut request = proto::BatchGetTransactionsRequest::default();
        request.digests = digests;
        request.read_mask = Some(FieldMask::from_paths(["digest", "timestamp"]));

        let batch_response = self.batch_get_transactions(request).await?;

        let mut results = HashMap::new();
        for tx_result in batch_response.transactions {
            let Some(proto::get_transaction_result::Result::Transaction(executed)) =
                tx_result.result
            else {
                continue;
            };

            let digest: TransactionDigest = executed
                .digest
                .as_deref()
                .context("BatchGetTransactions response missing digest")?
                .parse()
                .context("Failed to parse transaction digest")?;

            // Transactions served by the ledger service are always checkpointed, but tolerate a
            // missing timestamp by treating the transaction as not found.
            let Some(timestamp) = executed.timestamp else {
                continue;
            };
            let timestamp_ms = proto_to_timestamp_ms(timestamp)
                .map_err(|e| anyhow::anyhow!("Failed to parse timestamp: {}", e))?;

            results.insert(TransactionTimestampKey(digest), timestamp_ms);
        }
        Ok(results)
    }
}

#[async_trait::async_trait]
impl ChunkedLoader<ProtoEffectsKey> for LedgerGrpcReader {
    type Value = proto::TransactionEffects;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        self.max_batch_get_transactions()
    }

    async fn load_chunk(
        &self,
        keys: &[ProtoEffectsKey],
    ) -> Result<HashMap<ProtoEffectsKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let digests = keys.iter().map(|key| key.0.to_string()).collect();

        let mut request = proto::BatchGetTransactionsRequest::default();
        request.digests = digests;
        request.read_mask = Some(FieldMask::from_paths(["digest", "effects"]));

        let batch_response = self.batch_get_transactions(request).await?;

        let mut results = HashMap::new();
        for tx_result in batch_response.transactions {
            let Some(proto::get_transaction_result::Result::Transaction(executed)) =
                tx_result.result
            else {
                continue;
            };

            let digest: TransactionDigest = executed
                .digest
                .as_deref()
                .context("BatchGetTransactions response missing digest")?
                .parse()
                .context("Failed to parse transaction digest")?;

            let Some(effects) = executed.effects else {
                continue;
            };

            results.insert(ProtoEffectsKey(digest), effects);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::dataloader::Loader;

    use super::*;
    use crate::ledger_grpc_reader::test_support::assert_chunked;
    use crate::ledger_grpc_reader::test_support::mock_reader;

    #[tokio::test]
    async fn transaction_load_chunks_oversized_batches() {
        let (reader, mock, server) = mock_reader().await;
        let limit = reader.max_batch_get_transactions();

        let keys: Vec<TransactionKey> = (0..limit + 50)
            .map(|_| TransactionKey(TransactionDigest::random()))
            .collect();

        let result = reader.load(&keys).await.expect("load should succeed");
        assert!(result.is_empty());

        let expected: Vec<String> = keys.iter().map(|key| key.0.to_string()).collect();
        assert_chunked(mock.transaction_batches(), limit, &expected);

        server.abort();
    }

    #[tokio::test]
    async fn transaction_timestamp_load_chunks_oversized_batches() {
        let (reader, mock, server) = mock_reader().await;
        let limit = reader.max_batch_get_transactions();

        let keys: Vec<TransactionTimestampKey> = (0..limit + 50)
            .map(|_| TransactionTimestampKey(TransactionDigest::random()))
            .collect();

        let result = reader.load(&keys).await.expect("load should succeed");
        assert!(result.is_empty());

        let expected: Vec<String> = keys.iter().map(|key| key.0.to_string()).collect();
        assert_chunked(mock.transaction_batches(), limit, &expected);

        server.abort();
    }
}
