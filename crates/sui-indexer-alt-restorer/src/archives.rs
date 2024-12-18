// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use prometheus::Registry;
use sui_types::digests::CheckpointDigest;
use tracing::info;

use sui_archival::reader::{ArchiveReader, ArchiveReaderMetrics};
use sui_config::node::ArchiveReaderConfig;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};

use crate::Args;

#[derive(Clone, Debug)]
pub(crate) struct ArchivalCheckpointInfo {
    pub next_checkpoint_after_epoch: u64,
    // TODO(gegaowp): unused for now, will be used for kv_genesis.
    #[allow(unused)]
    pub chain_identifier: CheckpointDigest,
}

impl ArchivalCheckpointInfo {
    /// Reads checkpoint information from archival storage to determine:
    /// - The next checkpoint number after `start_epoch` for watermarking;
    /// - The genesis checkpoint digest for kv_genesis.
    pub async fn read_archival_checkpoint_info(args: &Args) -> anyhow::Result<Self> {
        // Configure GCS object store and initialize archive reader with minimal concurrency.
        let archive_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::GCS),
            bucket: Some(args.archive_bucket.clone()),
            object_store_connection_limit: 1, // 1 connection is sufficient
            no_sign_request: false,
            ..Default::default()
        };
        let archive_reader_config = ArchiveReaderConfig {
            remote_store_config: archive_store_config,
            download_concurrency: NonZeroUsize::new(1).unwrap(),
            use_for_pruning_watermark: false,
        };
        let metrics = ArchiveReaderMetrics::new(&Registry::default());
        let archive_reader = ArchiveReader::new(archive_reader_config, &metrics)?;

        // Sync and read manifest to get next checkpoint after `start_epoch` for watermarking.
        archive_reader.sync_manifest_once().await?;
        let manifest = archive_reader.get_manifest().await?;
        let next_checkpoint_after_epoch = manifest.next_checkpoint_after_epoch(args.start_epoch);
        info!(
            epoch = args.start_epoch,
            checkpoint = next_checkpoint_after_epoch,
            "Next checkpoint after epoch",
        );

        // Get genesis checkpoint (checkpoint 0) to determine chain identifier for kv_genesis.
        let cp_summaries = archive_reader
            .get_summaries_for_list_no_verify(vec![0])
            .await?;
        let first_cp = cp_summaries
            .first()
            .ok_or_else(|| anyhow::anyhow!("No checkpoint found"))?;
        let chain_identifier = *first_cp.digest();
        Ok(Self {
            next_checkpoint_after_epoch,
            chain_identifier,
        })
    }
}
