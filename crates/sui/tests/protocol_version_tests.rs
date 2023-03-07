// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::RegistryService;
use prometheus::Registry;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion, SupportedProtocolVersions};
use test_utils::authority::start_node;

#[tokio::test]
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

#[cfg(msim)]
mod sim_only_tests {

    use super::*;
    use fastcrypto::encoding::Base64;
    use move_binary_format::CompiledModule;
    use move_core_types::ident_str;
    use std::path::PathBuf;
    use std::sync::Arc;
    use sui_core::authority::sui_framework_injection;
    use sui_framework_build::compiled_package::BuildConfig;
    use sui_json_rpc::api::WriteApiClient;
    use sui_macros::*;
    use sui_protocol_config::{ProtocolVersion, SupportedProtocolVersions};
    use sui_types::{
        digests::TransactionDigest,
        messages::{MoveCall, SingleTransactionKind, TransactionKind},
        object::{Object, OBJECT_START_VERSION},
        SUI_FRAMEWORK_OBJECT_ID,
    };
    use test_utils::network::{TestCluster, TestClusterBuilder};
    use tokio::time::{sleep, timeout, Duration};
    use tracing::info;

    #[sim_test]
    async fn test_protocol_version_upgrade() {
        ProtocolConfig::poison_get_for_min_version();

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(10000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(1, 2))
            .build()
            .await
            .unwrap();

        monitor_version_change(&test_cluster, 2 /* expected proto version */).await;
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_one_laggard() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            config
        });

        ProtocolConfig::poison_get_for_min_version();

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

        // verify that the node that didn't support the new version shut itself down.
        for v in test_cluster.swarm.validators() {
            if !v
                .config
                .supported_protocol_versions
                .unwrap()
                .is_version_supported(ProtocolVersion::new(2))
            {
                assert!(!v.is_running(), "{:?}", v.name().concise());
            } else {
                assert!(v.is_running(), "{:?}", v.name().concise());
            }
        }
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_no_quorum() {
        ProtocolConfig::poison_get_for_min_version();

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

    #[sim_test]
    async fn test_framework_compatible_upgrade() {
        // Make a number of compatible changes, and expect the upgrade to go through:
        // - Add a new module, struct, and function
        // - Add a new ability to an existing struct
        // - Remove an ability from an existing type constraint
        // - Change the implementation of an existing function
        // - Change the signature and implementation of a private function
        // - Remove a private function.
        // - Promote a non-public function to public.
        // - Promote a non-entry function to entry.
        let cluster = run_framework_upgrade("base", "compatible").await;
        assert_eq!(call_canary(&cluster).await, 42);
        expect_upgrade_succeeded(&cluster).await;
        assert_eq!(call_canary(&cluster).await, 43);
    }

    #[sim_test]
    async fn test_framework_incompatible_struct_layout() {
        // Upgrade attempts to change an existing struct layout
        let cluster = run_framework_upgrade("base", "change_struct_layout").await;
        assert_eq!(call_canary(&cluster).await, 42);
        expect_upgrade_failed(&cluster).await;
        assert_eq!(call_canary(&cluster).await, 42);
    }

    #[sim_test]
    async fn test_framework_incompatible_struct_ability() {
        // Upgrade attempts to remove an ability from a struct
        let cluster = run_framework_upgrade("base", "change_struct_ability").await;
        assert_eq!(call_canary(&cluster).await, 42);
        expect_upgrade_failed(&cluster).await;
        assert_eq!(call_canary(&cluster).await, 42);
    }

    #[sim_test]
    async fn test_framework_incompatible_type_constraint() {
        // Upgrade attempts to add a new type constraint to a generic type parameter
        let cluster = run_framework_upgrade("base", "change_type_constraint").await;
        assert_eq!(call_canary(&cluster).await, 42);
        expect_upgrade_failed(&cluster).await;
        assert_eq!(call_canary(&cluster).await, 42);
    }

    #[sim_test]
    async fn test_framework_incompatible_public_function_signature() {
        // Upgrade attempts to change the signature of a public function
        let cluster = run_framework_upgrade("base", "change_public_function_signature").await;
        assert_eq!(call_canary(&cluster).await, 42);
        expect_upgrade_failed(&cluster).await;
        assert_eq!(call_canary(&cluster).await, 42);
    }

    #[sim_test]
    async fn test_framework_incompatible_entry_function_signature() {
        // Upgrade attempts to change the signature of an entry function
        let cluster = run_framework_upgrade("base", "change_entry_function_signature").await;
        assert_eq!(call_canary(&cluster).await, 42);
        expect_upgrade_failed(&cluster).await;
        assert_eq!(call_canary(&cluster).await, 42);
    }

    async fn run_framework_upgrade(from: &str, to: &str) -> TestCluster {
        ProtocolConfig::poison_get_for_min_version();

        sui_framework_injection::set_override(sui_framework(to));
        TestClusterBuilder::new()
            .with_epoch_duration_ms(10000)
            .with_objects([sui_framework_object(from)])
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(1, 2))
            .build()
            .await
            .unwrap()
    }

    async fn call_canary(cluster: &TestCluster) -> u64 {
        let client = cluster.rpc_client();
        let sender = cluster.accounts.first().cloned().unwrap();

        let txn = TransactionKind::Single(SingleTransactionKind::Call(MoveCall {
            package: SUI_FRAMEWORK_OBJECT_ID,
            module: ident_str!("msim_extra_1").to_owned(),
            function: ident_str!("canary").to_owned(),
            type_arguments: vec![],
            arguments: vec![],
        }));

        let response = client
            .dev_inspect_transaction(
                sender,
                Base64::from_bytes(&bcs::to_bytes(&txn).unwrap()),
                /* gas_price */ None,
                /* epoch_id */ None,
            )
            .await
            .unwrap();

        let results = response.results.unwrap();
        let return_ = &results.first().unwrap().1.return_values.first().unwrap().0;

        bcs::from_bytes(&return_).unwrap()
    }

    async fn expect_upgrade_failed(cluster: &TestCluster) {
        monitor_version_change(&cluster, 1 /* expected proto version */).await;
    }

    async fn expect_upgrade_succeeded(cluster: &TestCluster) {
        monitor_version_change(&cluster, 2 /* expected proto version */).await;
    }

    #[sim_test]
    async fn test_framework_compatible_upgrade_no_protocol_version() {
        ProtocolConfig::poison_get_for_min_version();

        // Even though a new framework is available, the required new protocol version is not.
        sui_framework_injection::set_override(sui_framework("compatible"));
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(10000)
            .with_objects([sui_framework_object("base")])
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(1, 1))
            .build()
            .await
            .unwrap();

        monitor_version_change(&test_cluster, 1 /* expected proto version */).await;
    }

    #[sim_test]
    #[ignore = "Blocked by checkpointing problems"]
    async fn test_framework_upgrade_conflicting_versions() {
        ProtocolConfig::poison_get_for_min_version();
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            config
        });

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(10000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(1, 2))
            .build()
            .await
            .unwrap();

        let first = test_cluster.swarm.validators().next().unwrap();
        let first_name = first.name();
        sui_framework_injection::set_override_cb(Box::new(move |name| {
            if name == first_name {
                info!("node {:?} using compatible packages", name.concise());
                Some(sui_framework("base"))
            } else {
                Some(sui_framework("compatible"))
            }
        }));

        monitor_version_change(&test_cluster, 2 /* expected proto version */).await;

        // monitor_version_change only waits for fullnode to reconfigure - validator can actually be
        // slower than fullnode if it wasn't one of the signers of the final checkpoint.
        sleep(Duration::from_secs(3)).await;

        let node_handle = first.get_node_handle().expect("node should be running");
        // The dissenting node receives the correct framework via state sync and completes the upgrade
        node_handle.with(|node| {
            let committee = node.state().epoch_store_for_testing().committee().clone();
            assert_eq!(
                node.state().epoch_store_for_testing().protocol_version(),
                ProtocolVersion::new(2)
            );
            assert_eq!(committee.epoch, 2);
        });
    }

    // Test that protocol version upgrade does not complete when there is no quorum on the
    // framework upgrades.
    #[sim_test]
    async fn test_framework_upgrade_conflicting_versions_no_quorum() {
        ProtocolConfig::poison_get_for_min_version();
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            config
        });

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(10000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(1, 2))
            .build()
            .await
            .unwrap();

        let mut validators = test_cluster.swarm.validators();
        let first = validators.next().unwrap().name();
        let second = validators.next().unwrap().name();
        sui_framework_injection::set_override_cb(Box::new(move |name| {
            if name == first || name == second {
                Some(sui_framework("compatible"))
            } else {
                None
            }
        }));

        monitor_version_change(&test_cluster, 1 /* expected proto version */).await;
    }

    async fn monitor_version_change(test_cluster: &TestCluster, final_version: u64) {
        let mut epoch_rx = test_cluster
            .fullnode_handle
            .sui_node
            .subscribe_to_epoch_change();

        timeout(Duration::from_secs(60), async move {
            while let Ok((committee, protocol_version)) = epoch_rx.recv().await {
                info!(
                    "received epoch {} {:?}",
                    committee.epoch(),
                    protocol_version
                );
                match committee.epoch() {
                    0 => assert_eq!(protocol_version, ProtocolVersion::new(1)),
                    1 => assert_eq!(protocol_version, ProtocolVersion::new(final_version)),
                    2 => break,
                    _ => unreachable!(),
                }
            }
        })
        .await
        .expect("Timed out waiting for cluster to target epoch");
    }

    /// Get compiled modules for Sui Framework, built from fixture `fixture` in the
    /// `framework_upgrades` directory.
    fn sui_framework(fixture: &str) -> Vec<CompiledModule> {
        let mut package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        package.extend(["tests", "framework_upgrades", fixture]);

        let mut config = BuildConfig::new_for_testing();
        config.run_bytecode_verifier = true;

        let pkg = config.build(package).unwrap();
        pkg.get_framework_modules().cloned().collect()
    }

    /// Like `sui_framework`, but package the modules in an `Object`.
    fn sui_framework_object(fixture: &str) -> Object {
        Object::new_package(
            sui_framework(fixture),
            OBJECT_START_VERSION,
            TransactionDigest::genesis(),
            u64::MAX,
        )
        .unwrap()
    }
}
