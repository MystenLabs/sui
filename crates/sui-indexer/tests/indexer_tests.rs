// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use sui_indexer::errors::IndexerError;
use sui_indexer::models::checkpoints::Checkpoint;
use sui_indexer::models::objects::Object;
use sui_indexer::models::owners::OwnerChange;
use sui_indexer::store::{IndexerStore, TemporaryCheckpointStore, TemporaryEpochStore};
use sui_indexer::Indexer;
use test_utils::network::TestClusterBuilder;

#[tokio::test]
async fn test_genesis() {
    let test_cluster = TestClusterBuilder::new().build().await.unwrap();
    let store = InMemoryIndexerStore::new();

    let s = store.clone();
    let _handle = tokio::task::spawn(async move {
        Indexer::start(test_cluster.rpc_url(), &Registry::default(), s).await
    });

    // Allow indexer to process the data
    // TODO: are there better way?
    for _ in 1..3 {
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // TODO: add more test
    assert!(!store.tables.read().unwrap().objects.is_empty());
}

#[derive(Clone)]
struct InMemoryIndexerStore {
    tables: Arc<RwLock<Tables>>,
}

impl InMemoryIndexerStore {
    fn new() -> Self {
        Self {
            tables: Arc::new(RwLock::new(Tables::default())),
        }
    }
}

#[derive(Default, Clone, Debug)]
struct Tables {
    pub objects: Vec<Object>,
    pub owner_changes: Vec<OwnerChange>,
    pub checkpoints: Vec<Checkpoint>,
}

impl IndexerStore for InMemoryIndexerStore {
    fn get_latest_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        Ok(self.tables.read().unwrap().checkpoints.len() as i64 - 1)
    }

    fn get_checkpoint(&self, checkpoint_sequence_number: i64) -> Result<Checkpoint, IndexerError> {
        Ok(self.tables.read().unwrap().checkpoints[checkpoint_sequence_number as usize].clone())
    }

    fn persist_checkpoint(&self, data: &TemporaryCheckpointStore) -> Result<usize, IndexerError> {
        let TemporaryCheckpointStore {
            objects,
            owner_changes,
            checkpoint,
            ..
        } = data;

        let mut tables = self.tables.write().unwrap();
        tables.objects.extend(objects.clone());
        tables.owner_changes.extend(owner_changes.clone());
        tables.checkpoints.push(checkpoint.clone());
        Ok(0)
    }

    fn persist_epoch(&self, _data: &TemporaryEpochStore) -> Result<usize, IndexerError> {
        todo!()
    }

    fn log_errors(&self, _errors: Vec<IndexerError>) -> Result<(), IndexerError> {
        todo!()
    }
}
