// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::reader::ArchiveReaderV1;
use crate::writer::ArchiveWriterV1;
use crate::{read_manifest, FileCompression, EPOCH_DIR_PREFIX};
use anyhow::Context;
use object_store::path::Path;
use object_store::DynObjectStore;
use prometheus::Registry;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_macros::sim_test;
use sui_network::state_sync::test_utils::{empty_contents, CommitteeFixture};
use sui_storage::object_store::util::path_to_filesystem;
use sui_storage::object_store::{ObjectStoreConfig, ObjectStoreType};
use sui_types::messages_checkpoint::{VerifiedCheckpoint, VerifiedCheckpointContents};
use sui_types::storage::{ReadStore, SharedInMemoryStore};
use tempfile::tempdir;

struct TestState {
    archive_writer: ArchiveWriterV1,
    archive_reader: ArchiveReaderV1,
    local_path: PathBuf,
    remote_path: PathBuf,
    local_store: Arc<DynObjectStore>,
    remote_store: Arc<DynObjectStore>,
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
) -> anyhow::Result<Option<VerifiedCheckpoint>> {
    let (ordered_checkpoints, _sequence_number_to_digest, _checkpoints) = test_state
        .committee
        .make_checkpoints(num_checkpoints, prev_checkpoint.clone());
    if prev_checkpoint.is_none() {
        store.inner_mut().insert_genesis_state(
            ordered_checkpoints.first().cloned().unwrap(),
            empty_contents(),
            test_state.committee.committee().to_owned(),
        );
    }
    for checkpoint in ordered_checkpoints.iter() {
        store.inner_mut().insert_checkpoint(checkpoint.clone());
    }
    Ok(ordered_checkpoints.last().cloned())
}

async fn setup_checkpoint_writer(temp_dir: PathBuf) -> anyhow::Result<TestState> {
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
    let archive_writer = ArchiveWriterV1::new(
        local_store_config.clone(),
        remote_store_config.clone(),
        FileCompression::Zstd,
        Duration::from_secs(10),
        20,
        &Registry::default(),
    )
    .await?;

    let archive_reader =
        ArchiveReaderV1::new(remote_store_config.clone(), NonZeroUsize::new(2).unwrap())?;
    let local_store = local_store_config.make()?;
    let remote_store = remote_store_config.make()?;
    Ok(TestState {
        archive_writer,
        archive_reader,
        local_path,
        remote_path,
        local_store,
        remote_store,
        committee,
    })
}

async fn insert_checkpoints_and_verify_manifest(
    test_state: &TestState,
    test_store: SharedInMemoryStore,
    prev_checkpoint: Option<VerifiedCheckpoint>,
) -> anyhow::Result<Option<VerifiedCheckpoint>> {
    let mut prev_tail = None;
    let mut prev_checkpoint = prev_checkpoint;
    let mut num_verified_iterations = 0;
    loop {
        if test_state.remote_path.join("MANIFEST").exists() {
            if let Ok(manifest) = read_manifest(test_state.remote_store.clone()).await {
                for file in manifest.files().into_iter() {
                    let dir_prefix = Path::from(format!("{}{}", EPOCH_DIR_PREFIX, file.epoch_num));
                    let file_path = path_to_filesystem(
                        test_state.remote_path.clone(),
                        &file.file_path(&dir_prefix),
                    )?;
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

#[sim_test]
async fn test_archive_basic() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let test_state = setup_checkpoint_writer(temp_dir()).await?;
    let _kill = test_state.archive_writer.start(test_store.clone())?;
    insert_checkpoints_and_verify_manifest(&test_state, test_store, None).await?;
    Ok(())
}

#[sim_test]
async fn test_archive_resumes() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let test_state = setup_checkpoint_writer(temp_dir()).await?;
    let kill = test_state.archive_writer.start(test_store.clone())?;
    let prev_checkpoint =
        insert_checkpoints_and_verify_manifest(&test_state, test_store.clone(), None).await?;

    // Kill the archive writer so we can restart it again
    drop(kill);
    let test_state = setup_checkpoint_writer(temp_dir()).await?;
    let _kill = test_state.archive_writer.start(test_store.clone())?;
    insert_checkpoints_and_verify_manifest(&test_state, test_store, prev_checkpoint).await?;

    Ok(())
}

#[sim_test]
async fn test_archive_reader_basic() -> Result<(), anyhow::Error> {
    let test_store = SharedInMemoryStore::default();
    let mut test_state = setup_checkpoint_writer(temp_dir()).await?;
    let _kill = test_state.archive_writer.start(test_store.clone())?;
    let mut latest_archived_checkpoint_seq_num = 0;
    while latest_archived_checkpoint_seq_num < 10 {
        insert_checkpoints_and_verify_manifest(&test_state, test_store.clone(), None).await?;
        let new_latest_archived_checkpoint_seq_num = test_state
            .archive_reader
            .latest_available_checkpoint()
            .await?;
        assert!(new_latest_archived_checkpoint_seq_num >= latest_archived_checkpoint_seq_num);
        latest_archived_checkpoint_seq_num = new_latest_archived_checkpoint_seq_num;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(latest_archived_checkpoint_seq_num >= 10);
    let genesis_checkpoint = test_store
        .get_checkpoint_by_sequence_number(0)?
        .context("Missing genesis checkpoint")?;
    let genesis_checkpoint_content = test_store
        .get_full_checkpoint_contents_by_sequence_number(0)?
        .context("Missing genesis checkpoint")?;
    let read_store = SharedInMemoryStore::default();
    read_store.inner_mut().insert_genesis_state(
        genesis_checkpoint,
        VerifiedCheckpointContents::new_unchecked(genesis_checkpoint_content),
        test_state.committee.committee().to_owned(),
    );
    test_state
        .archive_reader
        .read(
            read_store.clone(),
            0..(latest_archived_checkpoint_seq_num + 1),
        )
        .await?;
    assert!(
        read_store.get_highest_synced_checkpoint()?.sequence_number
            >= latest_archived_checkpoint_seq_num
    );
    Ok(())
}

#[tokio::test]
async fn test_read_manifest() -> Result<(), anyhow::Error> {
    let local_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let manifest = read_manifest(local_store_config.make()?).await?;
    assert_eq!(manifest.epoch_num(), 18);
    Ok(())
}
