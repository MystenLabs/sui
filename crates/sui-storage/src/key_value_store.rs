// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Immutable key/value store trait for storing/retrieving transactions, effects, and events
//! to/from a scalable.

use crate::key_value_store_metrics::KeyValueStoreMetrics;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use sui_types::base_types::{ObjectID, SequenceNumber, VersionNumber};
use sui_types::digests::{CheckpointDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber,
};
use sui_types::object::Object;
use sui_types::transaction::Transaction;
use tracing::instrument;

pub type KVStoreTransactionData = (Vec<Option<Transaction>>, Vec<Option<TransactionEffects>>);

pub type KVStoreCheckpointData = (
    Vec<Option<CertifiedCheckpointSummary>>,
    Vec<Option<CheckpointContents>>,
    Vec<Option<CertifiedCheckpointSummary>>,
);

pub struct TransactionKeyValueStore {
    store_name: &'static str,
    metrics: Arc<KeyValueStoreMetrics>,
    inner: Arc<dyn TransactionKeyValueStoreTrait + Send + Sync>,
}

impl TransactionKeyValueStore {
    pub fn new(
        store_name: &'static str,
        metrics: Arc<KeyValueStoreMetrics>,
        inner: Arc<dyn TransactionKeyValueStoreTrait + Send + Sync>,
    ) -> Self {
        Self {
            store_name,
            metrics,
            inner,
        }
    }

    /// Generic multi_get, allows implementors to get heterogenous values with a single round trip.
    pub async fn multi_get(
        &self,
        transactions: &[TransactionDigest],
        effects: &[TransactionDigest],
    ) -> SuiResult<(Vec<Option<Transaction>>, Vec<Option<TransactionEffects>>)> {
        let start = Instant::now();
        let res = self.inner.multi_get(transactions, effects).await;
        let elapsed = start.elapsed();

        let num_txns = transactions.len() as u64;
        let num_effects = effects.len() as u64;
        let total_keys = num_txns + num_effects;

        self.metrics
            .key_value_store_num_fetches_latency_ms
            .with_label_values(&[self.store_name, "tx"])
            .observe(elapsed.as_millis() as f64);
        self.metrics
            .key_value_store_num_fetches_batch_size
            .with_label_values(&[self.store_name, "tx"])
            .observe(total_keys as f64);

        if let Ok(res) = &res {
            let txns_not_found = res.0.iter().filter(|v| v.is_none()).count() as u64;
            let effects_not_found = res.1.iter().filter(|v| v.is_none()).count() as u64;

            if num_txns > 0 {
                self.metrics
                    .key_value_store_num_fetches_success
                    .with_label_values(&[self.store_name, "tx"])
                    .inc_by(num_txns);
            }
            if num_effects > 0 {
                self.metrics
                    .key_value_store_num_fetches_success
                    .with_label_values(&[self.store_name, "fx"])
                    .inc_by(num_effects);
            }

            if txns_not_found > 0 {
                self.metrics
                    .key_value_store_num_fetches_not_found
                    .with_label_values(&[self.store_name, "tx"])
                    .inc_by(txns_not_found);
            }
            if effects_not_found > 0 {
                self.metrics
                    .key_value_store_num_fetches_not_found
                    .with_label_values(&[self.store_name, "fx"])
                    .inc_by(effects_not_found);
            }
        } else {
            self.metrics
                .key_value_store_num_fetches_error
                .with_label_values(&[self.store_name, "tx"])
                .inc_by(num_txns);
            self.metrics
                .key_value_store_num_fetches_error
                .with_label_values(&[self.store_name, "fx"])
                .inc_by(num_effects);
        }

        res
    }

