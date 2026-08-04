// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::collections::HashMap;

use anyhow::Context;
use async_graphql::dataloader::Loader;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use prost_types::FieldMask;
use sui_indexer_alt_schema::schema::kv_transactions;
use sui_indexer_alt_schema::transactions::StoredTransaction;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::proto_to_timestamp_ms;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::digests::TransactionDigest;

use crate::error::Error;
use crate::ledger_grpc_reader::CheckpointedTransaction;
use crate::ledger_grpc_reader::ChunkedLoader;
use crate::ledger_grpc_reader::LedgerGrpcReader;
use crate::ledger_grpc_reader::MAX_BATCH_GET_TRANSACTIONS;
use crate::pg_reader::PgReader;

/// Key for fetching transaction contents (TransactionData, Effects, and Events) by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionKey(pub TransactionDigest);

/// Key for fetching just the checkpoint timestamp of a transaction by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionTimestampKey(pub TransactionDigest);

#[async_trait::async_trait]
impl Loader<TransactionKey> for PgReader {
    type Value = StoredTransaction;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionKey],
    ) -> Result<HashMap<TransactionKey, Self::Value>, Error> {
        use kv_transactions::dsl as t;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let digests: BTreeSet<_> = keys.iter().map(|d| d.0.into_inner()).collect();
        let transactions: Vec<StoredTransaction> = conn
            .results(t::kv_transactions.filter(t::tx_digest.eq_any(digests)))
            .await?;

        let digest_to_stored: HashMap<_, _> = transactions
            .into_iter()
            .map(|stored| (stored.tx_digest.clone(), stored))
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                Some((*key, digest_to_stored.get(slice).cloned()?))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl ChunkedLoader<TransactionKey> for LedgerGrpcReader {
    type Value = CheckpointedTransaction;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        MAX_BATCH_GET_TRANSACTIONS
    }

    async fn load_chunk(
        &self,
        keys: &[TransactionKey],
    ) -> Result<HashMap<TransactionKey, CheckpointedTransaction>, Error> {
        let digests = keys.iter().map(|key| key.0.to_string()).collect();

        let mut request = proto::BatchGetTransactionsRequest::default();
        request.digests = digests;
        request.read_mask = Some(FieldMask::from_paths([
            "transaction.bcs",
            "effects.bcs",
            "events.bcs",
            "signatures.bcs",
            "checkpoint",
            "timestamp",
            "balance_changes",
        ]));

        let batch_response = self.batch_get_transactions(request).await?;

        let mut results = HashMap::new();
        for tx_result in batch_response.transactions {
            if let Some(proto::get_transaction_result::Result::Transaction(executed)) =
                tx_result.result
            {
                let full_tx: sui_types::full_checkpoint_content::ExecutedTransaction = (&executed)
                    .try_into()
                    .context("Failed to convert ExecutedTransaction from proto")?;

                let timestamp_ms = executed
                    .timestamp
                    .map(proto_to_timestamp_ms)
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("Failed to parse timestamp: {}", e))?;

                let transaction = CheckpointedTransaction {
                    effects: Box::new(full_tx.effects),
                    events: full_tx.events.map(|events| events.data),
                    transaction_data: Box::new(full_tx.transaction),
                    signatures: full_tx.signatures,
                    timestamp_ms,
                    cp_sequence_number: executed.checkpoint,
                    balance_changes: executed.balance_changes,
                };
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
impl Loader<TransactionTimestampKey> for PgReader {
    type Value = u64;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionTimestampKey],
    ) -> Result<HashMap<TransactionTimestampKey, Self::Value>, Error> {
        use kv_transactions::dsl as t;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let digests: BTreeSet<_> = keys.iter().map(|d| d.0.into_inner()).collect();
        let timestamps: Vec<(Vec<u8>, i64)> = conn
            .results(
                t::kv_transactions
                    .select((t::tx_digest, t::timestamp_ms))
                    .filter(t::tx_digest.eq_any(digests)),
            )
            .await?;

        let digest_to_timestamp: HashMap<_, _> = timestamps.into_iter().collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                Some((*key, *digest_to_timestamp.get(slice)? as u64))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl ChunkedLoader<TransactionTimestampKey> for LedgerGrpcReader {
    type Value = u64;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        MAX_BATCH_GET_TRANSACTIONS
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

            let digest = executed
                .digest
                .as_deref()
                .context("BatchGetTransactions response missing digest")?
                .parse::<TransactionDigest>()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_grpc_reader::test_support::assert_chunked;
    use crate::ledger_grpc_reader::test_support::mock_reader;

    #[tokio::test]
    async fn transaction_load_chunks_oversized_batches() {
        let (reader, mock, server) = mock_reader().await;
        let limit = MAX_BATCH_GET_TRANSACTIONS;

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
        let limit = MAX_BATCH_GET_TRANSACTIONS;

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
