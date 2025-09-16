// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use sui_types::{SUI_COIN_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;

use sui_macros::sim_test;

#[sim_test]
async fn test_create_coin_registry_object() {
    let framework = sui_framework_snapshot::load_bytecode_snapshot(95)
        .unwrap()
        .into_iter()
        .map(|p| p.genesis_object())
        .collect::<Vec<_>>();

    let package = framework
        .iter()
        .find(|f| f.id() == SUI_FRAMEWORK_PACKAGE_ID)
        .unwrap()
        .data
        .try_as_package()
        .unwrap();

    // Make sure that `coin_registry` does not exist on previous protocol version.
    assert!(!package
        .serialized_module_map()
        .contains_key("coin_registry"));

    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(95.into())
        .with_epoch_duration_ms(10000)
        .with_objects(framework)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(95, 96))
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // no node has the coin registry object yet
    for h in &handles {
        h.with(|node| {
            assert!(node
                .state()
                .get_object_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_COIN_REGISTRY_OBJECT_ID)
                .is_none());
        });
    }

    // wait until feature is enabled
    test_cluster.wait_for_protocol_version(96.into()).await;
    // wait until next epoch - coin registry object is created at the end of the first epoch
    // in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    for h in &handles {
        h.with(|node| {
            node.state()
                .get_object_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_COIN_REGISTRY_OBJECT_ID)
                .expect("coin registry object should exist");
        });
    }
}