    pub async fn multi_get_checkpoints(
        &self,
        checkpoint_summaries: &[CheckpointSequenceNumber],
        checkpoint_contents: &[CheckpointSequenceNumber],
        checkpoint_summaries_by_digest: &[CheckpointDigest],
    ) -> SuiResult<(
        Vec<Option<CertifiedCheckpointSummary>>,
        Vec<Option<CheckpointContents>>,
        Vec<Option<CertifiedCheckpointSummary>>,
    )> {
        let start = Instant::now();
        let res = self
            .inner
            .multi_get_checkpoints(
                checkpoint_summaries,
                checkpoint_contents,
                checkpoint_summaries_by_digest,
            )
            .await;
        let elapsed = start.elapsed();

        let num_summaries =
            checkpoint_summaries.len() as u64 + checkpoint_summaries_by_digest.len() as u64;
        let num_contents = checkpoint_contents.len() as u64;

        self.metrics
            .key_value_store_num_fetches_latency_ms
            .with_label_values(&[self.store_name, "checkpoint"])
            .observe(elapsed.as_millis() as f64);
        self.metrics
            .key_value_store_num_fetches_batch_size
            .with_label_values(&[self.store_name, "checkpoint_summary"])
            .observe(num_summaries as f64);
        self.metrics
            .key_value_store_num_fetches_batch_size
            .with_label_values(&[self.store_name, "checkpoint_content"])
            .observe(num_contents as f64);

        if let Ok(res) = &res {
            let summaries_not_found = res.0.iter().filter(|v| v.is_none()).count() as u64
                + res.2.iter().filter(|v| v.is_none()).count() as u64;
            let contents_not_found = res.1.iter().filter(|v| v.is_none()).count() as u64;

            if num_summaries > 0 {
                self.metrics
                    .key_value_store_num_fetches_success
                    .with_label_values(&[self.store_name, "ckpt_summary"])
                    .inc_by(num_summaries);
            }
            if num_contents > 0 {
                self.metrics
                    .key_value_store_num_fetches_success
                    .with_label_values(&[self.store_name, "ckpt_contents"])
                    .inc_by(num_contents);
            }

            if summaries_not_found > 0 {
                self.metrics
                    .key_value_store_num_fetches_not_found
                    .with_label_values(&[self.store_name, "ckpt_summary"])
                    .inc_by(summaries_not_found);
            }
            if contents_not_found > 0 {
                self.metrics
                    .key_value_store_num_fetches_not_found
                    .with_label_values(&[self.store_name, "ckpt_contents"])
                    .inc_by(contents_not_found);
            }
        } else {
            self.metrics
                .key_value_store_num_fetches_error
                .with_label_values(&[self.store_name, "ckpt_summary"])
                .inc_by(num_summaries);
            self.metrics
                .key_value_store_num_fetches_error
                .with_label_values(&[self.store_name, "ckpt_contents"])
                .inc_by(num_contents);
        }

        res
    }

    pub async fn multi_get_checkpoints_summaries(
        &self,
        keys: &[CheckpointSequenceNumber],
    ) -> SuiResult<Vec<Option<CertifiedCheckpointSummary>>> {
        self.multi_get_checkpoints(keys, &[], &[])
            .await
            .map(|(summaries, _, _)| summaries)
    }

    pub async fn multi_get_checkpoints_contents(
        &self,
        keys: &[CheckpointSequenceNumber],
    ) -> SuiResult<Vec<Option<CheckpointContents>>> {
        self.multi_get_checkpoints(&[], keys, &[])
            .await
            .map(|(_, contents, _)| contents)
    }

    pub async fn multi_get_checkpoints_summaries_by_digest(
        &self,
        keys: &[CheckpointDigest],
    ) -> SuiResult<Vec<Option<CertifiedCheckpointSummary>>> {
        self.multi_get_checkpoints(&[], &[], keys)
            .await
            .map(|(_, _, summaries)| summaries)
    }

    pub async fn multi_get_tx(
        &self,
        keys: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Transaction>>> {
        self.multi_get(keys, &[]).await.map(|(txns, _)| txns)
    }

    pub async fn multi_get_fx_by_tx_digest(
        &self,
        keys: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        self.multi_get(&[], keys).await.map(|(_, fx)| fx)
    }

    /// Convenience method for fetching single digest, and returning an error if it's not found.
    /// Prefer using multi_get_tx whenever possible.
    pub async fn get_tx(&self, digest: TransactionDigest) -> SuiResult<Transaction> {
        self.multi_get_tx(&[digest])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or(SuiError::TransactionNotFound { digest })
    }

    /// Convenience method for fetching single digest, and returning an error if it's not found.
    /// Prefer using multi_get_fx_by_tx_digest whenever possible.
    pub async fn get_fx_by_tx_digest(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<TransactionEffects> {
        self.multi_get_fx_by_tx_digest(&[digest])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or(SuiError::TransactionNotFound { digest })
    }

    /// Convenience method for fetching single checkpoint, and returning an error if it's not found.
    /// Prefer using multi_get_checkpoints_summaries whenever possible.
    pub async fn get_checkpoint_summary(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<CertifiedCheckpointSummary> {
        self.multi_get_checkpoints_summaries(&[checkpoint])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointNotFound(checkpoint),
            })
    }

    /// Convenience method for fetching single checkpoint, and returning an error if it's not found.
    /// Prefer using multi_get_checkpoints_contents whenever possible.
    pub async fn get_checkpoint_contents(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<CheckpointContents> {
        self.multi_get_checkpoints_contents(&[checkpoint])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointNotFound(checkpoint),
            })
    }

