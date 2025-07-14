// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_tables::AuthorityPerpetualTables;

use super::*;
use futures::FutureExt;
use std::path::Path;
use std::time::Duration;
use sui_framework::BuiltInFramework;
use sui_move_build::BuildConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::object::{Object, Owner};
use sui_types::storage::InputKey;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use tempfile::tempdir;
use tokio::time::timeout;

async fn create_writeback_cache() -> Arc<WritebackCache> {
    let path = tempdir().unwrap();
    let tables = Arc::new(AuthorityPerpetualTables::open(path.path(), None));
    let config = ConfigBuilder::new_with_temp_dir().build();
    let store = AuthorityStore::open_with_committee_for_testing(
        tables,
        config.committee_with_network().committee(),
        &config.genesis,
    )
    .await
    .unwrap();
    Arc::new(WritebackCache::new_for_tests(store))
}

#[tokio::test]
async fn test_immediate_return_canceled_shared() {
    let cache = create_writeback_cache().await;

    let canceled_key = InputKey::VersionedObject {
        id: FullObjectID::new(ObjectID::random(), Some(SequenceNumber::from(1))),
        version: SequenceNumber::CANCELLED_READ,
    };
    let receiving_keys = HashSet::new();
    let epoch = &0;

    // Should return immediately since canceled shared objects are always available
    cache
        .notify_read_input_objects(&[canceled_key], &receiving_keys, epoch)
        .now_or_never()
        .unwrap();

    let congested_key = InputKey::VersionedObject {
        id: FullObjectID::new(ObjectID::random(), Some(SequenceNumber::from(1))),
        version: SequenceNumber::CONGESTED,
    };

    cache
        .notify_read_input_objects(&[congested_key], &receiving_keys, epoch)
        .now_or_never()
        .unwrap();

    let randomness_unavailable_key = InputKey::VersionedObject {
        id: FullObjectID::new(ObjectID::random(), Some(SequenceNumber::from(1))),
        version: SequenceNumber::RANDOMNESS_UNAVAILABLE,
    };

    cache
        .notify_read_input_objects(&[randomness_unavailable_key], &receiving_keys, epoch)
        .now_or_never()
        .unwrap();
}

#[tokio::test]
async fn test_immediate_return_cached_object() {
    let cache = create_writeback_cache().await;

    let object_id = ObjectID::random();
    let version = SequenceNumber::from(1);
    let object = Object::with_id_owner_version_for_testing(object_id, version, Owner::Immutable);

    cache.write_object_entry(&object_id, version, ObjectEntry::Object(object));

    let input_keys = vec![InputKey::VersionedObject {
        id: FullObjectID::new(object_id, None),
        version,
    }];
    let receiving_keys = HashSet::new();
    let epoch = &0;

    // Should return immediately since object is in cache
    cache
        .notify_read_input_objects(&input_keys, &receiving_keys, epoch)
        .now_or_never()
        .unwrap();
}

#[tokio::test]
async fn test_immediate_return_cached_package() {
    let cache = create_writeback_cache().await;

    let input_keys = vec![InputKey::Package {
        id: SUI_FRAMEWORK_PACKAGE_ID,
    }];
    let receiving_keys = HashSet::new();
    let epoch = &0;

    // Should return immediately since system package is available by default.
    cache
        .notify_read_input_objects(&input_keys, &receiving_keys, epoch)
        .now_or_never()
        .unwrap();
}

#[tokio::test]
async fn test_immediate_return_consensus_stream_ended() {
    let cache = create_writeback_cache().await;

    let object_id = ObjectID::random();
    let version = SequenceNumber::from(1);
    let epoch = 0;

    // Write consensus stream ended marker
    cache.write_marker_value(
        epoch,
        FullObjectKey::new(FullObjectID::new(object_id, Some(version)), version),
        MarkerValue::ConsensusStreamEnded(TransactionDigest::random()),
    );

    let input_keys = vec![InputKey::VersionedObject {
        id: FullObjectID::new(object_id, Some(version)),
        version,
    }];
    let receiving_keys = HashSet::new();

    // Should return immediately since object is marked as consensus stream ended
    cache
        .notify_read_input_objects(&input_keys, &receiving_keys, &epoch)
        .now_or_never()
        .unwrap();
}

