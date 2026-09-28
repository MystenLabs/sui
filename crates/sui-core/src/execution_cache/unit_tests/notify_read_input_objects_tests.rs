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
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::{ConsensusObjectVersion, ObjectID, SequenceNumber, SuiAddress};
use sui_types::object::{Object, Owner};
use sui_types::storage::{BackingStore, InputKey, TrackingBackingStore};
use tempfile::tempdir;
use tokio::time::timeout;

async fn create_writeback_cache() -> Arc<WritebackCache> {
    let path = tempdir().unwrap();
    let tables = Arc::new(AuthorityPerpetualTables::open(path.path(), None, None));
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
    let epoch = 0;

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
    let epoch = 0;

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
    let epoch = 0;

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
        .notify_read_input_objects(&input_keys, &receiving_keys, epoch)
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
    let epoch = 0;

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
    let epoch = 0;

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
    let epoch = 0;

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
                epoch,
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
    let epoch = 0;

    // Should return immediately since a higher version exists for receiving object
    cache
        .notify_read_input_objects(&input_keys, &receiving_keys, epoch)
        .now_or_never()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_load_implicitly_read_system_object() {
    let cache = create_writeback_cache().await;

    let object_id = sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
    let init_version = SequenceNumber::from(1);
    let target_version = SequenceNumber::from(3);

    let below_target = Object::with_id_owner_version_for_testing(
        object_id,
        SequenceNumber::from(2),
        Owner::Shared {
            initial_shared_version: init_version,
        },
    );
    cache.write_object_entry_for_test(below_target);

    let semaphore = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = semaphore.clone().try_acquire_owned().unwrap();
    assert_eq!(semaphore.available_permits(), 0);
    let blocked = tokio::task::spawn_blocking({
        let cache = cache.clone();
        move || {
            let _permit_guard =
                mysten_common::sync::execution_permit::set_execution_permit(Box::new(permit));
            cache.as_ref().load_implicitly_read_system_object(
                &object_id,
                ConsensusObjectVersion {
                    initial_shared_version: init_version,
                    version: target_version,
                },
            )
        }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    // Check that even though the task hasn't finished yet, the permit is already released.
    assert!(!blocked.is_finished());
    assert_eq!(semaphore.available_permits(), 1);

    let at_target = Object::with_id_owner_version_for_testing(
        object_id,
        target_version,
        Owner::Shared {
            initial_shared_version: init_version,
        },
    );
    cache.write_object_entry_for_test(at_target);
    let object = timeout(Duration::from_secs(3), blocked)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(object.version(), target_version);
    assert_eq!(semaphore.available_permits(), 1);

    let object = cache
        .as_ref()
        .load_implicitly_read_system_object(
            &object_id,
            ConsensusObjectVersion {
                initial_shared_version: init_version,
                version: target_version,
            },
        )
        .unwrap();
    assert_eq!(object.version(), target_version);
}

/// A dry-run pins the latest version before execution, and that version can be pruned
/// before it is read. When a newer version already exists, the load must return None
/// immediately instead of waiting for a version that will never be written again.
#[tokio::test]
async fn test_load_implicitly_read_system_object_superseded_version() {
    let cache = create_writeback_cache().await;

    let object_id = sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
    let init_version = SequenceNumber::from(1);
    for version in [2, 4] {
        cache.write_object_entry_for_test(Object::with_id_owner_version_for_testing(
            object_id,
            SequenceNumber::from(version),
            Owner::Shared {
                initial_shared_version: init_version,
            },
        ));
    }

    let object = cache.as_ref().load_implicitly_read_system_object(
        &object_id,
        ConsensusObjectVersion {
            initial_shared_version: init_version,
            version: SequenceNumber::from(3),
        },
    );
    assert!(object.is_none());
}

/// The executor only sees `&dyn BackingStore`, and `ObjectStore` has a non-blocking default
/// for this method. This calls through the same chain execution uses (`TrackingBackingStore`
/// over the cache's `BackingStore` pointer) and checks that it reaches the blocking
/// implementation in the writeback cache rather than the default.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_load_implicitly_read_system_object_via_backing_store() {
    let cache = create_writeback_cache().await;

    let object_id = sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
    let init_version = SequenceNumber::from(1);
    let target_version = SequenceNumber::from(3);

    let blocked = tokio::task::spawn_blocking({
        let cache = cache.clone();
        move || {
            let backing_store: Arc<dyn BackingStore + Send + Sync> = cache;
            let tracking_store = TrackingBackingStore::new(backing_store.as_ref());
            let store: &dyn BackingStore = &tracking_store;
            store.load_implicitly_read_system_object(
                &object_id,
                ConsensusObjectVersion {
                    initial_shared_version: init_version,
                    version: target_version,
                },
            )
        }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!blocked.is_finished());

    cache.write_object_entry_for_test(Object::with_id_owner_version_for_testing(
        object_id,
        target_version,
        Owner::Shared {
            initial_shared_version: init_version,
        },
    ));
    let object = timeout(Duration::from_secs(3), blocked)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(object.version(), target_version);
}
