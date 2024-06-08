// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_tables::LiveObject;
use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::state_accumulator::AccumulatorStore;
use std::path::PathBuf;
use sui_rest_api::CheckpointData;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::storage::error::Error as StorageError;
use tracing::{debug, info};
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::TypedStoreError;
use typed_store_derive::DBMapUtils;

/// RocksDB tables for the RestIndexStore
///
/// NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
/// - bounded in size by the live object set
/// - are prune-able and have corresponding logic in the `prune` function
#[derive(DBMapUtils)]
struct IndexStoreTables {
    empty: DBMap<(), ()>,

    // NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
    // - bounded in size by the live object set
    // - are prune-able and have corresponding logic in the `prune` function
}

impl IndexStoreTables {
    fn is_empty(&self) -> bool {
        true
    }

    fn init(
        &mut self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
    ) -> Result<(), StorageError> {
        info!("Initializing REST indexes");

        // Iterate through available, executed checkpoints that have yet to be pruned 
        // to initialize checkpoint and transaction based indexes.
        if let Some(highest_executed_checkpint) =
            checkpoint_store.get_highest_executed_checkpoint_seq_number()?
        {
            let lowest_available_checkpoint =
                checkpoint_store.get_highest_pruned_checkpoint_seq_number()?;

            for seq in lowest_available_checkpoint..=highest_executed_checkpint {
                //TODO
            }
        }

        // Iterate through live object set to initialize object-based indexes
        for object in authority_store
            .iter_live_object_set(false)
            .filter_map(LiveObject::to_normal)
        {
            //TODO
        }

        info!("Finished initializing REST indexes");

        Ok(())
    }

    /// Prune data from this Index
    fn prune(
        &self,
        checkpoint_contents_to_prune: &[CheckpointContents],
    ) -> Result<(), TypedStoreError> {
        Ok(())
    }

    /// Index a Checkpoint
    fn index_checkpoint(&self, checkpoint: &CheckpointData) -> Result<(), TypedStoreError> {
        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "indexing checkpoint"
        );

        //TODO

        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "finished indexing checkpoint"
        );

        Ok(())
    }
}

pub struct RestIndexStore {
    tables: IndexStoreTables,
}

impl RestIndexStore {
    pub fn new(
        path: PathBuf,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
    ) -> Self {
        let mut tables = IndexStoreTables::open_tables_read_write(
            path,
            MetricConf::new("rest-index"),
            None,
            None,
        );

        // If the index tables are empty then we need to populate them
        if tables.is_empty() {
            tables.init(authority_store, checkpoint_store).unwrap();
        }

        Self { tables }
    }

    pub fn new_without_init(path: PathBuf) -> Self {
        let tables = IndexStoreTables::open_tables_read_write(
            path,
            MetricConf::new("rest-index"),
            None,
            None,
        );

        Self { tables }
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    pub fn prune(
        &self,
        checkpoint_contents_to_prune: &[CheckpointContents],
    ) -> Result<(), TypedStoreError> {
        self.tables.prune(checkpoint_contents_to_prune)
    }

    pub fn index_checkpoint(&self, checkpoint: &CheckpointData) -> Result<(), TypedStoreError> {
        self.tables.index_checkpoint(checkpoint)
    }
}
