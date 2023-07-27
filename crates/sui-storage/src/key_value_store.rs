// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Immutable key/value store trait for storing/retrieving transactions, effects, and events
//! to/from a scalable.

use crate::sharded_lru::ShardedLruCache;
use async_trait::async_trait;
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult};
use sui_types::transaction::Transaction;
use tracing::error;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Key {
    Tx(TransactionDigest),
    Fx(TransactionEffectsDigest),
    Events(TransactionEventsDigest),

    // Separate key type so that you can do
    // multi_get(&[Key::Tx(tx_digest), Key::FxByTxDigest(tx_digest)])
    FxByTxDigest(TransactionDigest),
}

impl From<TransactionDigest> for Key {
    fn from(tx: TransactionDigest) -> Self {
        Key::Tx(tx)
    }
}

impl From<TransactionEffectsDigest> for Key {
    fn from(fx: TransactionEffectsDigest) -> Self {
        Key::Fx(fx)
    }
}

impl From<TransactionEventsDigest> for Key {
    fn from(events: TransactionEventsDigest) -> Self {
        Key::Events(events)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Tx(Box<Transaction>),
    Fx(Box<TransactionEffects>),
    Events(Box<TransactionEvents>),
}

impl From<Transaction> for Value {
    fn from(tx: Transaction) -> Self {
        Value::Tx(tx.into())
    }
}

impl From<TransactionEffects> for Value {
    fn from(fx: TransactionEffects) -> Self {
        Value::Fx(fx.into())
    }
}

impl From<TransactionEvents> for Value {
    fn from(events: TransactionEvents) -> Self {
        Value::Events(events.into())
    }
}

/// Immutable key/value store trait for storing/retrieving transactions, effects, and events.
/// Only defines multi_get/multi_put methods to discourage single key/value operations.
#[async_trait]
pub trait TransactionKeyValueStore {
    /// Generic multi_get, allows implementors to get heterogenous values with a single round trip.
    async fn multi_get(&self, keys: &[Key]) -> SuiResult<Vec<Option<Value>>>;

