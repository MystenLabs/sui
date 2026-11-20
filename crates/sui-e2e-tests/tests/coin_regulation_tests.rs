// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_types::SUI_FREEZE_LIST_OBJECT_ID;
use sui_types::object::Owner;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use test_cluster::TestClusterBuilder;

/// The FreezeList must be created by the end-of-epoch transaction when a chain upgrades to
/// protocol version 145, since only freshly-created networks get it at genesis.
#[sim_test]
async fn test_create_freeze_list_object_at_upgrade() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(144.into())
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(144, 145))
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    for h in &handles {
        h.with(|node| {
            assert!(
                node.state()
                    .get_object_cache_reader()
                    .get_latest_object_ref_or_tombstone(SUI_FREEZE_LIST_OBJECT_ID)
                    .is_none()
            );
        });
    }

    test_cluster.wait_for_protocol_version(145.into()).await;
    // The upgrade completes in epoch 1, whose end-of-epoch transaction creates the FreezeList.
    test_cluster.wait_for_epoch_all_nodes(2).await;

    for h in &handles {
        h.with(|node| {
            let state = node.state();
            let freeze_list = state
                .get_object_cache_reader()
                .get_object(&SUI_FREEZE_LIST_OBJECT_ID)
                .expect("freeze list object should exist");
            let Owner::Shared {
                initial_shared_version,
            } = freeze_list.owner
            else {
                panic!("the freeze list must be shared");
            };
            assert_eq!(
                state
                    .epoch_store_for_testing()
                    .epoch_start_config()
                    .freeze_list_obj_initial_shared_version(),
                Some(initial_shared_version),
            );
        });
    }
}
