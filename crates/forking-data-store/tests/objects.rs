// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use forking_data_store::{
    LatestObjectStore, Node, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, VersionQuery,
    stores::{FileSystemStore, ForkingStore, InMemoryStore, ReadThroughStore, WriteThroughStore},
};
use mockall::{mock, predicate::function};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    digests::get_mainnet_chain_identifier,
    object::{Object, Owner},
};
use tempfile::tempdir;

mock! {
    ObjectLayer {}

    impl ObjectStore for ObjectLayer {
        fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error>;
    }

    impl ObjectStoreWriter for ObjectLayer {
        fn write_object(&self, key: &ObjectKey, object: Object, actual_version: u64) -> Result<(), Error>;
    }
}

fn test_object(object_id: ObjectID, owner: SuiAddress, version: u64) -> Object {
    Object::with_id_owner_version_for_testing(
        object_id,
        SequenceNumber::from_u64(version),
        Owner::AddressOwner(owner),
    )
}

fn configured_fs_store(root: &std::path::Path, fork_checkpoint: u64) -> FileSystemStore {
    let store =
        FileSystemStore::new_with_path_for_fork(Node::Mainnet, root.to_path_buf(), fork_checkpoint)
            .unwrap();
    store
        .setup(Some(get_mainnet_chain_identifier().to_string()))
        .unwrap();
    store
}

#[test]
fn filesystem_store_round_trips_object_queries_and_scopes_by_fork() {
    let root = tempdir().unwrap();
    let store = configured_fs_store(root.path(), 50);
    let sibling_fork_store = configured_fs_store(root.path(), 51);
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object_v1 = test_object(object_id, owner, 1);
    let object_v3 = test_object(object_id, owner, 3);

    store
        .write_object(
            &ObjectKey {
                object_id,
                version_query: VersionQuery::Version(1),
            },
            object_v1.clone(),
            1,
        )
        .unwrap();
    store
        .write_object(
            &ObjectKey {
                object_id,
                version_query: VersionQuery::RootVersion(3),
            },
            object_v3.clone(),
            3,
        )
        .unwrap();
    store
        .write_object(
            &ObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(50),
            },
            object_v3.clone(),
            3,
        )
        .unwrap();

    let objects = store
        .get_objects(&[
            ObjectKey {
                object_id,
                version_query: VersionQuery::Version(1),
            },
            ObjectKey {
                object_id,
                version_query: VersionQuery::RootVersion(3),
            },
            ObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(50),
            },
        ])
        .unwrap();

    assert_eq!(objects[0].as_ref().unwrap().0.version().value(), 1);
    assert_eq!(objects[1].as_ref().unwrap().0.version().value(), 3);
    assert_eq!(objects[2].as_ref().unwrap().0.version().value(), 3);
    assert_eq!(
        store
            .latest_object(&object_id)
            .unwrap()
            .unwrap()
            .0
            .version()
            .value(),
        3
    );
    assert!(
        sibling_fork_store
            .get_objects(&[ObjectKey {
                object_id,
                version_query: VersionQuery::Version(1),
            }])
            .unwrap()[0]
            .is_none()
    );
}

#[test]
fn read_through_store_backfills_object_misses_into_primary() {
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = test_object(object_id, owner, 7);
    let key = ObjectKey {
        object_id,
        version_query: VersionQuery::AtCheckpoint(50),
    };
    let expected_key = key.clone();

    let mut primary = MockObjectLayer::new();
    primary.expect_get_objects().times(1).return_once(|keys| {
        assert_eq!(keys.len(), 1);
        Ok(vec![None])
    });
    primary
        .expect_write_object()
        .times(1)
        .with(
            function(move |candidate: &ObjectKey| candidate == &expected_key),
            function(move |candidate: &Object| candidate.id() == object_id),
            function(|version: &u64| *version == 7),
        )
        .return_once(|_, _, _| Ok(()));

    let mut secondary = MockObjectLayer::new();
    secondary
        .expect_get_objects()
        .times(1)
        .return_once(move |_| Ok(vec![Some((object, 7))]));

    let store = ReadThroughStore::new(primary, secondary);
    let result = store.get_objects(&[key]).unwrap();
    assert_eq!(result[0].as_ref().unwrap().1, 7);
}

#[test]
fn write_through_store_writes_objects_to_secondary_then_primary() {
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = test_object(object_id, owner, 9);
    let key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(9),
    };

    let mut primary = MockObjectLayer::new();
    primary.expect_get_objects().times(0);
    primary
        .expect_write_object()
        .times(1)
        .return_once(|_, _, _| Ok(()));

    let mut secondary = MockObjectLayer::new();
    secondary.expect_get_objects().times(0);
    secondary
        .expect_write_object()
        .times(1)
        .return_once(|_, _, _| Ok(()));

    let store = WriteThroughStore::new(primary, secondary);
    store.write_object(&key, object, 9).unwrap();
}

#[test]
fn forking_store_routes_object_reads_and_writes_to_object_lane() {
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = test_object(object_id, owner, 11);
    let key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(11),
    };

    let mut object_store = MockObjectLayer::new();
    object_store.expect_get_objects().times(1).return_once({
        let object = object.clone();
        move |_| Ok(vec![Some((object, 11))])
    });
    object_store
        .expect_write_object()
        .times(1)
        .return_once(|_, _, _| Ok(()));

    let store = ForkingStore::new(
        InMemoryStore::new(Node::Mainnet),
        InMemoryStore::new(Node::Mainnet),
        object_store,
        InMemoryStore::new(Node::Mainnet),
    );

    assert_eq!(
        store.get_objects(std::slice::from_ref(&key)).unwrap()[0]
            .as_ref()
            .unwrap()
            .1,
        11
    );
    store.write_object(&key, object, 11).unwrap();
}

#[cfg(feature = "test-utils")]
#[test]
fn feature_gated_generated_object_mock_is_available_to_downstream_tests() {
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = test_object(object_id, owner, 5);

    let mut store = forking_data_store::MockObjectStore::new();
    store
        .expect_get_objects()
        .times(1)
        .return_once(move |keys| {
            assert_eq!(keys.len(), 1);
            Ok(vec![Some((object, 5))])
        });

    assert_eq!(
        store
            .get_objects(&[ObjectKey {
                object_id,
                version_query: VersionQuery::Version(5),
            }])
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .1,
        5
    );
}
