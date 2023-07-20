// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Immutable key/value store trait for storing/retrieving transactions, effects, and events
//! to/from a scalable.

use async_trait::async_trait;
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::transaction::Transaction;
use tracing::error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tx(TransactionDigest),
    Fx(TransactionEffectsDigest),
    Events(TransactionEventsDigest),
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

    /// Generic multi_put, allows implementors to put heterogenous values with a single round trip.
    async fn multi_put(&self, keys: &[Key], values: &[Value]) -> SuiResult;

    // Convenience methods for getting/putting transactions, effects, and events without converting
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
    async fn multi_put_tx(&self, keys: &[TransactionDigest], values: &[Transaction]) -> SuiResult {
        let keys = keys.iter().map(|k| Key::Tx(*k)).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|v| Value::Tx(v.clone().into()))
            .collect::<Vec<_>>();
        self.multi_put(&keys, &values).await
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
    async fn multi_put_fx(
        &self,
        keys: &[TransactionEffectsDigest],
        values: &[TransactionEffects],
    ) -> SuiResult {
        let keys = keys.iter().map(|k| Key::Fx(*k)).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|v| Value::Fx(v.clone().into()))
            .collect::<Vec<_>>();
        self.multi_put(&keys, &values).await
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
    async fn multi_put_events(
        &self,
        keys: &[TransactionEventsDigest],
        values: &[TransactionEvents],
    ) -> SuiResult {
        let keys = keys.iter().map(|k| Key::Events(*k)).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|v| Value::Events(v.clone().into()))
            .collect::<Vec<_>>();
        self.multi_put(&keys, &values).await
    }
}
