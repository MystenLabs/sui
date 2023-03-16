// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{NodeStorage, PayloadToken};
use config::WorkerId;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use store::reopen;
use store::rocks::{open_cf, MetricConf, ReadWriteOptions};
use store::{rocks::DBMap, Map, TypedStoreError};
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tracing::warn;
use types::BatchDigest;

/// Store of the batch digests for the primary node for the own created batches.
#[derive(Clone)]
pub struct PayloadStore {
    store: DBMap<(BatchDigest, WorkerId), PayloadToken>,

    /// Senders to notify for a write that happened for the specified batch digest and worker id
    notify_on_write_subscribers:
        Arc<DashMap<(BatchDigest, WorkerId), VecDeque<Sender<PayloadToken>>>>,
}

impl PayloadStore {
    pub fn new(payload_store: DBMap<(BatchDigest, WorkerId), PayloadToken>) -> PayloadStore {
        Self {
            store: payload_store,
            notify_on_write_subscribers: Arc::new(DashMap::new()),
        }
    }

    pub fn new_for_tests() -> PayloadStore {
        let rocksdb = open_cf(
            tempfile::tempdir().unwrap(),
            None,
            MetricConf::default(),
            &[NodeStorage::PAYLOAD_CF],
        )
        .expect("Cannot open database");
        let map =
            reopen!(&rocksdb, NodeStorage::PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);
        PayloadStore::new(map)
    }

    pub fn write(&self, digest: BatchDigest, worker_id: WorkerId) -> Result<(), TypedStoreError> {
        self.store.insert(&(digest, worker_id), &0u8)?;
        self.notify_subscribers(digest, worker_id, 0u8);
        Ok(())
    }

    pub fn write_all(
        &self,
        keys: impl IntoIterator<Item = (BatchDigest, WorkerId)> + Clone,
    ) -> Result<(), TypedStoreError> {
        self.store
            .multi_insert(keys.clone().into_iter().map(|e| (e, 0u8)))?;

        keys.into_iter().for_each(|(digest, worker_id)| {
            self.notify_subscribers(digest, worker_id, 0u8);
        });
        Ok(())
    }

    pub fn read(
        &self,
        digest: BatchDigest,
        worker_id: WorkerId,
    ) -> Result<Option<PayloadToken>, TypedStoreError> {
        self.store.get(&(digest, worker_id))
    }

    pub async fn notify_read(
        &self,
        digest: BatchDigest,
        worker_id: WorkerId,
    ) -> Result<PayloadToken, TypedStoreError> {
        // we register our interest to be notified with the value
        let (sender, receiver) = oneshot::channel();
        self.notify_on_write_subscribers
            .entry((digest, worker_id))
            .or_insert_with(VecDeque::new)
            .push_back(sender);

        // let's read the value because we might have missed the opportunity
        // to get notified about it
        if let Ok(Some(token)) = self.read(digest, worker_id) {
            // notify any obligations - and remove the entries
            self.notify_subscribers(digest, worker_id, token);

            // reply directly
            return Ok(token);
        }

        // now wait to hear back the result
        let result = receiver
            .await
            .expect("Irrecoverable error while waiting to receive the notify_read result");

        Ok(result)
    }

    pub fn read_all(
        &self,
        keys: impl IntoIterator<Item = (BatchDigest, WorkerId)>,
    ) -> Result<Vec<Option<PayloadToken>>, TypedStoreError> {
        self.store.multi_get(keys)
    }

    pub fn remove_all(
        &self,
        keys: impl IntoIterator<Item = (BatchDigest, WorkerId)>,
    ) -> Result<(), TypedStoreError> {
        self.store.multi_remove(keys)
    }

    fn notify_subscribers(&self, digest: BatchDigest, worker_id: WorkerId, value: PayloadToken) {
        if let Some((_, mut senders)) = self
            .notify_on_write_subscribers
            .remove(&(digest, worker_id))
        {
            while let Some(s) = senders.pop_front() {
                if s.send(value).is_err() {
                    warn!("Couldn't notify obligation for batch with digest {digest} & worker with id {worker_id}");
                }
            }
        }
    }
}
