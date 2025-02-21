// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::reader::StateSnapshotReaderV1;
use crate::writer::StateSnapshotWriterV1;
use crate::FileCompression;
use fastcrypto::hash::MultisetHash;
use futures::future::AbortHandle;
use indicatif::MultiProgress;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::state_accumulator::StateAccumulator;
use sui_protocol_config::ProtocolConfig;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::ECMHLiveObjectSetDigest;
use sui_types::object::Object;
use tempfile::tempdir;

fn temp_dir() -> std::path::PathBuf {
    tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

pub fn insert_keys(
    db: &AuthorityPerpetualTables,
    total_unique_object_ids: u64,
) -> Result<(), anyhow::Error> {
    let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
    for id in ids {
        let object = Object::immutable_with_id_for_testing(id);
        db.insert_object_test_only(object)?;
    }
    Ok(())
}

fn compare_live_objects(
    db1: &AuthorityPerpetualTables,
    db2: &AuthorityPerpetualTables,
    include_wrapped_tombstone: bool,
) -> Result<(), anyhow::Error> {
    let mut object_set_1 = HashSet::new();
    let mut object_set_2 = HashSet::new();
    for live_object in db1.iter_live_object_set(include_wrapped_tombstone) {
        object_set_1.insert(live_object.object_reference());
    }
    for live_object in db2.iter_live_object_set(include_wrapped_tombstone) {
        object_set_2.insert(live_object.object_reference());
    }
    assert_eq!(object_set_1, object_set_2);
    Ok(())
}

fn accumulate_live_object_set(
    perpetual_db: &AuthorityPerpetualTables,
    include_wrapped_tombstone: bool,
) -> Accumulator {
    let mut acc = Accumulator::default();
    perpetual_db
        .iter_live_object_set(include_wrapped_tombstone)
        .for_each(|live_object| {
            StateAccumulator::accumulate_live_object(&mut acc, &live_object);
        });
    acc
}

#[tokio::test]
async fn test_snapshot_basic() -> Result<(), anyhow::Error> {
    let db_path = temp_dir();
    let restored_db_path = temp_dir();
    let local = temp_dir().join("local_dir");
    let remote = temp_dir().join("remote_dir");
    let restored_local = temp_dir().join("local_dir_restore");
    let local_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(local),
        ..Default::default()
    };
    let remote_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(remote),
        ..Default::default()
    };

    let snapshot_writer = StateSnapshotWriterV1::new(
        &local_store_config,
        &remote_store_config,
        FileCompression::Zstd,
        NonZeroUsize::new(1).unwrap(),
    )
    .await?;
    let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&db_path, None));
    insert_keys(&perpetual_db, 1000)?;
    let root_accumulator =
        ECMHLiveObjectSetDigest::from(accumulate_live_object_set(&perpetual_db, true).digest());
    snapshot_writer
        .write_internal(0, true, perpetual_db.clone(), root_accumulator)
        .await?;
    let local_store_restore_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(restored_local),
        ..Default::default()
    };
    let mut snapshot_reader = StateSnapshotReaderV1::new(
        0,
        &remote_store_config,
        &local_store_restore_config,
        NonZeroUsize::new(1).unwrap(),
        MultiProgress::new(),
        false, // skip_reset_local_store
    )
    .await?;
    let restored_perpetual_db = AuthorityPerpetualTables::open(&restored_db_path, None);
    let (_abort_handle, abort_registration) = AbortHandle::new_pair();
    snapshot_reader
        .read(&restored_perpetual_db, abort_registration, None)
        .await?;
    compare_live_objects(&perpetual_db, &restored_perpetual_db, true)?;
    Ok(())
}

#[tokio::test]
async fn test_snapshot_empty_db() -> Result<(), anyhow::Error> {
    let db_path = temp_dir();
    let restored_db_path = temp_dir();
    let local = temp_dir().join("local_dir");
    let remote = temp_dir().join("remote_dir");
    let restored_local = temp_dir().join("local_dir_restore");
    let local_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(local),
        ..Default::default()
    };
    let remote_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(remote),
        ..Default::default()
    };
    let include_wrapped_tombstone =
        !ProtocolConfig::get_for_max_version_UNSAFE().simplified_unwrap_then_delete();
    let snapshot_writer = StateSnapshotWriterV1::new(
        &local_store_config,
        &remote_store_config,
        FileCompression::Zstd,
        NonZeroUsize::new(1).unwrap(),
    )
    .await?;
    let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&db_path, None));
    let root_accumulator =
        ECMHLiveObjectSetDigest::from(accumulate_live_object_set(&perpetual_db, true).digest());
    snapshot_writer
        .write_internal(0, true, perpetual_db.clone(), root_accumulator)
        .await?;
    let local_store_restore_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(restored_local),
        ..Default::default()
    };
    let mut snapshot_reader = StateSnapshotReaderV1::new(
        0,
        &remote_store_config,
        &local_store_restore_config,
        NonZeroUsize::new(1).unwrap(),
        MultiProgress::new(),
        false, // skip_reset_local_store
    )
    .await?;
    let restored_perpetual_db = AuthorityPerpetualTables::open(&restored_db_path, None);
    let (_abort_handle, abort_registration) = AbortHandle::new_pair();
    snapshot_reader
        .read(&restored_perpetual_db, abort_registration, None)
        .await?;
    compare_live_objects(
        &perpetual_db,
        &restored_perpetual_db,
        include_wrapped_tombstone,
    )?;
    Ok(())
}
