// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{NodeStorage, NotifySubscribers, PayloadToken};
use config::WorkerId;
use store::reopen;
use store::rocks::{open_cf, MetricConf, ReadWriteOptions};
use store::{rocks::DBMap, Map, TypedStoreError};
use types::BatchDigest;

/// Store of the batch digests for the primary node for the own created batches.
#[derive(Clone)]
pub struct PayloadStore {
    store: DBMap<(BatchDigest, WorkerId), PayloadToken>,

    /// Senders to notify for a write that happened for the specified batch digest and worker id
    notify_subscribers: NotifySubscribers<(BatchDigest, WorkerId), PayloadToken>,
}

impl PayloadStore {
    pub fn new(payload_store: DBMap<(BatchDigest, WorkerId), PayloadToken>) -> Self {
        Self {
            store: payload_store,
            notify_subscribers: NotifySubscribers::new(),
        }
    }

    pub fn new_for_tests() -> Self {
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
        self.notify_subscribers.notify(&(digest, worker_id), &0u8);
        Ok(())
    }

    pub fn write_all(
        &self,
        keys: impl IntoIterator<Item = (BatchDigest, WorkerId)> + Clone,
    ) -> Result<(), TypedStoreError> {
        self.store
            .multi_insert(keys.clone().into_iter().map(|e| (e, 0u8)))?;

        keys.into_iter().for_each(|(digest, worker_id)| {
            self.notify_subscribers.notify(&(digest, worker_id), &0u8);
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
        let receiver = self.notify_subscribers.subscribe(&(digest, worker_id));

        // let's read the value because we might have missed the opportunity
        // to get notified about it
        if let Ok(Some(token)) = self.read(digest, worker_id) {
            // notify any obligations - and remove the entries (including ours)
            self.notify_subscribers.notify(&(digest, worker_id), &token);

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
}
