// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use prometheus::Registry;
use sui_types::digests::CheckpointDigest;
use tracing::info;

use sui_archival::reader::{ArchiveReader, ArchiveReaderMetrics};
use sui_config::node::ArchiveReaderConfig;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};

use crate::errors::IndexerError;
use crate::types::IndexerResult;

#[derive(Clone, Debug)]
pub struct RestoreCheckpointInfo {
    pub next_checkpoint_after_epoch: u64,
    pub chain_identifier: CheckpointDigest,
}

pub async fn read_restore_checkpoint_info(
    archive_bucket: Option<String>,
    epoch: u64,
) -> IndexerResult<RestoreCheckpointInfo> {
    let archive_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::GCS),
        bucket: archive_bucket,
        object_store_connection_limit: 50,
        no_sign_request: false,
        ..Default::default()
    };
    let archive_reader_config = ArchiveReaderConfig {
        remote_store_config: archive_store_config,
        download_concurrency: NonZeroUsize::new(50).unwrap(),
        use_for_pruning_watermark: false,
    };
    let metrics = ArchiveReaderMetrics::new(&Registry::default());
    let archive_reader = ArchiveReader::new(archive_reader_config, &metrics)?;
    archive_reader.sync_manifest_once().await?;
    let manifest = archive_reader.get_manifest().await?;
    let next_checkpoint_after_epoch = manifest.next_checkpoint_after_epoch(epoch);
    info!(
        "Read from archives: next checkpoint sequence after epoch {} is: {}",
        epoch, next_checkpoint_after_epoch
    );
    let cp_summaries = archive_reader
        .get_summaries_for_list_no_verify(vec![0])
        .await
        .map_err(|e| IndexerError::ArchiveReaderError(format!("Failed to get summaries: {}", e)))?;
    let first_cp = cp_summaries
        .first()
        .ok_or_else(|| IndexerError::ArchiveReaderError("No checkpoint found".to_string()))?;
    let chain_identifier = *first_cp.digest();
    Ok(RestoreCheckpointInfo {
        next_checkpoint_after_epoch,
        chain_identifier,
    })
}
