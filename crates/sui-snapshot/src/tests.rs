// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::writer::StateSnapshotWriterV1;
use crate::FileCompression;
use std::num::NonZeroUsize;
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_storage::object_store::{ObjectStoreConfig, ObjectStoreType};
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use tempfile::tempdir;

fn temp_dir() -> std::path::PathBuf {
    tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

fn insert_keys(
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

#[tokio::test]
async fn test_snapshot_basic() -> Result<(), anyhow::Error> {
    let db_path = temp_dir();
    let local = temp_dir().join("local_dir");
    let remote = temp_dir().join("remote_dir");
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
        0,
        local_store_config,
        remote_store_config,
        FileCompression::Zstd,
        NonZeroUsize::new(1).unwrap(),
    )
    .await?;
    let perpetual_db = AuthorityPerpetualTables::open(&db_path, None);
    insert_keys(&perpetual_db, 1000)?;
    snapshot_writer.write(&perpetual_db).await?;
    // TODO: Read the live object set from remote store and assert it is the same as local
    Ok(())
}

#[tokio::test]
async fn test_snapshot_empty_db() -> Result<(), anyhow::Error> {
    let db_path = temp_dir();
    let local = temp_dir().join("local_dir");
    let remote = temp_dir().join("remote_dir");
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
        0,
        local_store_config,
        remote_store_config,
        FileCompression::Zstd,
        NonZeroUsize::new(1).unwrap(),
    )
    .await?;
    let perpetual_db = AuthorityPerpetualTables::open(&db_path, None);
    snapshot_writer.write(&perpetual_db).await?;
    // TODO: Read the live object set from remote store and assert it is the same as local
    Ok(())
}
