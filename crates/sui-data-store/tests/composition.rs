// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Result, anyhow};
use sui_data_store::{
    CheckpointStore, CheckpointStoreWriter, EpochData, Node, ObjectKey, ObjectStore,
    ObjectStoreWriter, ReadDataStore, SetupStore, TransactionInfo, TransactionStore,
    TransactionStoreWriter, VersionQuery,
    stores::{
        CHECKPOINT_DIR, CHECKPOINT_LATEST_FILE, CompositeStore, DataStore, FileSystemStore,
        InMemoryStore, ReadThroughStore, WriteThroughStore,
    },
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    object::{Object, Owner},
    test_checkpoint_data_builder::TestCheckpointBuilder,
};

const CHAIN_ID: &str = "test_chain";

fn make_store() -> Result<(tempfile::TempDir, FileSystemStore)> {
    let tempdir = tempfile::tempdir()?;
    let store = FileSystemStore::new_with_path(Node::Testnet, tempdir.path().to_path_buf())?;
    store.setup(Some(CHAIN_ID.to_string()))?;
    Ok((tempdir, store))
}

fn sample_checkpoint(sequence: u64) -> sui_data_store::FullCheckpointData {
    TestCheckpointBuilder::new(sequence)
        .start_transaction(1)
        .create_owned_object(42)
        .finish_transaction()
        .build_checkpoint()
}

fn transaction_info(checkpoint: &sui_data_store::FullCheckpointData) -> TransactionInfo {
    let executed = &checkpoint.transactions[0];
    TransactionInfo {
        data: executed.transaction.clone(),
        effects: executed.effects.clone(),
        checkpoint: checkpoint.summary.sequence_number,
    }
}

fn sample_object(object_id: ObjectID, owner: SuiAddress, version: u64) -> Object {
    Object::with_id_owner_version_for_testing(
        object_id,
        SequenceNumber::from_u64(version),
        Owner::AddressOwner(owner),
    )
}

#[derive(Default)]
struct TxStoreStub {
    transactions: RwLock<BTreeMap<String, TransactionInfo>>,
    read_calls: AtomicUsize,
    write_calls: AtomicUsize,
    fail_writes: bool,
}

impl TxStoreStub {
    fn with_failure() -> Self {
        Self {
            fail_writes: true,
            ..Self::default()
        }
    }
}

impl TransactionStore for TxStoreStub {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, anyhow::Error> {
        self.read_calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.transactions.read().unwrap().get(tx_digest).cloned())
    }
}

impl TransactionStoreWriter for TxStoreStub {
    fn write_transaction(
        &self,
        tx_digest: &str,
        transaction_info: TransactionInfo,
    ) -> Result<(), anyhow::Error> {
        self.write_calls.fetch_add(1, Ordering::Relaxed);
        if self.fail_writes {
            return Err(anyhow!("stub write failure"));
        }
        self.transactions
            .write()
            .unwrap()
            .insert(tx_digest.to_string(), transaction_info);
        Ok(())
    }
}

#[derive(Default)]
struct ObjectSourceStub {
    objects: RwLock<BTreeMap<ObjectKey, (Object, u64)>>,
    read_calls: AtomicUsize,
}

impl ObjectStore for ObjectSourceStub {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        self.read_calls.fetch_add(1, Ordering::Relaxed);
        let objects = self.objects.read().unwrap();
        Ok(keys.iter().map(|key| objects.get(key).cloned()).collect())
    }
}

