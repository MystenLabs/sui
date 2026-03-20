// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Result;
use sui_data_store::{
    Node, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, VersionQuery,
    stores::{DataStore, FileSystemStore, InMemoryStore, ReadThroughStore, WriteThroughStore},
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    object::{Object, Owner},
};

const CHAIN_ID: &str = "test_chain";

fn make_store() -> Result<(tempfile::TempDir, FileSystemStore)> {
    let tempdir = tempfile::tempdir()?;
    let store = FileSystemStore::new_with_path(Node::Testnet, tempdir.path().to_path_buf())?;
    store.setup(Some(CHAIN_ID.to_string()))?;
    Ok((tempdir, store))
}

fn sample_object(object_id: ObjectID, owner: SuiAddress, version: u64) -> Object {
    Object::with_id_owner_version_for_testing(
        object_id,
        SequenceNumber::from_u64(version),
        Owner::AddressOwner(owner),
    )
}

#[derive(Default)]
struct ObjectSourceStub {
    objects: RwLock<BTreeMap<ObjectKey, (Object, u64)>>,
    read_calls: AtomicUsize,
    write_calls: AtomicUsize,
}

impl ObjectStore for ObjectSourceStub {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        self.read_calls.fetch_add(1, Ordering::Relaxed);
        let objects = self.objects.read().unwrap();
        Ok(keys.iter().map(|key| objects.get(key).cloned()).collect())
    }
}

impl ObjectStoreWriter for ObjectSourceStub {
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), anyhow::Error> {
        self.write_calls.fetch_add(1, Ordering::Relaxed);
        self.objects
            .write()
            .unwrap()
            .insert(key.clone(), (object, actual_version));
        Ok(())
    }
}

#[test]
fn filesystem_store_round_trips_objects_and_helpers() -> Result<()> {
    let (_tempdir, store) = make_store()?;
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

    Ok(())
}

#[test]
fn read_through_backfills_object_misses_into_primary() -> Result<()> {
    let primary = InMemoryStore::new(Node::Testnet);
    let secondary = ObjectSourceStub::default();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = sample_object(object_id, owner, 7);
    let key = ObjectKey {
        object_id,
        version_query: VersionQuery::AtCheckpoint(22),
    };
    secondary
        .objects
        .write()
        .unwrap()
        .insert(key.clone(), (object.clone(), 7));

    let store = ReadThroughStore::new(&primary, &secondary);
    let loaded = ObjectStore::get_objects(&store, std::slice::from_ref(&key))?;
    let (loaded_object, actual_version) = loaded[0].clone().expect("object should load");

    assert_eq!(actual_version, 7);
    assert_eq!(bcs::to_bytes(&loaded_object)?, bcs::to_bytes(&object)?);
    assert_eq!(secondary.read_calls.load(Ordering::Relaxed), 1);
    assert!(ObjectStore::get_objects(&primary, &[key])?[0].is_some());

    Ok(())
}

#[test]
fn write_through_reads_and_writes_across_both_layers() -> Result<()> {
    let primary = InMemoryStore::new(Node::Testnet);
    let secondary = ObjectSourceStub::default();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = sample_object(object_id, owner, 4);
    let key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(4),
    };
    secondary
        .objects
        .write()
        .unwrap()
        .insert(key.clone(), (object.clone(), 4));

    let store = WriteThroughStore::new(&primary, &secondary);
    let loaded = ObjectStore::get_objects(&store, std::slice::from_ref(&key))?;
    assert_eq!(loaded[0].as_ref().map(|(_, version)| *version), Some(4));
    assert!(ObjectStore::get_objects(&primary, std::slice::from_ref(&key))?[0].is_some());

    let new_object = sample_object(object_id, owner, 5);
    let new_key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(5),
    };
    ObjectStoreWriter::write_object(&store, &new_key, new_object.clone(), 5)?;

    let primary_loaded = ObjectStore::get_objects(&primary, std::slice::from_ref(&new_key))?;
    let secondary_loaded = ObjectStore::get_objects(&secondary, std::slice::from_ref(&new_key))?;
    assert!(primary_loaded[0].is_some());
    assert!(secondary_loaded[0].is_some());

    Ok(())
}

#[test]
fn arc_forwarding_impls_compile_for_object_store_shapes() {
    fn assert_object_store<T: ObjectStore>() {}

    assert_object_store::<Arc<FileSystemStore>>();
    assert_object_store::<Arc<ReadThroughStore<InMemoryStore, FileSystemStore>>>();
    assert_object_store::<Arc<WriteThroughStore<InMemoryStore, FileSystemStore>>>();
    assert_object_store::<
        Arc<WriteThroughStore<InMemoryStore, ReadThroughStore<FileSystemStore, DataStore>>>,
    >();
}
