// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use sui_indexer::errors::IndexerError;
use sui_indexer::models::checkpoints::Checkpoint;
use sui_indexer::models::objects::Object;
use sui_indexer::models::owners::OwnerChange;
use sui_indexer::models::transactions::Transaction;
use sui_indexer::store::{IndexerStore, TemporaryCheckpointStore, TemporaryEpochStore};
use sui_indexer::Indexer;
use sui_json_rpc_types::CheckpointId;
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

    fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, IndexerError> {
        Ok(match id {
            CheckpointId::SequenceNumber(seq) => {
                self.tables.read().unwrap().checkpoints[seq as usize].clone()
            }
            CheckpointId::Digest(digest) => self
                .tables
                .read()
                .unwrap()
                .checkpoints
                .iter()
                .find(|c| c.checkpoint_digest == digest.base58_encode())
                .unwrap()
                .clone(),
        })
    }

    fn get_total_transaction_number(&self) -> Result<i64, IndexerError> {
        todo!()
    }

    fn get_latest_move_call_sequence_number(&self) -> Result<i64, IndexerError> {
        todo!()
    }

    fn get_latest_transaction_sequence_number(&self) -> Result<i64, IndexerError> {
        todo!()
    }

    fn get_transaction_by_digest(&self, _txn_digest: String) -> Result<Transaction, IndexerError> {
        todo!()
    }

    fn get_transaction_sequence_by_digest(
        &self,
        _txn_digest: Option<String>,
        _is_descending: bool,
    ) -> Result<i64, IndexerError> {
        todo!()
    }

    fn get_move_call_sequence_by_digest(
        &self,
        _txn_digest: Option<String>,
        _is_descending: bool,
    ) -> Result<i64, IndexerError> {
        todo!()
    }

    fn get_all_transaction_digest_page(
        &self,
        _start_sequence: i64,
        _limit: usize,
        _is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        todo!()
    }

    fn get_transaction_digest_page_by_mutated_object(
        &self,
        _object_id: String,
        _start_sequence: i64,
        _limit: usize,
        _is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        todo!()
    }

    fn get_transaction_digest_page_by_sender_address(
        &self,
        _sender_address: String,
        _start_sequence: i64,
        _limit: usize,
        _is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        todo!()
    }

    fn get_transaction_digest_page_by_move_call(
        &self,
        _package: String,
        _module: Option<String>,
        _function: Option<String>,
        _start_sequence: i64,
        _limit: usize,
        _is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        todo!()
    }

    fn get_transaction_digest_page_by_recipient_address(
        &self,
        _recipient_address: String,
        _start_sequence: i64,
        _limit: usize,
        _is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        todo!()
    }

    fn read_transactions(
        &self,
        _last_processed_id: i64,
        _limit: usize,
    ) -> Result<Vec<Transaction>, IndexerError> {
        todo!()
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
