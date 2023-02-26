// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::snapshot::StateSnapshotReader;
use crate::snapshot::StateSnapshotReader::StateSnapshotReaderV1;
use crate::StateSnapshotWriterV1;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, TransactionDigest};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tempfile::tempdir;
use typed_store::rocks::DBMap;
use typed_store::Map;

fn temp_dir() -> std::path::PathBuf {
    tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

fn insert_keys(
    objects: &DBMap<ObjectKey, Object>,
    parent_sync: &DBMap<ObjectRef, TransactionDigest>,
    total_unique_object_ids: u64,
) -> Result<(), anyhow::Error> {
    let num_versions_per_object = 10;
    let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
    for id in ids {
        for i in (0..num_versions_per_object).rev() {
            let object = Object::immutable_with_id_version_for_testing(id, SequenceNumber::from(i));
            let object_ref = (id, SequenceNumber::from(i), object.digest());
            objects.insert(&ObjectKey(object_ref.0, object_ref.1), &object)?;
            parent_sync.insert(&object_ref, &TransactionDigest::random())?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_snapshot_basic() -> Result<(), anyhow::Error> {
    let db_path = temp_dir();
    let snapshot_path = temp_dir().join("snapshot");
    let epoch = 10;
    let end_of_epoch_checkpoint_seq_number = 1_000_000;
    let perpetual_db = AuthorityPerpetualTables::open(&db_path, None);
    insert_keys(&perpetual_db.objects, &perpetual_db.parent_sync, 1000)?;
    perpetual_db.set_recovery_epoch(epoch)?;
    let iter = perpetual_db.iter_live_object_set();
    let snapshot_writer = StateSnapshotWriterV1::new(&snapshot_path)?;
    snapshot_writer.write_objects(iter, &perpetual_db, end_of_epoch_checkpoint_seq_number)?;
    let snapshot_reader = StateSnapshotReader::new(&snapshot_path)?;
    match snapshot_reader {
        StateSnapshotReaderV1(mut reader) => {
            let buckets = reader.buckets()?;
            let mut object_refs_in_snapshot = HashSet::new();
            for bucket in buckets.iter() {
                let safe_object_iter = reader.safe_obj_iter(*bucket)?;
                for (object, _) in safe_object_iter {
                    object_refs_in_snapshot.insert(object.compute_object_reference());
                }
            }
            let object_refs_in_live_set: HashSet<ObjectRef> =
                perpetual_db.iter_live_object_set().collect();
            assert_eq!(object_refs_in_snapshot, object_refs_in_live_set);
            assert_eq!(reader.epoch(), epoch);
            assert_eq!(reader.checkpoint_seq_number(), end_of_epoch_checkpoint_seq_number);
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_snapshot_empty_db() -> Result<(), anyhow::Error> {
    let db_path = temp_dir();
    let snapshot_path = temp_dir().join("snapshot");
    let epoch = 10;
    let end_of_epoch_checkpoint_seq_number = 1_000_000;
    let perpetual_db = AuthorityPerpetualTables::open(&db_path, None);
    perpetual_db.set_recovery_epoch(epoch)?;
    let iter = perpetual_db.iter_live_object_set();
    let snapshot_writer = StateSnapshotWriterV1::new(&snapshot_path)?;
    snapshot_writer.write_objects(iter, &perpetual_db, end_of_epoch_checkpoint_seq_number)?;
    let snapshot_reader = StateSnapshotReader::new(&snapshot_path)?;
    match snapshot_reader {
        StateSnapshotReaderV1(mut reader) => {
            let buckets = reader.buckets()?;
            let mut object_refs_in_snapshot = HashSet::new();
            for bucket in buckets.iter() {
                let safe_object_iter = reader.safe_obj_iter(*bucket)?;
                for (object, _) in safe_object_iter {
                    object_refs_in_snapshot.insert(object.compute_object_reference());
                }
            }
            let object_refs_in_live_set: HashSet<ObjectRef> =
                perpetual_db.iter_live_object_set().collect();
            assert_eq!(object_refs_in_snapshot, object_refs_in_live_set);
            assert_eq!(reader.epoch(), epoch);
            assert_eq!(reader.checkpoint_seq_number(), end_of_epoch_checkpoint_seq_number);
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_snapshot_multiple_buckets() -> Result<(), anyhow::Error> {
    let db_path = temp_dir();
    let snapshot_path = temp_dir().join("snapshot");
    let epoch = 10;
    let end_of_epoch_checkpoint_seq_number = 1_000_000;
    let perpetual_db = AuthorityPerpetualTables::open(&db_path, None);
    perpetual_db.set_recovery_epoch(epoch)?;
    insert_keys(&perpetual_db.objects, &perpetual_db.parent_sync, 1_000)?;
    let iter = perpetual_db.iter_live_object_set();
    let mut snapshot_writer = StateSnapshotWriterV1::new(&snapshot_path)?;
    snapshot_writer.write_object_with_bucket_func(iter, &perpetual_db, |object_ref| {
        let mut hasher = DefaultHasher::new();
        object_ref.2.base58_encode().hash(&mut hasher);
        (hasher.finish() % 1024) as u32
    })?;
    snapshot_writer.finalize(epoch, end_of_epoch_checkpoint_seq_number)?;
    let snapshot_reader = StateSnapshotReader::new(&snapshot_path)?;
    match snapshot_reader {
        StateSnapshotReaderV1(mut reader) => {
            let buckets = reader.buckets()?;
            let mut object_refs_in_snapshot = HashSet::new();
            for bucket in buckets.iter() {
                let safe_object_iter = reader.safe_obj_iter(*bucket)?;
                for (object, _) in safe_object_iter {
                    object_refs_in_snapshot.insert(object.compute_object_reference());
                }
            }
            let object_refs_in_live_set: HashSet<ObjectRef> =
                perpetual_db.iter_live_object_set().collect();
            assert_eq!(object_refs_in_snapshot, object_refs_in_live_set);
            assert_eq!(reader.epoch(), epoch);
            assert_eq!(reader.checkpoint_seq_number(), end_of_epoch_checkpoint_seq_number);
        }
    }
    Ok(())
}
