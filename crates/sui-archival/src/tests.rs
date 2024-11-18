// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::reader::{ArchiveReader, ArchiveReaderMetrics};
use crate::writer::ArchiveWriter;
use crate::{read_manifest, verify_archive_with_local_store, write_manifest, Manifest};
use anyhow::{anyhow, Context, Result};
use more_asserts as ma;
use object_store::DynObjectStore;
use prometheus::Registry;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use sui_config::node::ArchiveReaderConfig;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_storage::object_store::util::path_to_filesystem;
use sui_storage::{FileCompression, StorageFormat};
use sui_swarm_config::test_utils::{empty_contents, CommitteeFixture};
use sui_types::messages_checkpoint::{VerifiedCheckpoint, VerifiedCheckpointContents};
use sui_types::storage::{ReadStore, SharedInMemoryStore, SingleCheckpointSharedInMemoryStore};
use tempfile::tempdir;

struct TestState {
    archive_writer: ArchiveWriter,
    archive_reader: ArchiveReader,
    local_path: PathBuf,
    remote_path: PathBuf,
    local_store: Arc<DynObjectStore>,
    remote_store: Arc<DynObjectStore>,
    local_store_config: ObjectStoreConfig,
    remote_store_config: ObjectStoreConfig,
    committee: CommitteeFixture,
}

fn temp_dir() -> std::path::PathBuf {
    tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

async fn write_new_checkpoints_to_store(
    test_state: &TestState,
    store: SharedInMemoryStore,
    num_checkpoints: usize,
    prev_checkpoint: Option<VerifiedCheckpoint>,
) -> Result<Option<VerifiedCheckpoint>> {
    let (ordered_checkpoints, _contents, _sequence_number_to_digest, _checkpoints) = test_state
        .committee
        .make_empty_checkpoints(num_checkpoints, prev_checkpoint.clone());
    if prev_checkpoint.is_none() {
        store.inner_mut().insert_genesis_state(
            ordered_checkpoints.first().cloned().unwrap(),
            empty_contents(),
            test_state.committee.committee().to_owned(),
        );
    }
    for checkpoint in ordered_checkpoints.iter() {
        store.inner_mut().insert_checkpoint(checkpoint);
    }
    Ok(ordered_checkpoints.last().cloned())
}

async fn setup_test_state(temp_dir: PathBuf) -> anyhow::Result<TestState> {
    let local_path = temp_dir.join("local_dir");
    let remote_path = temp_dir.join("remote_dir");
    let local_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(local_path.clone()),
        ..Default::default()
    };
    let remote_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(remote_path.clone()),
        ..Default::default()
    };
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let archive_writer = ArchiveWriter::new(
        local_store_config.clone(),
        remote_store_config.clone(),
        FileCompression::Zstd,
        StorageFormat::Blob,
        Duration::from_secs(10),
        20,
        &Registry::default(),
    )
    .await?;
    let archive_reader_config = ArchiveReaderConfig {
        remote_store_config: remote_store_config.clone(),
        download_concurrency: NonZeroUsize::new(2).unwrap(),
        use_for_pruning_watermark: false,
    };
    let metrics = ArchiveReaderMetrics::new(&Registry::default());
    let archive_reader = ArchiveReader::new(archive_reader_config, &metrics)?;
    let local_store = local_store_config.make()?;
    let remote_store = remote_store_config.make()?;
    Ok(TestState {
        archive_writer,
        archive_reader,
        local_path,
        remote_path,
        local_store,
        remote_store,
        local_store_config,
        remote_store_config,
        committee,
    })
}

