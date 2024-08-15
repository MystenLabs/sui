// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod indexer_builder;
pub mod sui_datasource;

#[derive(Clone, Debug)]
pub struct Task {
    pub task_name: String,
    pub checkpoint: u64,
    pub target_checkpoint: u64,
    pub timestamp: u64,
}

pub trait Tasks {
    fn live_task(&self) -> Option<Task>;

    fn backfill_tasks(&self) -> Vec<Task>;
}

impl Tasks for Vec<Task> {
    fn live_task(&self) -> Option<Task> {
        // TODO: Change the schema to record live task properly.
        self.iter()
            .find(|t| t.target_checkpoint == i64::MAX as u64)
            .cloned()
    }

    fn backfill_tasks(&self) -> Vec<Task> {
        self.iter()
            .filter(|t| t.target_checkpoint != i64::MAX as u64)
            .cloned()
            .collect()
    }
}
