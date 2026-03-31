// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Path, PathBuf},
};

use forking_data_store::{
    CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter, Node,
    SetupStore,
    stores::{
        CHECKPOINT_DIR, CHECKPOINT_FILE_EXTENSION, CHECKPOINT_LATEST_FILE, DATA_STORE_DIR,
        FileSystemStore, ForkingStore, InMemoryStore, ReadThroughStore, WriteThroughStore,
    },
};
use mockall::predicate::eq;
use sui_types::{
    digests::{
        ChainIdentifier, CheckpointDigest, MAINNET_CHAIN_IDENTIFIER_BASE58,
        TESTNET_CHAIN_IDENTIFIER_BASE58,
    },
    message_envelope::Message as _,
    test_checkpoint_data_builder::TestCheckpointBuilder,
};

type CheckpointData = sui_types::full_checkpoint_content::Checkpoint;

mockall::mock! {
    CheckpointDelegate {}

    impl CheckpointStore for CheckpointDelegate {
        fn get_checkpoint_by_sequence_number(
            &self,
            sequence: u64,
        ) -> anyhow::Result<Option<CheckpointData>>;

        fn get_latest_checkpoint(&self) -> anyhow::Result<Option<CheckpointData>>;

        fn get_sequence_by_checkpoint_digest(
            &self,
            digest: &sui_types::digests::CheckpointDigest,
        ) -> anyhow::Result<Option<u64>>;

        fn get_sequence_by_contents_digest(
            &self,
            digest: &sui_types::digests::CheckpointContentsDigest,
        ) -> anyhow::Result<Option<u64>>;
    }

    impl CheckpointStoreWriter for CheckpointDelegate {
        fn write_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()>;
    }
}

fn test_checkpoint(sequence: u64, epoch: u64) -> CheckpointData {
    TestCheckpointBuilder::new(sequence)
        .with_epoch(epoch)
        .build_checkpoint()
}

fn test_epoch_data(epoch: u64, protocol_version: u64) -> EpochData {
    EpochData {
        epoch_id: epoch,
        protocol_version,
        rgp: 1,
        start_timestamp: 0,
    }
}

fn checkpoint_path(store_root: &Path, chain_id: &str, sequence: u64) -> PathBuf {
    store_root
        .join(chain_id)
        .join(CHECKPOINT_DIR)
        .join(format!("{sequence}.{CHECKPOINT_FILE_EXTENSION}"))
}

fn fork_checkpoint_path(
    store_root: &Path,
    chain_id: &str,
    fork_origin_checkpoint: u64,
    sequence: u64,
) -> PathBuf {
    store_root
        .join(chain_id)
        .join(FileSystemStore::fork_directory_name(fork_origin_checkpoint))
        .join(CHECKPOINT_DIR)
        .join(format!("{sequence}.{CHECKPOINT_FILE_EXTENSION}"))
}

