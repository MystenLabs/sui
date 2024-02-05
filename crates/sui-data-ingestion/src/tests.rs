// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::progress_store::ExecutorProgress;
use crate::reader::ENV_VAR_LOCAL_READ_TIMEOUT_MS;
use crate::workers::Worker;
use crate::{
    ArchivalConfig, ArchivalWorker, DataIngestionMetrics, FileProgressStore, IndexerExecutor,
    WorkerPool,
};
use anyhow::Result;
use async_trait::async_trait;
use object_store::parse_url_opts;
use prometheus::Registry;
use rand::prelude::StdRng;
use rand::SeedableRng;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_archival::verify_archive_with_checksums2;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::crypto::KeypairTraits;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
    SignedCheckpointSummary,
};
use sui_types::utils::make_committee_key;
use tempfile::NamedTempFile;
use tokio::sync::oneshot;
use url::Url;

async fn add_worker_pool<W: Worker + 'static>(
    indexer: &mut IndexerExecutor<FileProgressStore>,
    worker: W,
    concurrency: usize,
) -> Result<()> {
    let worker_pool = WorkerPool::new(worker, "test".to_string(), concurrency);
    indexer.register(worker_pool).await?;
    Ok(())
}

async fn run(
    indexer: IndexerExecutor<FileProgressStore>,
    path: Option<PathBuf>,
    duration: Option<Duration>,
) -> Result<ExecutorProgress> {
    std::env::set_var(ENV_VAR_LOCAL_READ_TIMEOUT_MS, "10");
    let (sender, recv) = oneshot::channel();
    let result = match duration {
        None => {
            indexer
                .run(path.unwrap_or_else(temp_dir), None, vec![], 1, recv)
                .await
        }
        Some(duration) => {
            let handle = tokio::task::spawn(async move {
                indexer
                    .run(path.unwrap_or_else(temp_dir), None, vec![], 1, recv)
                    .await
            });
            tokio::time::sleep(duration).await;
            drop(sender);
            handle.await?
        }
    };
    std::env::remove_var(ENV_VAR_LOCAL_READ_TIMEOUT_MS);
    result
}

struct ExecutorBundle {
    executor: IndexerExecutor<FileProgressStore>,
    _progress_file: NamedTempFile,
}

#[derive(Clone)]
struct TestWorker;

#[async_trait]
impl Worker for TestWorker {
    async fn process_checkpoint(&self, _checkpoint: CheckpointData) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn empty_pools() {
    let bundle = create_executor_bundle();
    let result = run(bundle.executor, None, None).await;
    assert!(result.is_err());
    if let Err(err) = result {
        assert!(err.to_string().contains("pools can't be empty"));
    }
}

#[tokio::test]
async fn basic_flow() {
    let mut bundle = create_executor_bundle();
    add_worker_pool(&mut bundle.executor, TestWorker, 5)
        .await
        .unwrap();
    let path = temp_dir();
    for checkpoint_number in 0..20 {
        let bytes = mock_checkpoint_data_bytes(checkpoint_number);
        std::fs::write(path.join(format!("{}.chk", checkpoint_number)), bytes).unwrap();
    }
    let result = run(bundle.executor, Some(path), Some(Duration::from_secs(1))).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().get("test"), Some(&20));
}

#[tokio::test]
async fn archival_flow() {
    let mut bundle = create_executor_bundle();
    let remote_url = "https://s3.us-east-1.amazonaws.com/tmpingestion".to_string();
    let remote_store_options = vec![
        ("aws_access_key_id".to_string(), "fake".to_string()),
        ("aws_secret_access_key".to_string(), "fake".to_string()),
    ];
    let config = ArchivalConfig {
        remote_url: remote_url.clone(),
        remote_store_options: remote_store_options.clone(),
        commit_file_size: 1,
        commit_duration_seconds: 3,
    };
    let path = temp_dir();
    let worker = ArchivalWorker::new(config).await.unwrap();
    add_worker_pool(&mut bundle.executor, worker, 1)
        .await
        .unwrap();
    for checkpoint_number in 0..20 {
        let bytes = mock_checkpoint_data_bytes(checkpoint_number);
        std::fs::write(path.join(format!("{}.chk", checkpoint_number)), bytes).unwrap();
    }
    let result = run(bundle.executor, Some(path), Some(Duration::from_secs(10))).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().get("test"), Some(&20));

    let remote_store = parse_url_opts(&Url::parse(&remote_url).unwrap(), remote_store_options)
        .unwrap()
        .0;
    verify_archive_with_checksums2("tmpingestion".to_string(), Arc::new(remote_store))
        .await
        .unwrap();
}

fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

fn create_executor_bundle() -> ExecutorBundle {
    let progress_file = NamedTempFile::new().unwrap();
    let path = progress_file.path().to_path_buf();
    std::fs::write(path.clone(), "{}").unwrap();
    let progress_store = FileProgressStore::new(path);
    let executor = IndexerExecutor::new(
        progress_store,
        1,
        DataIngestionMetrics::new(&Registry::new()),
    );
    ExecutorBundle {
        executor,
        _progress_file: progress_file,
    }
}

const RNG_SEED: [u8; 32] = [
    21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130, 157,
    179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
];

fn mock_checkpoint_data_bytes(seq_number: CheckpointSequenceNumber) -> Vec<u8> {
    let mut rng = StdRng::from_seed(RNG_SEED);
    let (keys, committee) = make_committee_key(&mut rng);
    let contents = CheckpointContents::new_with_digests_only_for_tests(vec![]);
    let summary = CheckpointSummary::new(
        0,
        seq_number,
        0,
        &contents,
        None,
        GasCostSummary::default(),
        None,
        0,
    );

    let sign_infos: Vec<_> = keys
        .iter()
        .map(|k| {
            let name = k.public().into();
            SignedCheckpointSummary::sign(committee.epoch, &summary, k, name)
        })
        .collect();

    let checkpoint_data = CheckpointData {
        checkpoint_summary: CertifiedCheckpointSummary::new(summary, sign_infos, &committee)
            .unwrap(),
        checkpoint_contents: contents,
        transactions: vec![],
    };
    Blob::encode(&checkpoint_data, BlobEncoding::Bcs)
        .unwrap()
        .to_bytes()
}
