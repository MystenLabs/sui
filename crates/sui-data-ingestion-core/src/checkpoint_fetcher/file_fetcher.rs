// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoint_fetcher::CheckpointFetcherTrait;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub(crate) struct FileFetcher {
    dir_path: PathBuf,
}

impl FileFetcher {
    pub fn new(dir_path: PathBuf) -> Self {
        Self { dir_path }
    }
}

#[async_trait::async_trait]
impl CheckpointFetcherTrait for FileFetcher {
    async fn fetch_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<(Arc<CheckpointData>, usize)> {
        let file_path = self.dir_path.join(format!("{}.chk", sequence_number));
        let bytes = fs::read(&file_path)?;
        let checkpoint = Blob::from_bytes::<Arc<CheckpointData>>(&bytes)?;
        let size = bytes.len();
        Ok((checkpoint, size))
    }
}