async fn insert_checkpoints_and_verify_manifest(
    test_state: &TestState,
    test_store: SharedInMemoryStore,
    prev_checkpoint: Option<VerifiedCheckpoint>,
) -> Result<Option<VerifiedCheckpoint>> {
    let mut prev_tail = None;
    let mut prev_checkpoint = prev_checkpoint;
    let mut num_verified_iterations = 0;
    loop {
        if test_state.remote_path.join("MANIFEST").exists() {
            if let Ok(manifest) = read_manifest(test_state.remote_store.clone()).await {
                for file in manifest.files().into_iter() {
                    let file_path =
                        path_to_filesystem(test_state.remote_path.clone(), &file.file_path())?;
                    assert!(file_path.exists());
                }

                if let Some(prev_tail) = prev_tail {
                    // Ensure checkpoint sequence number in manifest never moves back
                    assert!(manifest.next_checkpoint_seq_num() >= prev_tail);
                    if manifest.next_checkpoint_seq_num() > prev_tail {
                        num_verified_iterations += 1;
                    }
                }
                prev_tail = Some(manifest.next_checkpoint_seq_num());
                // Break out of the loop once we have ensured that we noticed MANIFEST
                // got updated at least 5 times
                if num_verified_iterations > 5 {
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        prev_checkpoint =
            write_new_checkpoints_to_store(test_state, test_store.clone(), 1, prev_checkpoint)
                .await?;
    }
    Ok(prev_checkpoint)
}

#[tokio::test]
async fn test_archive_basic() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let test_state = setup_test_state(temp_dir()).await?;
    let kill = test_state.archive_writer.start(test_store.clone()).await?;
    insert_checkpoints_and_verify_manifest(&test_state, test_store, None).await?;
    kill.send(())?;
    Ok(())
}

#[tokio::test]
async fn test_archive_resumes() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let test_state = setup_test_state(temp_dir()).await?;
    let kill = test_state.archive_writer.start(test_store.clone()).await?;
    let prev_checkpoint =
        insert_checkpoints_and_verify_manifest(&test_state, test_store.clone(), None).await?;

    // Kill the archive writer so we can restart it again
    drop(kill);
    let test_state = setup_test_state(temp_dir()).await?;
    let kill = test_state.archive_writer.start(test_store.clone()).await?;
    insert_checkpoints_and_verify_manifest(&test_state, test_store, prev_checkpoint).await?;
    kill.send(())?;
    Ok(())
}

#[tokio::test]
async fn test_manifest_serde() -> Result<()> {
    let original_manifest = Manifest::new(0, 100);
    let remote_store = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(temp_dir()),
        ..Default::default()
    }
    .make()?;
    write_manifest(original_manifest.clone(), remote_store.clone()).await?;
    let downloaded_manifest = read_manifest(remote_store).await?;
    assert_eq!(downloaded_manifest, original_manifest);
    Ok(())
}

#[tokio::test]
async fn test_archive_reader_e2e() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let test_state = setup_test_state(temp_dir()).await?;
    let kill = test_state.archive_writer.start(test_store.clone()).await?;
    let mut latest_archived_checkpoint_seq_num = 0;
    while latest_archived_checkpoint_seq_num < 10 {
        insert_checkpoints_and_verify_manifest(&test_state, test_store.clone(), None).await?;
        let new_latest_archived_checkpoint_seq_num = test_state
            .archive_reader
            .latest_available_checkpoint()
            .await?;
        ma::assert_ge!(
            new_latest_archived_checkpoint_seq_num,
            latest_archived_checkpoint_seq_num
        );
        latest_archived_checkpoint_seq_num = new_latest_archived_checkpoint_seq_num;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    ma::assert_ge!(latest_archived_checkpoint_seq_num, 10);
    let genesis_checkpoint = test_store
        .get_checkpoint_by_sequence_number(0)
        .context("Missing genesis checkpoint")?;
    let genesis_checkpoint_content = test_store
        .get_full_checkpoint_contents_by_sequence_number(0)
        .context("Missing genesis checkpoint")?;
    let read_store = SharedInMemoryStore::default();
    read_store.inner_mut().insert_genesis_state(
        genesis_checkpoint,
        VerifiedCheckpointContents::new_unchecked(genesis_checkpoint_content),
        test_state.committee.committee().to_owned(),
    );
    let tx_counter = Arc::new(AtomicU64::new(0));
    let checkpoint_counter = Arc::new(AtomicU64::new(0));
    test_state.archive_reader.sync_manifest_once().await?;
    test_state
        .archive_reader
        .read(
            read_store.clone(),
            0..(latest_archived_checkpoint_seq_num + 1),
            tx_counter,
            checkpoint_counter,
            true,
        )
        .await?;
    ma::assert_ge!(
        read_store
            .get_highest_verified_checkpoint()?
            .sequence_number,
        latest_archived_checkpoint_seq_num
    );
    ma::assert_ge!(
        read_store.get_highest_synced_checkpoint()?.sequence_number,
        latest_archived_checkpoint_seq_num
    );
    kill.send(())?;
    Ok(())
}

