// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use eyre::Result;
use rocksdb::MultiThreaded;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    cmp::Eq,
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::Arc,
};
use tokio::sync::{
    mpsc::{channel, Sender},
    oneshot,
};

pub mod traits;
pub use traits::Map;
pub mod metrics;
pub mod rocks;
pub use metrics::DBMetrics;

#[cfg(test)]
#[path = "tests/store_tests.rs"]
pub mod store_tests;

pub type StoreError = rocks::TypedStoreError;

type StoreResult<T> = Result<T, StoreError>;
type IterPredicate<Key, Value> = dyn Fn(&(Key, Value)) -> bool + Send;

pub enum StoreCommand<Key, Value> {
    Write(Key, Value, Option<oneshot::Sender<StoreResult<()>>>),
    WriteAll(Vec<(Key, Value)>, oneshot::Sender<StoreResult<()>>),
    Delete(Key),
    DeleteAll(Vec<Key>, oneshot::Sender<StoreResult<()>>),
    Read(Key, oneshot::Sender<StoreResult<Option<Value>>>),
    ReadRawBytes(Key, oneshot::Sender<StoreResult<Option<Vec<u8>>>>),
    ReadAll(Vec<Key>, oneshot::Sender<StoreResult<Vec<Option<Value>>>>),
    NotifyRead(Key, oneshot::Sender<StoreResult<Option<Value>>>),
    Iter(
        Option<Box<IterPredicate<Key, Value>>>,
        oneshot::Sender<HashMap<Key, Value>>,
    ),
}

#[derive(Clone)]
pub struct Store<K, V> {
    channel: Sender<StoreCommand<K, V>>,
    pub rocksdb: Arc<rocksdb::DBWithThreadMode<MultiThreaded>>,
}

impl<Key, Value> Store<Key, Value>
where
    Key: Hash + Eq + Serialize + DeserializeOwned + Send + 'static,
    Value: Serialize + DeserializeOwned + Send + Clone + 'static,
{
    pub fn new(keyed_db: rocks::DBMap<Key, Value>) -> Self {
        let mut obligations = HashMap::<Key, VecDeque<oneshot::Sender<_>>>::new();
        let clone_db = keyed_db.rocksdb.clone();
        let (tx, mut rx) = channel(100);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    StoreCommand::Write(key, value, sender) => {
                        let response = keyed_db.insert(&key, &value);
                        if response.is_ok() {
                            if let Some(mut senders) = obligations.remove(&key) {
                                while let Some(s) = senders.pop_front() {
                                    let _ = s.send(Ok(Some(value.clone())));
                                }
                            }
                        }
                        if let Some(replier) = sender {
                            let _ = replier.send(response);
                        }
                    }
                    StoreCommand::WriteAll(key_values, sender) => {
                        let response =
                            keyed_db.multi_insert(key_values.iter().map(|(k, v)| (k, v)));

                        if response.is_ok() {
                            for (key, _) in key_values {
                                if let Some(mut senders) = obligations.remove(&key) {
                                    while let Some(s) = senders.pop_front() {
                                        let _ = s.send(Ok(None));
                                    }
                                }
                            }
                        }
                        let _ = sender.send(response);
                    }
                    StoreCommand::Delete(key) => {
                        let _ = keyed_db.remove(&key);
                        if let Some(mut senders) = obligations.remove(&key) {
                            while let Some(s) = senders.pop_front() {
                                let _ = s.send(Ok(None));
                            }
                        }
                    }
                    StoreCommand::DeleteAll(keys, sender) => {
                        let response = keyed_db.multi_remove(keys.iter());
                        // notify the obligations only when the delete was successful
                        if response.is_ok() {
                            for key in keys {
                                if let Some(mut senders) = obligations.remove(&key) {
                                    while let Some(s) = senders.pop_front() {
                                        let _ = s.send(Ok(None));
                                    }
                                }
                            }
                        }
                        let _ = sender.send(response);
                    }
                    StoreCommand::Read(key, sender) => {
                        let response = keyed_db.get(&key);
                        let _ = sender.send(response);
                    }
                    StoreCommand::ReadAll(keys, sender) => {
                        let response = keyed_db.multi_get(keys.as_slice());
                        let _ = sender.send(response);
                    }
                    StoreCommand::NotifyRead(key, sender) => {
                        let response = keyed_db.get(&key);
                        if let Ok(Some(_)) = response {
                            let _ = sender.send(response);
                        } else {
                            obligations
                                .entry(key)
                                .or_insert_with(VecDeque::new)
                                .push_back(sender)
                        }
                    }
                    StoreCommand::Iter(predicate, sender) => {
                        let response = if let Some(func) = predicate {
                            keyed_db.iter().filter(func).collect()
                        } else {
                            // Beware, we may overload the memory with a large table!
                            keyed_db.iter().collect()
                        };

                        let _ = sender.send(response);
                    }
                    StoreCommand::ReadRawBytes(key, sender) => {
                        let response = keyed_db.get_raw_bytes(&key);
                        let _ = sender.send(response);
                    }
                }
            }
        });
        Self {
            channel: tx,
            rocksdb: clone_db,
        }
    }
}