#[test]
fn filesystem_store_round_trips_generic_traits_and_helpers() -> Result<()> {
    let (tempdir, store) = make_store()?;
    let checkpoint = sample_checkpoint(7);
    let tx_info = transaction_info(&checkpoint);

    TransactionStoreWriter::write_transaction(&store, "tx-1", tx_info.clone())?;
    let stored_tx = TransactionStore::transaction_data_and_effects(&store, "tx-1")?
        .expect("transaction should be present");
    assert_eq!(stored_tx.checkpoint, tx_info.checkpoint);
    assert_eq!(
        bcs::to_bytes(&stored_tx.data)?,
        bcs::to_bytes(&tx_info.data)?
    );
    assert_eq!(
        bcs::to_bytes(&stored_tx.effects)?,
        bcs::to_bytes(&tx_info.effects)?
    );

    let epoch = EpochData {
        epoch_id: 7,
        protocol_version: 1,
        rgp: 1_000,
        start_timestamp: 123_456,
    };
    sui_data_store::EpochStoreWriter::write_epoch_info(&store, epoch.epoch_id, epoch.clone())?;
    let stored_epoch =
        sui_data_store::EpochStore::epoch_info(&store, epoch.epoch_id)?.expect("epoch present");
    assert_eq!(stored_epoch.epoch_id, epoch.epoch_id);
    assert!(
        sui_data_store::EpochStore::protocol_config(&store, epoch.epoch_id)?.is_some(),
        "protocol config should derive from stored epoch data"
    );

    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = sample_object(object_id, owner, 2);
    let exact_key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(2),
    };
    let root_key = ObjectKey {
        object_id,
        version_query: VersionQuery::RootVersion(10),
    };
    let checkpoint_key = ObjectKey {
        object_id,
        version_query: VersionQuery::AtCheckpoint(100),
    };

    ObjectStoreWriter::write_object(&store, &exact_key, object.clone(), 2)?;
    ObjectStoreWriter::write_object(&store, &root_key, object.clone(), 2)?;
    ObjectStoreWriter::write_object(&store, &checkpoint_key, object.clone(), 2)?;

    let objects = ObjectStore::get_objects(&store, &[exact_key.clone(), root_key, checkpoint_key])?;
    assert!(objects.iter().all(|entry| entry.is_some()));
    assert!(store.get_object_latest(&object_id)?.is_some());
    assert!(store.get_object_at_version(&object_id, 2)?.is_some());
    assert!(store.get_object_at_root_version(&object_id, 10)?.is_some());
    assert!(store.get_object_at_checkpoint(&object_id, 100)?.is_some());
    assert_eq!(store.get_objects_by_owner(owner)?.len(), 1);

    CheckpointStoreWriter::write_checkpoint(&store, &checkpoint)?;
    let by_sequence = CheckpointStore::get_checkpoint_by_sequence_number(
        &store,
        checkpoint.summary.sequence_number,
    )?
    .expect("checkpoint should be present");
    assert_eq!(
        by_sequence.summary.sequence_number,
        checkpoint.summary.sequence_number
    );
    assert_eq!(
        CheckpointStore::get_sequence_by_checkpoint_digest(&store, checkpoint.summary.digest())?,
        Some(checkpoint.summary.sequence_number)
    );
    assert_eq!(
        CheckpointStore::get_sequence_by_contents_digest(&store, checkpoint.contents.digest())?,
        Some(checkpoint.summary.sequence_number)
    );

    let latest_marker = tempdir
        .path()
        .join(CHAIN_ID)
        .join(CHECKPOINT_DIR)
        .join(CHECKPOINT_LATEST_FILE);
    fs::remove_file(latest_marker)?;
    let latest = CheckpointStore::get_latest_checkpoint(&store)?.expect("latest should exist");
    assert_eq!(
        latest.summary.sequence_number,
        checkpoint.summary.sequence_number
    );

    Ok(())
}

#[test]
fn read_through_backfills_checkpoints_and_writes_only_primary() -> Result<()> {
    let primary = InMemoryStore::new(Node::Testnet);
    let secondary = InMemoryStore::new(Node::Testnet);
    let checkpoint = sample_checkpoint(7);
    let second_checkpoint = sample_checkpoint(8);

    CheckpointStoreWriter::write_checkpoint(&secondary, &checkpoint)?;

    let store = ReadThroughStore::new(&primary, &secondary);
    let loaded = CheckpointStore::get_checkpoint_by_sequence_number(
        &store,
        checkpoint.summary.sequence_number,
    )?
    .expect("checkpoint should load from secondary");
    assert_eq!(
        loaded.summary.sequence_number,
        checkpoint.summary.sequence_number
    );
    assert!(
        CheckpointStore::get_checkpoint_by_sequence_number(
            &primary,
            checkpoint.summary.sequence_number
        )?
        .is_some()
    );

    CheckpointStoreWriter::write_checkpoint(&store, &second_checkpoint)?;
    assert!(
        CheckpointStore::get_checkpoint_by_sequence_number(
            &primary,
            second_checkpoint.summary.sequence_number
        )?
        .is_some()
    );
    assert!(
        CheckpointStore::get_checkpoint_by_sequence_number(
            &secondary,
            second_checkpoint.summary.sequence_number
        )?
        .is_none()
    );

    Ok(())
}

