// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::progress_store::ProgressStore;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Number, Value};
use std::path::PathBuf;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub struct FileProgressStore {
    path: PathBuf,
}

impl FileProgressStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl ProgressStore for FileProgressStore {
    async fn load(&mut self, task_name: String) -> Result<CheckpointSequenceNumber> {
        let content: Value = serde_json::from_slice(&std::fs::read(self.path.clone())?)?;
        Ok(content
            .get(&task_name)
            .and_then(|v| v.as_u64())
            .unwrap_or_default())
    }
    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<()> {
        let mut content: Value = serde_json::from_slice(&std::fs::read(self.path.clone())?)?;
        content[task_name] = Value::Number(Number::from(checkpoint_number));
        std::fs::write(self.path.clone(), serde_json::to_string_pretty(&content)?)?;
        Ok(())
    }
}