    /// Convenience method for fetching single checkpoint, and returning an error if it's not found.
    /// Prefer using multi_get_checkpoints_summaries_by_digest whenever possible.
    pub async fn get_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> SuiResult<CertifiedCheckpointSummary> {
        self.multi_get_checkpoints_summaries_by_digest(&[digest])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointDigestNotFound(format!("{:?}", digest)),
            })
    }

    pub async fn deprecated_get_transaction_checkpoint(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        self.inner
            .deprecated_get_transaction_checkpoint(digest)
            .await
    }

    pub async fn get_object(
        &self,
        object_id: ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<Object>> {
        self.inner.get_object(object_id, version).await
    }

    pub async fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CheckpointSequenceNumber>>> {
        self.inner.multi_get_transaction_checkpoint(digests).await
    }

    pub async fn multi_get_events_by_tx_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        self.inner.multi_get_events_by_tx_digests(digests).await
    }
}

/// Immutable key/value store trait for storing/retrieving transactions, effects, and events.
/// Only defines multi_get/multi_put methods to discourage single key/value operations.
#[async_trait]
pub trait TransactionKeyValueStoreTrait {
    /// Generic multi_get, allows implementors to get heterogenous values with a single round trip.
    async fn multi_get(
        &self,
        transactions: &[TransactionDigest],
        effects: &[TransactionDigest],
    ) -> SuiResult<KVStoreTransactionData>;

    /// Generic multi_get to allow implementors to get heterogenous values with a single round trip.
    async fn multi_get_checkpoints(
        &self,
        checkpoint_summaries: &[CheckpointSequenceNumber],
        checkpoint_contents: &[CheckpointSequenceNumber],
        checkpoint_summaries_by_digest: &[CheckpointDigest],
    ) -> SuiResult<KVStoreCheckpointData>;

    async fn deprecated_get_transaction_checkpoint(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<Option<CheckpointSequenceNumber>>;

    async fn get_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>>;

    async fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CheckpointSequenceNumber>>>;

    async fn multi_get_events_by_tx_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>>;
}

/// A TransactionKeyValueStoreTrait that falls back to a secondary store for any key for which the
/// primary store returns None.
///
/// Will be used to check the local rocksdb store, before falling back to a remote scalable store.
pub struct FallbackTransactionKVStore {
    primary: TransactionKeyValueStore,
    fallback: TransactionKeyValueStore,
}

impl FallbackTransactionKVStore {
    pub fn new_kv(
        primary: TransactionKeyValueStore,
        fallback: TransactionKeyValueStore,
        metrics: Arc<KeyValueStoreMetrics>,
        label: &'static str,
    ) -> TransactionKeyValueStore {
        let store = Arc::new(Self { primary, fallback });
        TransactionKeyValueStore::new(label, metrics, store)
    }
}

#[async_trait]
impl TransactionKeyValueStoreTrait for FallbackTransactionKVStore {
    #[instrument(level = "trace", skip_all)]
    async fn multi_get(
        &self,
        transactions: &[TransactionDigest],
        effects: &[TransactionDigest],
    ) -> SuiResult<(Vec<Option<Transaction>>, Vec<Option<TransactionEffects>>)> {
        let mut res = self.primary.multi_get(transactions, effects).await?;

        let (fallback_transactions, indices_transactions) = find_fallback(&res.0, transactions);
        let (fallback_effects, indices_effects) = find_fallback(&res.1, effects);

        if fallback_transactions.is_empty() && fallback_effects.is_empty() {
            return Ok(res);
        }

        let secondary_res = self
            .fallback
            .multi_get(&fallback_transactions, &fallback_effects)
            .await?;

        merge_res(&mut res.0, secondary_res.0, &indices_transactions);
        merge_res(&mut res.1, secondary_res.1, &indices_effects);

        Ok((res.0, res.1))
    }

