// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{NodeStorage, PayloadToken};
use config::WorkerId;
use mysten_common::sync::notify_read::NotifyRead;
use std::sync::Arc;
use store::reopen;
use store::rocks::{open_cf, MetricConf, ReadWriteOptions};
use store::{rocks::DBMap, Map, TypedStoreError};
use sui_macros::fail_point;
use types::BatchDigest;

/// Store of the batch digests for the primary node for the own created batches.
#[derive(Clone)]
pub struct PayloadStore {
    store: DBMap<(BatchDigest, WorkerId), PayloadToken>,

    /// Senders to notify for a write that happened for the specified batch digest and worker id
    notify_subscribers: Arc<NotifyRead<(BatchDigest, WorkerId), ()>>,
}

impl PayloadStore {
    pub fn new(payload_store: DBMap<(BatchDigest, WorkerId), PayloadToken>) -> Self {
        Self {
            store: payload_store,
            notify_subscribers: Arc::new(NotifyRead::new()),
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

    pub fn write(&self, digest: &BatchDigest, worker_id: &WorkerId) -> Result<(), TypedStoreError> {
        fail_point!("narwhal-store-before-write");

        self.store.insert(&(*digest, *worker_id), &0u8)?;
        self.notify_subscribers.notify(&(*digest, *worker_id), &());

        fail_point!("narwhal-store-after-write");
        Ok(())
    }

    /// Writes all the provided values atomically in store - either all will succeed or nothing will
    /// be stored.
    pub fn write_all(
        &self,
        keys: impl IntoIterator<Item = (BatchDigest, WorkerId)> + Clone,
    ) -> Result<(), TypedStoreError> {
        fail_point!("narwhal-store-before-write");

        self.store
            .multi_insert(keys.clone().into_iter().map(|e| (e, 0u8)))?;

        keys.into_iter().for_each(|(digest, worker_id)| {
            self.notify_subscribers.notify(&(digest, worker_id), &());
        });

        fail_point!("narwhal-store-after-write");
        Ok(())
    }

    /// Queries the store whether the batch with provided `digest` and `worker_id` exists. It returns
    /// `true` if exists, `false` otherwise.
    pub fn contains(
        &self,
        digest: BatchDigest,
        worker_id: WorkerId,
    ) -> Result<bool, TypedStoreError> {
        self.store
            .get(&(digest, worker_id))
            .map(|result| result.is_some())
    }

    /// When called the method will wait until the entry of batch with `digest` and `worker_id`
    /// becomes available.
    pub async fn notify_contains(
        &self,
        digest: BatchDigest,
        worker_id: WorkerId,
    ) -> Result<(), TypedStoreError> {
        let receiver = self.notify_subscribers.register_one(&(digest, worker_id));

        // let's read the value because we might have missed the opportunity
        // to get notified about it
        if self.contains(digest, worker_id)? {
            // notify any obligations - and remove the entries (including ours)
            self.notify_subscribers.notify(&(digest, worker_id), &());

            // reply directly
            return Ok(());
        }

        // now wait to hear back the result
        receiver.await;

        Ok(())
    }

    pub fn read_all(
        &self,
        keys: impl IntoIterator<Item = (BatchDigest, WorkerId)>,
    ) -> Result<Vec<Option<PayloadToken>>, TypedStoreError> {
        self.store.multi_get(keys)
    }

    #[allow(clippy::let_and_return)]
    pub fn remove_all(
        &self,
        keys: impl IntoIterator<Item = (BatchDigest, WorkerId)>,
    ) -> Result<(), TypedStoreError> {
        fail_point!("narwhal-store-before-write");

        let result = self.store.multi_remove(keys);

        fail_point!("narwhal-store-after-write");
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::PayloadStore;
    use fastcrypto::hash::Hash;
    use futures::future::join_all;
    use test_utils::latest_protocol_version;
    use types::Batch;

    #[tokio::test]
    async fn test_notify_read() {
        let store = PayloadStore::new_for_tests();

        // run the tests a few times
        let batch: Batch =
            test_utils::fixture_batch_with_transactions(10, &latest_protocol_version());
        let id = batch.digest();
        let worker_id = 0;

        // now populate a batch
        store.write(&id, &worker_id).unwrap();

        // now spawn a series of tasks before writing anything in store
        let mut handles = vec![];
        for _i in 0..5 {
            let cloned_store = store.clone();
            let handle =
                tokio::spawn(async move { cloned_store.notify_contains(id, worker_id).await });

            handles.push(handle)
        }

        // and populate the rest with a write_all
        store.write_all(vec![(id, worker_id)]).unwrap();

        // now asset the notify reads return with the result
        let result = join_all(handles).await;

        assert_eq!(result.len(), 5);

        for r in result {
            let token = r.unwrap();
            assert!(token.is_ok());
        }
    }
}