#[tokio::test]
async fn test_wait_for_object() {
    let cache = create_writeback_cache().await;

    let object_id = ObjectID::random();
    let version = SequenceNumber::from(1);

    let input_keys = vec![InputKey::VersionedObject {
        id: FullObjectID::new(object_id, Some(version)),
        version,
    }];
    let receiving_keys = HashSet::new();
    let epoch = &0;

    let result = timeout(
        Duration::from_secs(3),
        cache.notify_read_input_objects(&input_keys, &receiving_keys, epoch),
    )
    .await;
    assert!(result.is_err());

    // Write an older version of the object.
    tokio::spawn({
        let cache = cache.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let object = Object::with_id_owner_version_for_testing(
                object_id,
                SequenceNumber::from(0),
                Owner::Shared {
                    initial_shared_version: version,
                },
            );
            cache.write_object_entry_for_test(object);
        }
    });
    let result = timeout(
        Duration::from_secs(3),
        cache.notify_read_input_objects(&input_keys, &receiving_keys, epoch),
    )
    .await;
    assert!(result.is_err());

    // Write the correct version of the object.
    tokio::spawn({
        let cache = cache.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let object = Object::with_id_owner_version_for_testing(
                object_id,
                version,
                Owner::Shared {
                    initial_shared_version: version,
                },
            );
            cache.write_object_entry_for_test(object);
        }
    });
    timeout(
        Duration::from_secs(3),
        cache.notify_read_input_objects(&input_keys, &receiving_keys, epoch),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn test_wait_for_package() {
    let cache = create_writeback_cache().await;

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/move/basics");
    let compiled_modules = BuildConfig::new_for_testing()
        .build(&path)
        .unwrap()
        .into_modules();
    let package = Object::new_package_for_testing(
        &compiled_modules,
        TransactionDigest::genesis_marker(),
        BuiltInFramework::genesis_move_packages(),
    )
    .unwrap();
    let package_id = package.id();
    let version = package.version();

    let input_keys = vec![InputKey::Package { id: package_id }];
    let receiving_keys = HashSet::new();
    let epoch = &0;

    // Start notification future
    let notification = cache.notify_read_input_objects(&input_keys, &receiving_keys, epoch);

    // Write package after small delay
    tokio::spawn({
        let cache = cache.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cache.write_object_entry(&package_id, version, ObjectEntry::Object(package));
        }
    });

    // Should complete once package is written
    timeout(Duration::from_secs(1), notification).await.unwrap();
}

#[tokio::test]
async fn test_wait_for_consensus_stream_end() {
    let cache = create_writeback_cache().await;

    let object_id = ObjectID::random();
    let version = SequenceNumber::from(1);
    let epoch = &0;

    let input_keys = vec![InputKey::VersionedObject {
        id: FullObjectID::new(object_id, Some(version)),
        version,
    }];
    let receiving_keys = HashSet::new();

    // Start notification future
    let notification = cache.notify_read_input_objects(&input_keys, &receiving_keys, epoch);

    // Write consensus stream ended marker after small delay
    tokio::spawn({
        let cache = cache.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cache.write_marker_value(
                *epoch,
                FullObjectKey::new(FullObjectID::new(object_id, Some(version)), version),
                MarkerValue::ConsensusStreamEnded(TransactionDigest::random()),
            );
        }
    });

    // Should complete once marker is written
    timeout(Duration::from_secs(1), notification).await.unwrap();
}

#[tokio::test]
async fn test_receiving_object_higher_version() {
    let cache = create_writeback_cache().await;

    let object_id = ObjectID::random();
    let requested_version = SequenceNumber::from(1);
    let higher_version = SequenceNumber::from(2);
    let object = Object::with_id_owner_version_for_testing(
        object_id,
        higher_version,
        Owner::AddressOwner(SuiAddress::default()),
    );

    // Write higher version to cache
    cache.write_object_entry(&object_id, higher_version, ObjectEntry::Object(object));

    let input_keys = vec![InputKey::VersionedObject {
        id: FullObjectID::new(object_id, None),
        version: requested_version,
    }];
    let mut receiving_keys = HashSet::new();
    receiving_keys.insert(input_keys[0]);
    let epoch = &0;

    // Should return immediately since a higher version exists for receiving object
    cache
        .notify_read_input_objects(&input_keys, &receiving_keys, epoch)
        .now_or_never()
        .unwrap();
}