#[test]
fn filesystem_store_persists_checkpoint_files_indexes_and_latest_marker() {
    let tempdir = tempfile::tempdir().unwrap();
    let store_root = tempdir.path().join(DATA_STORE_DIR);
    let long_chain_id = MAINNET_CHAIN_IDENTIFIER_BASE58;
    let short_chain_id = "35834a8a";
    let checkpoint = test_checkpoint(7, 3);
    let epoch_data = test_epoch_data(3, 1);
    let checkpoint_digest = checkpoint.summary.data().digest();
    let contents_digest = *checkpoint.contents.digest();

    let store = FileSystemStore::new_with_path(Node::Mainnet, store_root.clone()).unwrap();
    assert_eq!(
        store.setup(Some(long_chain_id.to_string())).unwrap(),
        Some(short_chain_id.to_string())
    );
    store.write_epoch_info(3, epoch_data.clone()).unwrap();
    store.write_checkpoint(&checkpoint).unwrap();

    let checkpoint_file = checkpoint_path(&store_root, short_chain_id, 7);
    assert!(checkpoint_file.exists(), "checkpoint file should exist");
    assert!(
        store_root
            .join(short_chain_id)
            .join(CHECKPOINT_DIR)
            .join(CHECKPOINT_LATEST_FILE)
            .exists(),
        "latest marker should exist"
    );
    assert_eq!(
        fs::read_to_string(store_root.join("node_mapping.csv")).unwrap(),
        "mainnet,35834a8a\n"
    );

    let loaded = store
        .get_checkpoint_by_sequence_number(7)
        .unwrap()
        .expect("checkpoint should be readable");
    assert_eq!(loaded.summary.data().sequence_number, 7);
    assert_eq!(loaded.summary.data().digest(), checkpoint_digest);
    assert_eq!(*loaded.contents.digest(), contents_digest);
    assert_eq!(
        store
            .get_latest_checkpoint()
            .unwrap()
            .unwrap()
            .summary
            .data()
            .sequence_number,
        7
    );
    assert_eq!(
        store
            .get_sequence_by_checkpoint_digest(&checkpoint_digest)
            .unwrap(),
        Some(7)
    );
    assert_eq!(
        store
            .get_sequence_by_contents_digest(&contents_digest)
            .unwrap(),
        Some(7)
    );
    assert_eq!(store.epoch_info(3).unwrap().unwrap().protocol_version, 1);

    let reopened = FileSystemStore::new_with_path(Node::Mainnet, store_root.clone()).unwrap();
    assert_eq!(
        reopened.setup(None).unwrap(),
        Some(short_chain_id.to_string())
    );
    assert_eq!(
        reopened
            .get_checkpoint_by_sequence_number(7)
            .unwrap()
            .unwrap()
            .summary
            .data()
            .digest(),
        checkpoint_digest
    );
    assert_eq!(
        reopened
            .get_sequence_by_contents_digest(&contents_digest)
            .unwrap(),
        Some(7)
    );
    assert_eq!(reopened.epoch_info(3).unwrap().unwrap().protocol_version, 1);
}

#[test]
fn filesystem_store_normalizes_known_long_chain_identifiers() {
    let tempdir = tempfile::tempdir().unwrap();
    let store_root = tempdir.path().join(DATA_STORE_DIR);
    let store = FileSystemStore::new_with_path(Node::Testnet, store_root.clone()).unwrap();

    assert_eq!(
        store
            .setup(Some(TESTNET_CHAIN_IDENTIFIER_BASE58.to_string()))
            .unwrap(),
        Some("4c78adac".to_string())
    );
    assert_eq!(
        fs::read_to_string(store_root.join("node_mapping.csv")).unwrap(),
        "testnet,4c78adac\n"
    );
}

#[test]
fn filesystem_store_normalizes_synthetic_long_chain_identifiers() {
    let tempdir = tempfile::tempdir().unwrap();
    let store_root = tempdir.path().join(DATA_STORE_DIR);
    let synthetic_digest = CheckpointDigest::new([0xabu8; 32]);
    let expected_short = ChainIdentifier::from(synthetic_digest).to_string();
    let store = FileSystemStore::new_with_path(Node::Devnet, store_root.clone()).unwrap();

    assert_eq!(
        store.setup(Some(synthetic_digest.to_string())).unwrap(),
        Some(expected_short.clone())
    );
    assert_eq!(
        fs::read_to_string(store_root.join("node_mapping.csv")).unwrap(),
        format!("devnet,{expected_short}\n")
    );
}

#[test]
fn filesystem_store_scopes_fork_session_data_under_the_fork_directory() {
    let tempdir = tempfile::tempdir().unwrap();
    let store_root = tempdir.path().join(DATA_STORE_DIR);
    let checkpoint = test_checkpoint(9, 3);
    let epoch_data = test_epoch_data(3, 1);
    let store =
        FileSystemStore::new_with_path_for_fork(Node::Mainnet, store_root.clone(), 7).unwrap();

    assert_eq!(
        store
            .setup(Some(MAINNET_CHAIN_IDENTIFIER_BASE58.to_string()))
            .unwrap(),
        Some("35834a8a".to_string())
    );
    store.write_epoch_info(3, epoch_data).unwrap();
    store.write_checkpoint(&checkpoint).unwrap();

    assert!(fork_checkpoint_path(&store_root, "35834a8a", 7, 9).exists());
    assert!(!checkpoint_path(&store_root, "35834a8a", 9).exists());
}