#[test]
fn write_through_reads_and_writes_across_both_layers() -> Result<()> {
    let primary = TxStoreStub::default();
    let secondary = TxStoreStub::default();
    let checkpoint = sample_checkpoint(7);
    let tx_info = transaction_info(&checkpoint);

    TransactionStoreWriter::write_transaction(&secondary, "tx-1", tx_info.clone())?;

    let store = WriteThroughStore::new(&primary, &secondary);
    let loaded = TransactionStore::transaction_data_and_effects(&store, "tx-1")?
        .expect("transaction should load from secondary");
    assert_eq!(loaded.checkpoint, tx_info.checkpoint);
    assert!(TransactionStore::transaction_data_and_effects(&primary, "tx-1")?.is_some());

    let second_checkpoint = sample_checkpoint(8);
    let second_tx = transaction_info(&second_checkpoint);
    TransactionStoreWriter::write_transaction(&store, "tx-2", second_tx.clone())?;
    assert!(TransactionStore::transaction_data_and_effects(&primary, "tx-2")?.is_some());
    assert!(TransactionStore::transaction_data_and_effects(&secondary, "tx-2")?.is_some());

    Ok(())
}

#[test]
fn write_through_secondary_failure_prevents_primary_write() {
    let primary = TxStoreStub::default();
    let secondary = TxStoreStub::with_failure();
    let checkpoint = sample_checkpoint(7);
    let tx_info = transaction_info(&checkpoint);

    let store = WriteThroughStore::new(&primary, &secondary);
    let err = TransactionStoreWriter::write_transaction(&store, "tx-1", tx_info);
    assert!(err.is_err());
    assert_eq!(primary.write_calls.load(Ordering::Relaxed), 0);
    assert!(
        TransactionStore::transaction_data_and_effects(&primary, "tx-1")
            .expect("lookup should succeed")
            .is_none()
    );
}

#[test]
fn composite_store_routes_the_forking_topology() -> Result<()> {
    let (_tempdir, filesystem) = make_store()?;
    let memory = InMemoryStore::new(Node::Testnet);
    let object_source = ObjectSourceStub::default();

    let checkpoint = sample_checkpoint(7);
    let tx_info = transaction_info(&checkpoint);
    TransactionStoreWriter::write_transaction(&filesystem, "tx-1", tx_info)?;
    CheckpointStoreWriter::write_checkpoint(&filesystem, &checkpoint)?;

    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = sample_object(object_id, owner, 2);
    let object_key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(2),
    };
    object_source
        .objects
        .write()
        .unwrap()
        .insert(object_key.clone(), (object.clone(), 2));

    let hot_mem_fs = WriteThroughStore::new(&memory, &filesystem);
    let disk_then_source_objects = ReadThroughStore::new(&filesystem, &object_source);
    let hot_objects = WriteThroughStore::new(&memory, &disk_then_source_objects);
    let store = CompositeStore::new(&hot_mem_fs, &hot_mem_fs, &hot_objects, &hot_mem_fs);

    assert!(TransactionStore::transaction_data_and_effects(&store, "tx-1")?.is_some());
    assert!(
        CheckpointStore::get_checkpoint_by_sequence_number(
            &store,
            checkpoint.summary.sequence_number
        )?
        .is_some()
    );
    assert_eq!(object_source.read_calls.load(Ordering::Relaxed), 0);

    let loaded_objects = ObjectStore::get_objects(&store, std::slice::from_ref(&object_key))?;
    assert_eq!(object_source.read_calls.load(Ordering::Relaxed), 1);
    let (loaded_object, actual_version) = loaded_objects[0].clone().expect("object should load");
    assert_eq!(actual_version, 2);
    assert_eq!(bcs::to_bytes(&loaded_object)?, bcs::to_bytes(&object)?);

    assert!(filesystem.get_object_at_version(&object_id, 2)?.is_some());
    assert!(ObjectStore::get_objects(&memory, &[object_key])?[0].is_some());

    Ok(())
}

#[test]
fn arc_forwarding_impls_compile_for_common_store_shapes() {
    fn assert_tx_store<T: TransactionStore>() {}
    fn assert_obj_store<T: ObjectStore>() {}
    fn assert_checkpoint_store<T: CheckpointStore>() {}
    fn assert_read_data_store<T: ReadDataStore>() {}

    assert_tx_store::<Arc<FileSystemStore>>();
    assert_obj_store::<Arc<FileSystemStore>>();
    assert_checkpoint_store::<Arc<FileSystemStore>>();
    assert_read_data_store::<Arc<ReadThroughStore<InMemoryStore, FileSystemStore>>>();
    assert_checkpoint_store::<Arc<ReadThroughStore<InMemoryStore, FileSystemStore>>>();
    assert_checkpoint_store::<Arc<WriteThroughStore<InMemoryStore, FileSystemStore>>>();
    assert_obj_store::<
        Arc<WriteThroughStore<InMemoryStore, ReadThroughStore<FileSystemStore, DataStore>>>,
    >();
}
