// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::RegistryService;
use prometheus::Registry;
use std::sync::Arc;
use sui_macros::*;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion, SupportedProtocolVersions};
use test_utils::{
    authority::start_node,
    network::{TestCluster, TestClusterBuilder},
};
use tokio::time::{timeout, Duration};
use tracing::info;

#[sim_test]
#[should_panic]
async fn test_validator_panics_on_unsupported_protocol_version() {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir)
        .with_protocol_version(ProtocolVersion::new(2))
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(1, 1))
        .build();

    let registry_service = RegistryService::new(Registry::new());
    let _sui_node = start_node(&network_config.validator_configs[0], registry_service).await;
}

#[test]
fn test_protocol_overrides() {
    telemetry_subscribers::init_for_testing();

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_max_function_definitions_for_testing(42);
        config
    });

    assert_eq!(
        ProtocolConfig::get_for_max_version().max_function_definitions(),
        42
    );
}

// Same as the previous test, to ensure we have test isolation with all the caching that
// happens in get_for_min_version/get_for_max_version.
#[test]
fn test_protocol_overrides_2() {
    telemetry_subscribers::init_for_testing();

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_max_function_definitions_for_testing(43);
        config
    });

    assert_eq!(
        ProtocolConfig::get_for_max_version().max_function_definitions(),
        43
    );
}

#[sim_test]
async fn test_protocol_version_upgrade() {
    telemetry_subscribers::init_for_testing();
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(1, 2))
        .build()
        .await
        .unwrap();

    monitor_version_change(&test_cluster, 2 /* expected proto version */).await;
}

// TODO: should_panic should be removed - however we need to add the ability to model intentional
// shutdown to the simulator first. Currently in this test the network proceeds to version 2 just
// fine, but the validator that doesn't support it panics.
#[sim_test]
#[should_panic]
async fn test_protocol_version_upgrade_one_laggard() {
    telemetry_subscribers::init_for_testing();
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_version_callback(Arc::new(|idx, name| {
            if name.is_some() && idx == 0 {
                // first validator only does not support version 2.
                SupportedProtocolVersions::new_for_testing(1, 1)
            } else {
                SupportedProtocolVersions::new_for_testing(1, 2)
            }
        }))
        .build()
        .await
        .unwrap();

    monitor_version_change(&test_cluster, 2 /* expected proto version */).await;
}

#[sim_test]
async fn test_protocol_version_upgrade_no_quorum() {
    telemetry_subscribers::init_for_testing();
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .with_supported_protocol_version_callback(Arc::new(|idx, name| {
            if name.is_some() && idx <= 1 {
                // two validators don't support version 2, so we never advance to 2.
                SupportedProtocolVersions::new_for_testing(1, 1)
            } else {
                SupportedProtocolVersions::new_for_testing(1, 2)
            }
        }))
        .build()
        .await
        .unwrap();

    monitor_version_change(&test_cluster, 1 /* expected proto version */).await;
}

async fn monitor_version_change(test_cluster: &TestCluster, final_version: u64) {
    let mut epoch_rx = test_cluster
        .fullnode_handle
        .sui_node
        .subscribe_to_epoch_change();

    timeout(Duration::from_secs(60), async move {
        while let Ok(committee) = epoch_rx.recv().await {
            info!(
                "received epoch {} {:?}",
                committee.epoch, committee.protocol_version
            );
            match committee.epoch {
                0 => assert_eq!(committee.protocol_version, ProtocolVersion::new(1)),
                1 => assert_eq!(
                    committee.protocol_version,
                    ProtocolVersion::new(final_version)
                ),
                2 => break,
                _ => unreachable!(),
            }
        }
    })
    .await
    .expect("Timed out waiting for cluster to target epoch");
}
