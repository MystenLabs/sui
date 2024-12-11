// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod indexer_builder;
pub mod metrics;
pub mod progress;
pub mod sui_datasource;
pub const LIVE_TASK_TARGET_CHECKPOINT: i64 = i64::MAX;

#[derive(Clone, Debug)]
pub struct Task {
    pub task_name: String,
    pub start_checkpoint: u64,
    pub target_checkpoint: u64,
    pub timestamp: u64,
    pub is_live_task: bool,
}

impl Task {
    // TODO: this is really fragile and we should fix the task naming thing and storage schema asasp
    pub fn name_prefix(&self) -> &str {
        self.task_name.split(' ').next().unwrap_or("Unknown")
    }

    pub fn type_str(&self) -> &str {
        if self.is_live_task {
            "live"
        } else {
            "backfill"
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tasks {
    live_task: Option<Task>,
    backfill_tasks: Vec<Task>,
}

impl Tasks {
    pub fn new(tasks: Vec<Task>) -> anyhow::Result<Self> {
        let mut live_tasks = vec![];
        let mut backfill_tasks = vec![];
        for task in tasks {
            if task.is_live_task {
                live_tasks.push(task);
            } else {
                backfill_tasks.push(task);
            }
        }
        if live_tasks.len() > 1 {
            anyhow::bail!("More than one live task found: {:?}", live_tasks);
        }
        Ok(Self {
            live_task: live_tasks.pop(),
            backfill_tasks,
        })
    }

    pub fn live_task(&self) -> Option<Task> {
        self.live_task.clone()
    }

    pub fn backfill_tasks_ordered_desc(&self) -> Vec<Task> {
        let mut tasks = self.backfill_tasks.clone();
        tasks.sort_by(|t1, t2| t2.start_checkpoint.cmp(&t1.start_checkpoint));
        tasks
    }
}