#[tokio::test]
async fn test_verify_archive_with_oneshot_store() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let test_state = setup_test_state(temp_dir()).await?;
    let kill = test_state.archive_writer.start(test_store.clone()).await?;
    let mut latest_archived_checkpoint_seq_num = 0;
    while latest_archived_checkpoint_seq_num < 10 {
        insert_checkpoints_and_verify_manifest(&test_state, test_store.clone(), None).await?;
        let new_latest_archived_checkpoint_seq_num = test_state
            .archive_reader
            .latest_available_checkpoint()
            .await?;
        ma::assert_ge!(
            new_latest_archived_checkpoint_seq_num,
            latest_archived_checkpoint_seq_num
        );
        latest_archived_checkpoint_seq_num = new_latest_archived_checkpoint_seq_num;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    ma::assert_ge!(latest_archived_checkpoint_seq_num, 10);
    let genesis_checkpoint = test_store
        .get_checkpoint_by_sequence_number(0)
        .context("Missing genesis checkpoint")?;
    let genesis_checkpoint_content = test_store
        .get_full_checkpoint_contents_by_sequence_number(0)
        .context("Missing genesis checkpoint")?;
    let mut read_store = SingleCheckpointSharedInMemoryStore::default();
    read_store.insert_genesis_state(
        genesis_checkpoint,
        VerifiedCheckpointContents::new_unchecked(genesis_checkpoint_content),
        test_state.committee.committee().to_owned(),
    );

    // Verification should pass
    assert!(verify_archive_with_local_store(
        read_store,
        test_state.remote_store_config.clone(),
        1,
        false
    )
    .await
    .is_ok());
    kill.send(())?;
    Ok(())
}

#[tokio::test]
async fn test_verify_archive_with_oneshot_store_bad_data() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let test_state = setup_test_state(temp_dir()).await?;
    let kill = test_state.archive_writer.start(test_store.clone()).await?;
    let mut latest_archived_checkpoint_seq_num = 0;
    while latest_archived_checkpoint_seq_num < 10 {
        insert_checkpoints_and_verify_manifest(&test_state, test_store.clone(), None).await?;
        let new_latest_archived_checkpoint_seq_num = test_state
            .archive_reader
            .latest_available_checkpoint()
            .await?;
        ma::assert_ge!(
            new_latest_archived_checkpoint_seq_num,
            latest_archived_checkpoint_seq_num
        );
        latest_archived_checkpoint_seq_num = new_latest_archived_checkpoint_seq_num;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    ma::assert_ge!(latest_archived_checkpoint_seq_num, 10);

    // Corrupt the .chk and .sum files in the archive
    let dir = fs::read_dir(test_state.remote_path)?;
    let mut num_files_corrupted = 0;
    for file in dir {
        let file = file?;
        let file_metadata = file.metadata()?;
        if file_metadata.is_dir() {
            // epoch dir
            let epoch_dir = fs::read_dir(file.path())?;
            for epoch_file in epoch_dir {
                let epoch_file = epoch_file?;
                // epoch dir should only have checkpoint files and no dir
                assert!(epoch_file.metadata()?.is_file());
                if epoch_file
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow!("Failed to read file name"))?
                    .ends_with(".chk")
                {
                    let mut f = File::options().write(true).open(epoch_file.path())?;
                    f.write_all("hello_world".as_bytes())?;
                    num_files_corrupted += 1;
                }
            }
        }
    }
    ma::assert_gt!(num_files_corrupted, 0);
    let genesis_checkpoint = test_store
        .get_checkpoint_by_sequence_number(0)
        .context("Missing genesis checkpoint")?;
    let genesis_checkpoint_content = test_store
        .get_full_checkpoint_contents_by_sequence_number(0)
        .context("Missing genesis checkpoint")?;
    let mut read_store = SingleCheckpointSharedInMemoryStore::default();
    read_store.insert_genesis_state(
        genesis_checkpoint,
        VerifiedCheckpointContents::new_unchecked(genesis_checkpoint_content),
        test_state.committee.committee().to_owned(),
    );

    // Verification should fail
    assert!(verify_archive_with_local_store(
        read_store,
        test_state.remote_store_config.clone(),
        1,
        false
    )
    .await
    .is_err());
    kill.send(())?;

    Ok(())
}
