// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use prometheus::Registry;
use tracing::info;

use sui_archival::reader::{ArchiveReader, ArchiveReaderMetrics};
use sui_config::node::ArchiveReaderConfig;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};

use crate::types::IndexerResult;

pub async fn read_next_checkpoint_after_epoch(
    cred_path: String,
    archive_bucket: Option<String>,
    epoch: u64,
) -> IndexerResult<u64> {
    let archive_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::GCS),
        bucket: archive_bucket,
        google_service_account: Some(cred_path.clone()),
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
    Ok(next_checkpoint_after_epoch)
}
