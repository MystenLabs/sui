// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer_builder::BackfillStrategy;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Display;
use std::str::FromStr;

pub mod indexer_builder;
pub mod sui_datasource;

#[derive(Clone, Debug)]
pub struct Task {
    pub task_name: String,
    pub task_type: TaskType,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskConfig {
    pub disable_live_task: bool,
    pub backfill_strategy: BackfillStrategy,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            disable_live_task: false,
            backfill_strategy: BackfillStrategy::Simple,
        }
    }
}

impl TaskConfig {
    pub fn with_backfill_strategy(mut self, backfill_strategy: BackfillStrategy) -> Self {
        self.backfill_strategy = backfill_strategy;
        self
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomTask {
    pub task_name: String,
    pub from_checkpoint: u64,
    pub target_checkpoint: u64,
}

#[derive(Clone, Debug)]
pub enum TaskType {
    Live,
    Backfill,
    Custom,
}

impl FromStr for TaskType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "live" => Self::Live,
            "backfill" => Self::Backfill,
            "custom" => Self::Custom,
            _ => return Err(anyhow!("Unreconized Task type: {s}")),
        })
    }
}

impl Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TaskType::Live => "live",
            TaskType::Backfill => "backfill",
            TaskType::Custom => "custom",
        };
        write!(f, "{s}")
    }
}