    #[instrument(level = "trace", skip_all)]
    async fn multi_get_checkpoints(
        &self,
        checkpoint_summaries: &[CheckpointSequenceNumber],
        checkpoint_contents: &[CheckpointSequenceNumber],
        checkpoint_summaries_by_digest: &[CheckpointDigest],
    ) -> SuiResult<(
        Vec<Option<CertifiedCheckpointSummary>>,
        Vec<Option<CheckpointContents>>,
        Vec<Option<CertifiedCheckpointSummary>>,
    )> {
        let mut res = self
            .primary
            .multi_get_checkpoints(
                checkpoint_summaries,
                checkpoint_contents,
                checkpoint_summaries_by_digest,
            )
            .await?;

        let (fallback_summaries, indices_summaries) = find_fallback(&res.0, checkpoint_summaries);
        let (fallback_contents, indices_contents) = find_fallback(&res.1, checkpoint_contents);
        let (fallback_summaries_by_digest, indices_summaries_by_digest) =
            find_fallback(&res.2, checkpoint_summaries_by_digest);

        if fallback_summaries.is_empty()
            && fallback_contents.is_empty()
            && fallback_summaries_by_digest.is_empty()
        {
            return Ok(res);
        }

        let secondary_res = self
            .fallback
            .multi_get_checkpoints(
                &fallback_summaries,
                &fallback_contents,
                &fallback_summaries_by_digest,
            )
            .await?;

        merge_res(&mut res.0, secondary_res.0, &indices_summaries);
        merge_res(&mut res.1, secondary_res.1, &indices_contents);
        merge_res(&mut res.2, secondary_res.2, &indices_summaries_by_digest);

        Ok((res.0, res.1, res.2))
    }

    #[instrument(level = "trace", skip_all)]
    async fn deprecated_get_transaction_checkpoint(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        let mut res = self
            .primary
            .deprecated_get_transaction_checkpoint(digest)
            .await?;
        if res.is_none() {
            res = self
                .fallback
                .deprecated_get_transaction_checkpoint(digest)
                .await?;
        }
        Ok(res)
    }

    #[instrument(level = "trace", skip_all)]
    async fn get_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let mut res = self.primary.get_object(object_id, version).await?;
        if res.is_none() {
            res = self.fallback.get_object(object_id, version).await?;
        }
        Ok(res)
    }

    #[instrument(level = "trace", skip_all)]
    async fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CheckpointSequenceNumber>>> {
        let mut res = self
            .primary
            .multi_get_transaction_checkpoint(digests)
            .await?;

        let (fallback, indices) = find_fallback(&res, digests);

        if fallback.is_empty() {
            return Ok(res);
        }

        let secondary_res = self
            .fallback
            .multi_get_transaction_checkpoint(&fallback)
            .await?;

        merge_res(&mut res, secondary_res, &indices);

        Ok(res)
    }

    #[instrument(level = "trace", skip_all)]
    async fn multi_get_events_by_tx_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        let mut res = self.primary.multi_get_events_by_tx_digests(digests).await?;
        let (fallback, indices) = find_fallback(&res, digests);
        if fallback.is_empty() {
            return Ok(res);
        }
        let secondary_res = self
            .fallback
            .multi_get_events_by_tx_digests(&fallback)
            .await?;
        merge_res(&mut res, secondary_res, &indices);
        Ok(res)
    }
}

fn find_fallback<T, K: Clone>(values: &[Option<T>], keys: &[K]) -> (Vec<K>, Vec<usize>) {
    let num_nones = values.iter().filter(|v| v.is_none()).count();
    let mut fallback_keys = Vec::with_capacity(num_nones);
    let mut fallback_indices = Vec::with_capacity(num_nones);
    for (i, value) in values.iter().enumerate() {
        if value.is_none() {
            fallback_keys.push(keys[i].clone());
            fallback_indices.push(i);
        }
    }
    (fallback_keys, fallback_indices)
}

fn merge_res<T>(values: &mut [Option<T>], fallback_values: Vec<Option<T>>, indices: &[usize]) {
    for (&index, fallback_value) in indices.iter().zip(fallback_values) {
        values[index] = fallback_value;
    }
}
