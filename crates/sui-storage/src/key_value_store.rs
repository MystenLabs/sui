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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use std::collections::HashMap;
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::random_object_ref;
    use sui_types::crypto::{get_key_pair, AccountKeyPair};
    use sui_types::event::Event;
    use sui_types::message_envelope::Message;

    fn random_tx() -> Transaction {
        let (sender, key): (_, AccountKeyPair) = get_key_pair();
        let gas = random_object_ref();
        TestTransactionBuilder::new(sender, gas, 1)
            .transfer(random_object_ref(), sender)
            .build_and_sign(&key)
    }

    fn random_fx() -> TransactionEffects {
        let tx = random_tx();
        TransactionEffects::new_with_tx(&tx)
    }

    fn random_events() -> TransactionEvents {
        let event = Event::random_for_testing();
        TransactionEvents { data: vec![event] }
    }

    struct MockTxStore {
        txs: HashMap<TransactionDigest, Transaction>,
        fxs: HashMap<TransactionEffectsDigest, TransactionEffects>,
        events: HashMap<TransactionEventsDigest, TransactionEvents>,
    }

    impl MockTxStore {
        fn new() -> Self {
            Self {
                txs: HashMap::new(),
                fxs: HashMap::new(),
                events: HashMap::new(),
            }
        }

        fn add_tx(&mut self, tx: Transaction) {
            self.txs.insert(*tx.digest(), tx);
        }

        fn add_fx(&mut self, fx: TransactionEffects) {
            self.fxs.insert(fx.digest(), fx);
        }

        fn add_events(&mut self, events: TransactionEvents) {
            self.events.insert(events.digest(), events);
        }
    }

    #[async_trait]
    impl TransactionKeyValueStore for MockTxStore {
        async fn multi_get(&self, keys: &[Key]) -> SuiResult<Vec<Option<Value>>> {
            let mut values = Vec::new();
            for key in keys {
                let value = match key {
                    Key::Tx(digest) => self.txs.get(digest).map(|tx| Value::Tx(tx.clone().into())),
                    Key::Fx(digest) => self.fxs.get(digest).map(|fx| Value::Fx(fx.clone().into())),
                    Key::Events(digest) => self
                        .events
                        .get(digest)
                        .map(|events| Value::Events(events.clone().into())),
                };
                values.push(value);
            }
            Ok(values)
        }
    }

    #[test]
    fn test_get_tx() {
        let mut store = MockTxStore::new();
        let tx = random_tx();
        store.add_tx(tx.clone());

        let result = store.multi_get_tx(&[*tx.digest()]).now_or_never().unwrap();
        assert_eq!(result.unwrap(), vec![Some(tx)]);

        let result = store
            .multi_get_tx(&[TransactionDigest::random()])
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), vec![None]);
    }

    #[test]
    fn test_get_fx() {
        let mut store = MockTxStore::new();
        let fx = random_fx();
        store.add_fx(fx.clone());

        let result = store.multi_get_fx(&[fx.digest()]).now_or_never().unwrap();
        assert_eq!(result.unwrap(), vec![Some(fx)]);

        let result = store
            .multi_get_fx(&[TransactionEffectsDigest::random()])
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), vec![None]);
    }

    #[test]
    fn test_get_events() {
        let mut store = MockTxStore::new();
        let events = random_events();
        store.add_events(events.clone());

        let result = store
            .multi_get_events(&[events.digest()])
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), vec![Some(events)]);

        let result = store
            .multi_get_events(&[TransactionEventsDigest::random()])
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), vec![None]);
    }

    #[test]
    fn test_get_tx_from_fallback() {
        let mut store = MockTxStore::new();
        let tx = random_tx();
        store.add_tx(tx.clone());

        let mut fallback = MockTxStore::new();
        let fallback_tx = random_tx();
        fallback.add_tx(fallback_tx.clone());

        let fallback = FallbackTransactionKVStore::new(Box::new(store), Box::new(fallback));

        let result = fallback
            .multi_get_tx(&[*tx.digest()])
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), vec![Some(tx)]);

        let result = fallback
            .multi_get_tx(&[*fallback_tx.digest()])
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), vec![Some(fallback_tx)]);

        let result = fallback
            .multi_get_tx(&[TransactionDigest::random()])
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), vec![None]);
    }
}
