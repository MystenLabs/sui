// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_types::SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use test_cluster::TestClusterBuilder;

/// The registry must be created by the end-of-epoch transaction when a chain upgrades to
/// protocol version 131, since only freshly-created networks get it at genesis. Genesis at
/// protocol version 130 uses the frozen v130 framework snapshot, which predates the
/// `forwarding_address` module.
#[sim_test]
async fn test_create_forwarding_address_registry_object_at_upgrade() {
    let _guard =
        sui_protocol_config::ProtocolConfig::apply_overrides_for_testing(|version, mut config| {
            // The flag is devnet-only; force it on so the test also runs under the mainnet
            // chain override. Only from version 131: at 130 the frozen framework snapshot
            // predates the forwarding_address module, so genesis must not call it.
            if version.as_u64() >= 131 {
                config.set_create_forwarding_address_registry_for_testing(true);
            }
            config
        });

    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(130.into())
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(130, 131))
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // No node has the registry object yet.
    for h in &handles {
        h.with(|node| {
            assert!(
                node.state()
                    .get_object_cache_reader()
                    .get_latest_object_ref_or_tombstone(SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
                    .is_none()
            );
        });
    }

    test_cluster.wait_for_protocol_version(131.into()).await;
    // The registry object is created at the end of the first epoch in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    // Checking through the epoch start config also verifies that the registry's initial
    // shared version is registered there at the start of the following epoch.
    for h in &handles {
        h.with(|node| {
            node.state()
                .epoch_store_for_testing()
                .epoch_start_config()
                .forwarding_address_registry_obj_initial_shared_version()
                .expect("forwarding address registry object should exist");
        });
    }
}