    // Convenience methods for getting transactions, effects, and events without converting
    // to/from Key and Value.
    async fn multi_get_tx(
        &self,
        keys: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Transaction>>> {
        let keys = keys.iter().map(|k| Key::Tx(*k)).collect::<Vec<_>>();
        self.multi_get(&keys).await.map(|values| {
            values
                .into_iter()
                .enumerate()
                .map(|(i, v)| match v {
                    Some(Value::Tx(tx)) => Some(*tx),
                    Some(_) => {
                        error!("Key {:?} had unexpected value type {:?}", keys[i], v);
                        None
                    }
                    _ => None,
                })
                .collect()
        })
    }

    async fn multi_get_fx(
        &self,
        keys: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        let keys = keys.iter().map(|k| Key::Fx(*k)).collect::<Vec<_>>();
        self.multi_get(&keys).await.map(|values| {
            values
                .into_iter()
                .enumerate()
                .map(|(i, v)| match v {
                    Some(Value::Fx(fx)) => Some(*fx),
                    Some(_) => {
                        error!("Key {:?} had unexpected value type {:?}", keys[i], v);
                        None
                    }
                    _ => None,
                })
                .collect()
        })
    }

    async fn multi_get_fx_by_tx_digest(
        &self,
        keys: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        let keys = keys
            .iter()
            .map(|k| Key::FxByTxDigest(*k))
            .collect::<Vec<_>>();
        self.multi_get(&keys).await.map(|values| {
            values
                .into_iter()
                .enumerate()
                .map(|(i, v)| match v {
                    Some(Value::Fx(fx)) => Some(*fx),
                    Some(_) => {
                        error!("Key {:?} had unexpected value type {:?}", keys[i], v);
                        None
                    }
                    _ => None,
                })
                .collect()
        })
    }

    async fn multi_get_events(
        &self,
        keys: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        let keys = keys.iter().map(|k| Key::Events(*k)).collect::<Vec<_>>();
        self.multi_get(&keys).await.map(|values| {
            values
                .into_iter()
                .enumerate()
                .map(|(i, v)| match v {
                    Some(Value::Events(events)) => Some(*events),
                    Some(_) => {
                        error!("Key {:?} had unexpected value type {:?}", keys[i], v);
                        None
                    }
                    _ => None,
                })
                .collect()
        })
    }

    /// Convenience method for fetching single digest, and returning an error if it's not found.
    /// Prefer using multi_get_tx whenever possible.
    async fn get_tx(&self, digest: TransactionDigest) -> SuiResult<Transaction> {
        self.multi_get_tx(&[digest])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or(SuiError::TransactionNotFound { digest })
    }

    /// Convenience method for fetching single digest, and returning an error if it's not found.
    /// Prefer using multi_get_fx_by_tx_digest whenever possible.
    async fn get_fx_by_tx_digest(
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

    /// Convenience method for fetching single digest, and returning an error if it's not found.
    /// Prefer using multi_get_events whenever possible.
    async fn get_events(&self, digest: TransactionEventsDigest) -> SuiResult<TransactionEvents> {
        self.multi_get_events(&[digest])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or(SuiError::TransactionEventsNotFound { digest })
    }
}

/// A TransactionKeyValueStore that falls back to a secondary store for any key for which the
/// primary store returns None.
///
/// Will be used to check the local rocksdb store, before falling back to a remote scalable store.
pub struct FallbackTransactionKVStore {
    primary: Box<dyn TransactionKeyValueStore + Send + Sync>,
    fallback: Box<dyn TransactionKeyValueStore + Send + Sync>,
}

impl FallbackTransactionKVStore {
    pub fn new(
        primary: Box<dyn TransactionKeyValueStore + Send + Sync>,
        fallback: Box<dyn TransactionKeyValueStore + Send + Sync>,
    ) -> Self {
        Self { primary, fallback }
    }
}

macro_rules! fallback_fetch {
    ($self:ident, $keys:ident, $method:ident) => {{
        let mut values = $self.primary.$method(&$keys).await?;
        let mut fallback_keys = Vec::new();
        let mut fallback_indices = Vec::new();

        for (i, value) in values.iter().enumerate() {
            if value.is_none() {
                fallback_keys.push($keys[i]);
                fallback_indices.push(i);
            }
        }

        if !fallback_keys.is_empty() {
            let fallback_values = $self.fallback.$method(&fallback_keys).await?;
            for (fallback_value, &index) in fallback_values.into_iter().zip(&fallback_indices) {
                values[index] = fallback_value;
            }
        }

        Ok(values)
    }};
}

#[async_trait]
impl TransactionKeyValueStore for FallbackTransactionKVStore {
    async fn multi_get(&self, keys: &[Key]) -> SuiResult<Vec<Option<Value>>> {
        fallback_fetch!(self, keys, multi_get)
    }

    async fn multi_get_tx(
        &self,
        keys: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Transaction>>> {
        fallback_fetch!(self, keys, multi_get_tx)
    }

    async fn multi_get_fx(
        &self,
        keys: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        fallback_fetch!(self, keys, multi_get_fx)
    }

    async fn multi_get_events(
        &self,
        keys: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        fallback_fetch!(self, keys, multi_get_events)
    }
}

pub struct CachingKVStore {
    store: Box<dyn TransactionKeyValueStore + Send + Sync>,
    cache: ShardedLruCache<Key, Value>,
}

impl CachingKVStore {
    pub fn new(
        store: Box<dyn TransactionKeyValueStore + Send + Sync>,
        cache: ShardedLruCache<Key, Value>,
    ) -> Self {
        Self { store, cache }
    }
}

#[async_trait]
impl TransactionKeyValueStore for CachingKVStore {
    async fn multi_get(&self, keys: &[Key]) -> SuiResult<Vec<Option<Value>>> {
        let mut values = self.cache.batch_get(keys.iter().cloned()).await;

        let mut missing_keys = Vec::new();
        let mut missing_indices = Vec::new();
        for (i, value) in values.iter().enumerate() {
            if value.is_none() {
                missing_keys.push(keys[i]);
                missing_indices.push(i);
            }
        }
        if !missing_keys.is_empty() {
            let missing_values = self.store.multi_get(&missing_keys).await?;
            let mut new_key_values = Vec::new();
            for (missing_value, &index) in missing_values.into_iter().zip(&missing_indices) {
                values[index] = missing_value.clone();
                if let Some(missing_value) = missing_value {
                    new_key_values.push((keys[index], missing_value));
                }
            }
            if !new_key_values.is_empty() {
                self.cache.batch_set(new_key_values).await;
            }
        }
        Ok(values)
    }
}