#[test]
fn read_through_checkpoint_miss_populates_primary() {
    let checkpoint = test_checkpoint(7, 3);
    let epoch_data = test_epoch_data(3, 1);
    let primary = InMemoryStore::new(Node::Mainnet);
    let secondary = InMemoryStore::new(Node::Mainnet);
    secondary.write_checkpoint(&checkpoint).unwrap();
    secondary.write_epoch_info(3, epoch_data).unwrap();

    let store = ReadThroughStore::new(primary, secondary);
    let loaded = store
        .get_checkpoint_by_sequence_number(7)
        .unwrap()
        .expect("checkpoint should be returned from secondary");
    assert_eq!(loaded.summary.data().sequence_number, 7);
    assert!(
        store
            .primary()
            .get_checkpoint_by_sequence_number(7)
            .unwrap()
            .is_some()
    );
    assert!(store.primary().epoch_info(3).unwrap().is_none());
    assert_eq!(store.epoch_info(3).unwrap().unwrap().protocol_version, 1);
    assert_eq!(
        store
            .primary()
            .epoch_info(3)
            .unwrap()
            .unwrap()
            .protocol_version,
        1
    );
}

#[test]
fn write_through_checkpoint_write_hits_both_layers() {
    let checkpoint = test_checkpoint(11, 5);
    let primary = InMemoryStore::new(Node::Mainnet);
    let secondary = InMemoryStore::new(Node::Mainnet);
    let store = WriteThroughStore::new(primary, secondary);

    store.write_checkpoint(&checkpoint).unwrap();

    assert!(
        store
            .primary()
            .get_checkpoint_by_sequence_number(11)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .secondary()
            .get_checkpoint_by_sequence_number(11)
            .unwrap()
            .is_some()
    );
}

#[test]
fn forking_store_delegates_checkpoint_operations_to_checkpoint_store() {
    let checkpoint = test_checkpoint(9, 4);
    let checkpoint_digest = checkpoint.summary.data().digest();

    let mut checkpoint_store = MockCheckpointDelegate::new();
    checkpoint_store
        .expect_get_checkpoint_by_sequence_number()
        .with(eq(9))
        .times(1)
        .return_once({
            let checkpoint = checkpoint.clone();
            move |_| Ok(Some(checkpoint))
        });
    checkpoint_store
        .expect_get_sequence_by_checkpoint_digest()
        .with(eq(checkpoint_digest))
        .times(1)
        .return_once(move |_| Ok(Some(9)));
    checkpoint_store
        .expect_write_checkpoint()
        .times(1)
        .return_once(|_| Ok(()));
    checkpoint_store
        .expect_get_latest_checkpoint()
        .times(0)
        .returning(|| Ok(None));
    checkpoint_store
        .expect_get_sequence_by_contents_digest()
        .times(0)
        .returning(|_| Ok(None));

    let store = ForkingStore::new(
        InMemoryStore::new(Node::Mainnet),
        InMemoryStore::new(Node::Mainnet),
        InMemoryStore::new(Node::Mainnet),
        checkpoint_store,
    );

    assert!(
        store
            .get_checkpoint_by_sequence_number(9)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        store
            .get_sequence_by_checkpoint_digest(&checkpoint_digest)
            .unwrap(),
        Some(9)
    );
    store.write_checkpoint(&checkpoint).unwrap();
}

#[cfg(feature = "test-utils")]
#[test]
fn feature_gated_generated_checkpoint_mock_is_available_to_downstream_tests() {
    let checkpoint = test_checkpoint(13, 6);
    let mut store = forking_data_store::MockCheckpointStore::new();
    store
        .expect_get_checkpoint_by_sequence_number()
        .times(1)
        .return_once(move |sequence| {
            assert_eq!(sequence, 13);
            Ok(Some(checkpoint))
        });
    store
        .expect_get_latest_checkpoint()
        .times(0)
        .returning(|| Ok(None));
    store
        .expect_get_sequence_by_checkpoint_digest()
        .times(0)
        .returning(|_| Ok(None));
    store
        .expect_get_sequence_by_contents_digest()
        .times(0)
        .returning(|_| Ok(None));

    assert!(
        store
            .get_checkpoint_by_sequence_number(13)
            .unwrap()
            .is_some()
    );
}
