// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::state_sync::metrics::Metrics;
use anyhow::{Context, anyhow};
use backoff::ExponentialBackoff;
use object_store::ClientOptions;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::RetryConfig;
use object_store::aws::AmazonS3Builder;
use object_store::aws::AmazonS3ConfigKey;
use object_store::http::HttpBuilder;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use prost::Message;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_storage::verify_checkpoint;
use sui_types::base_types::ExecutionData;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VerifiedCheckpointContents;
use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
use sui_types::storage::WriteStore;
use sui_types::transaction::Transaction;

pub(crate) fn build_object_store(
    ingestion_url: &str,
    remote_store_options: Vec<(String, String)>,
) -> Arc<dyn ObjectStore> {
    let timeout_secs = 5;
    let client_options = ClientOptions::new()
        .with_timeout(Duration::from_secs(timeout_secs))
        .with_allow_http(true);
    let retry_config = RetryConfig {
        max_retries: 10,
        retry_timeout: Duration::from_secs(timeout_secs + 1),
        ..Default::default()
    };
    let url = ingestion_url
        .parse::<url::Url>()
        .expect("archival ingestion url must be valid");
    if url.scheme() == "file" {
        Arc::new(
            LocalFileSystem::new_with_prefix(
                url.to_file_path()
                    .expect("archival ingestion url must have a valid file path"),
            )
            .expect("failed to create local file system store"),
        )
    } else if url.host_str().unwrap_or_default().starts_with("s3") {
        let mut builder = AmazonS3Builder::new()
            .with_client_options(client_options)
            .with_retry(retry_config)
            .with_imdsv1_fallback()
            .with_url(ingestion_url);
        for (key, value) in &remote_store_options {
            builder = builder.with_config(
                AmazonS3ConfigKey::from_str(key).expect("invalid S3 config key"),
                value.clone(),
            );
        }
        Arc::new(builder.build().expect("failed to build S3 store"))
    } else {
        Arc::new(
            HttpBuilder::new()
                .with_url(url.to_string())
                .with_client_options(client_options)
                .with_retry(retry_config)
                .build()
                .expect("failed to build HTTP store"),
        )
    }
}

pub(crate) async fn fetch_checkpoint(
    store: &Arc<dyn ObjectStore>,
    seq: u64,
) -> anyhow::Result<Checkpoint> {
    let store = store.clone();
    let request = move || {
        let store = store.clone();
        async move {
            use backoff::Error as BE;
            let path = ObjectPath::from(format!("{seq}.binpb.zst"));
            let bytes = store
                .get(&path)
                .await
                .map_err(|e| match e {
                    object_store::Error::NotFound { .. } => {
                        BE::permanent(anyhow!("Checkpoint {seq} not found in archive"))
                    }
                    e => BE::transient(anyhow::Error::from(e)),
                })?
                .bytes()
                .await
                .map_err(|e| BE::transient(anyhow::Error::from(e)))?;
            let decompressed =
                zstd::decode_all(&bytes[..]).map_err(|e| BE::transient(anyhow::Error::from(e)))?;
            let proto_checkpoint = proto::Checkpoint::decode(&decompressed[..])
                .map_err(|e| BE::transient(anyhow::Error::from(e)))?;
            Checkpoint::try_from(&proto_checkpoint).map_err(|e| BE::transient(anyhow!(e)))
        }
    };
    let backoff = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(60)),
        multiplier: 1.0,
        ..Default::default()
    };
    backoff::future::retry(backoff, request).await
}

pub(crate) fn process_archive_checkpoint<S>(
    store: &S,
    checkpoint: &Checkpoint,
    metrics: &Metrics,
) -> anyhow::Result<()>
where
    S: WriteStore + Clone,
{
    let verified_checkpoint =
        get_or_insert_verified_checkpoint(store, checkpoint.summary.clone(), true)?;
    let full_contents = VersionedFullCheckpointContents::from_contents_and_execution_data(
        checkpoint.contents.clone(),
        checkpoint.transactions.iter().map(|t| ExecutionData {
            transaction: Transaction::from_generic_sig_data(
                t.transaction.clone(),
                t.signatures.clone(),
            ),
            effects: t.effects.clone(),
        }),
    );
    full_contents.verify_digests(verified_checkpoint.content_digest)?;
    let verified_contents = VerifiedCheckpointContents::new_unchecked(full_contents);
    store.insert_checkpoint_contents(&verified_checkpoint, verified_contents)?;
    store.update_highest_synced_checkpoint(&verified_checkpoint)?;
    metrics.update_checkpoints_synced_from_archive();
    Ok(())
}

pub fn get_or_insert_verified_checkpoint<S>(
    store: &S,
    certified_checkpoint: CertifiedCheckpointSummary,
    verify: bool,
) -> anyhow::Result<VerifiedCheckpoint>
where
    S: WriteStore + Clone,
{
    store
        .get_checkpoint_by_sequence_number(certified_checkpoint.sequence_number)
        .map(Ok::<VerifiedCheckpoint, anyhow::Error>)
        .unwrap_or_else(|| {
            let verified_checkpoint = if verify {
                // Verify checkpoint summary
                let prev_checkpoint_seq_num = certified_checkpoint
                    .sequence_number
                    .checked_sub(1)
                    .context("Checkpoint seq num underflow")?;
                let prev_checkpoint = store
                    .get_checkpoint_by_sequence_number(prev_checkpoint_seq_num)
                    .context(format!(
                        "Missing previous checkpoint {} in store",
                        prev_checkpoint_seq_num
                    ))?;

                verify_checkpoint(&prev_checkpoint, store, certified_checkpoint)
                    .map_err(|_| anyhow!("Checkpoint verification failed"))?
            } else {
                VerifiedCheckpoint::new_unchecked(certified_checkpoint)
            };
            // Insert checkpoint summary
            store
                .insert_checkpoint(&verified_checkpoint)
                .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"))?;
            // Update highest verified checkpoint watermark
            store
                .update_highest_verified_checkpoint(&verified_checkpoint)
                .expect("store operation should not fail");
            Ok::<VerifiedCheckpoint, anyhow::Error>(verified_checkpoint)
        })
        .map_err(|e| anyhow!("Failed to get verified checkpoint: {:?}", e))
}
