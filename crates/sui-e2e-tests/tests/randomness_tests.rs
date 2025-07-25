// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use sui_types::SUI_RANDOMNESS_STATE_OBJECT_ID;
use test_cluster::TestClusterBuilder;

use sui_macros::sim_test;

#[sim_test]
async fn test_create_randomness_state_object() {
    #[cfg(msim)]
    {
        use sui_core::authority::framework_injection;
        let framework = sui_framework_snapshot::load_bytecode_snapshot(54).unwrap();
        framework_injection::set_system_packages(framework);
    }

    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(31.into())
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(31, 54))
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // no node has the randomness state object yet
    for h in &handles {
        h.with(|node| {
            assert!(node
                .state()
                .get_object_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_RANDOMNESS_STATE_OBJECT_ID)
                .is_none());
        });
    }

    // wait until feature is enabled
    test_cluster.wait_for_protocol_version(32.into()).await;
    // wait until next epoch - randomness state object is created at the end of the first epoch
    // in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    for h in &handles {
        h.with(|node| {
            node.state()
                .get_object_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_RANDOMNESS_STATE_OBJECT_ID)
                .expect("randomness state object should exist");
        });
    }
}
