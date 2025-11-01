// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};

use anyhow::Context;
use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use prost_types::FieldMask;
use sui_indexer_alt_schema::{schema::kv_transactions, transactions::StoredTransaction};
use sui_kvstore::TransactionData;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc::{field::FieldMaskUtil, proto::proto_to_timestamp_ms};
use sui_types::digests::TransactionDigest;

use crate::ledger_grpc_reader::{CheckpointedTransaction, LedgerGrpcReader};
use crate::{bigtable_reader::BigtableReader, error::Error, pg_reader::PgReader};

/// Key for fetching transaction contents (TransactionData, Effects, and Events) by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionKey(pub TransactionDigest);

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
impl Loader<TransactionKey> for BigtableReader {
    type Value = TransactionData;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionKey],
    ) -> Result<HashMap<TransactionKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let digests: Vec<_> = keys.iter().map(|k| k.0).collect();
        Ok(self
            .transactions(&digests)
            .await?
            .into_iter()
            .map(|t| (TransactionKey(*t.transaction.digest()), t))
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<TransactionKey> for LedgerGrpcReader {
    type Value = CheckpointedTransaction;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionKey],
    ) -> Result<HashMap<TransactionKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

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
        ]));

        let response = self.0.clone().batch_get_transactions(request).await?;
        let batch_response = response.into_inner();

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