impl<Key, Value> Store<Key, Value>
where
    Key: Serialize + DeserializeOwned + Send,
    Value: Serialize + DeserializeOwned + Send,
{
    pub async fn async_write(&self, key: Key, value: Value) {
        if let Err(e) = self
            .channel
            .send(StoreCommand::Write(key, value, None))
            .await
        {
            panic!("Failed to send Write command to store: {e}");
        }
    }

    pub async fn sync_write(&self, key: Key, value: Value) -> StoreResult<()> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .channel
            .send(StoreCommand::Write(key, value, Some(sender)))
            .await
        {
            panic!("Failed to send Write command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Write command from store")
    }

    /// Atomically writes all the key-value pairs in storage.
    /// If the operation is successful, then the result will be a non
    /// error empty result. Otherwise the error is returned.
    pub async fn sync_write_all(
        &self,
        key_value_pairs: impl IntoIterator<Item = (Key, Value)>,
    ) -> StoreResult<()> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .channel
            .send(StoreCommand::WriteAll(
                key_value_pairs.into_iter().collect(),
                sender,
            ))
            .await
        {
            panic!("Failed to send WriteAll command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to WriteAll command from store")
    }

    pub async fn remove(&self, key: Key) {
        if let Err(e) = self.channel.send(StoreCommand::Delete(key)).await {
            panic!("Failed to send Delete command to store: {e}");
        }
    }

    /// Atomically removes all the data referenced by the provided keys.
    /// If the operation is successful, then the result will be a non
    /// error empty result. Otherwise the error is returned.
    pub async fn remove_all(&self, keys: impl IntoIterator<Item = Key>) -> StoreResult<()> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .channel
            .send(StoreCommand::DeleteAll(keys.into_iter().collect(), sender))
            .await
        {
            panic!("Failed to send DeleteAll command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to RemoveAll command from store")
    }

    /// Returns the read value in raw bincode bytes
    pub async fn read_raw_bytes(&self, key: Key) -> StoreResult<Option<Vec<u8>>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .channel
            .send(StoreCommand::ReadRawBytes(key, sender))
            .await
        {
            panic!("Failed to send ReadRawBytes command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to ReadRawBytes command from store")
    }

    pub async fn read(&self, key: Key) -> StoreResult<Option<Value>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self.channel.send(StoreCommand::Read(key, sender)).await {
            panic!("Failed to send Read command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Read command from store")
    }

    /// Fetches all the values for the provided keys.
    pub async fn read_all(
        &self,
        keys: impl IntoIterator<Item = Key>,
    ) -> StoreResult<Vec<Option<Value>>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .channel
            .send(StoreCommand::ReadAll(keys.into_iter().collect(), sender))
            .await
        {
            panic!("Failed to send ReadAll command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to ReadAll command from store")
    }

    pub async fn notify_read(&self, key: Key) -> StoreResult<Option<Value>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .channel
            .send(StoreCommand::NotifyRead(key, sender))
            .await
        {
            panic!("Failed to send NotifyRead command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to NotifyRead command from store")
    }

    pub async fn iter(
        &self,
        predicate: Option<Box<IterPredicate<Key, Value>>>,
    ) -> HashMap<Key, Value> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .channel
            .send(StoreCommand::Iter(predicate, sender))
            .await
        {
            panic!("Failed to send Iter command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Iter command from store")
    }
}
