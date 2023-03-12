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
        base_types::SequenceNumber,
        digests::TransactionDigest,
        messages::{TransactionEffectsAPI, TransactionKind},
        object::{Object, OBJECT_START_VERSION},
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        storage::ObjectStore,
        SUI_FRAMEWORK_OBJECT_ID,
    };
    use test_utils::network::{TestCluster, TestClusterBuilder};
    use tokio::time::{sleep, timeout, Duration};
    use tracing::info;

    const START: u64 = ProtocolVersion::MAX.as_u64();
    const FINISH: u64 = ProtocolVersion::MAX_ALLOWED.as_u64();

    #[sim_test]
    async fn test_protocol_version_upgrade() {
        ProtocolConfig::poison_get_for_min_version();

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .build()
            .await
            .unwrap();

        expect_upgrade_succeeded(&test_cluster).await;
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_with_shutdown_validator() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            config
        });

        ProtocolConfig::poison_get_for_min_version();

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .build()
            .await
            .unwrap();

        let validator = test_cluster.get_validator_addresses()[0].clone();
        test_cluster.stop_validator(validator);

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
                    0 => assert_eq!(protocol_version, ProtocolVersion::new(START)),
                    1 => {
                        assert_eq!(protocol_version, ProtocolVersion::new(FINISH));
                        test_cluster.start_validator(validator).await;
                    }
                    2 => {
                        let validator_handle = test_cluster
                            .swarm
                            .validator(validator.clone())
                            .unwrap()
                            .get_node_handle()
                            .unwrap();
                        validator_handle
                            .with_async(|node| async {
                                // give time for restarted node to catch up, reconfig
                                // to new protocol, and reconfig again
                                sleep(Duration::from_secs(5)).await;

                                let epoch_store = node.state().epoch_store_for_testing();
                                assert_eq!(epoch_store.epoch(), 2);
                                assert!(node.state().is_validator(&epoch_store));
                            })
                            .await;
                        break;
                    }
                    _ => unreachable!(),
                }
            }
        })
        .await
        .expect("Timed out waiting for cluster to target epoch");
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_one_laggard() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            config
        });

        ProtocolConfig::poison_get_for_min_version();

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_version_callback(Arc::new(|idx, name| {
                if name.is_some() && idx == 0 {
                    // first validator only does not support version FINISH.
                    SupportedProtocolVersions::new_for_testing(START, START)
                } else {
                    SupportedProtocolVersions::new_for_testing(START, FINISH)
                }
            }))
            .build()
            .await
            .unwrap();

        expect_upgrade_succeeded(&test_cluster).await;

        // verify that the node that didn't support the new version shut itself down.
        for v in test_cluster.swarm.validators() {
            if !v
                .config
                .supported_protocol_versions
                .unwrap()
                .is_version_supported(ProtocolVersion::new(FINISH))
            {
                assert!(!v.is_running(), "{:?}", v.name().concise());
            } else {
                assert!(v.is_running(), "{:?}", v.name().concise());
            }
        }
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_forced() {
        ProtocolConfig::poison_get_for_min_version();

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
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

        test_cluster.swarm.validators().for_each(|v| {
            let node_handle = v.get_node_handle().expect("node should be running");
            node_handle.with(|node| {
                node.set_override_protocol_upgrade_buffer_stake(0, 0)
                    .unwrap()
            });
        });

        // upgrade happens with only 3 votes
        monitor_version_change(&test_cluster, 2 /* expected proto version */).await;
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_no_override_cleared() {
        ProtocolConfig::poison_get_for_min_version();

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
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

        test_cluster.swarm.validators().for_each(|v| {
            let node_handle = v.get_node_handle().expect("node should be running");
            node_handle.with(|node| {
                node.set_override_protocol_upgrade_buffer_stake(0, 0)
                    .unwrap()
            });
        });

        // Verify that clearing the override is respected.
        test_cluster.swarm.validators().for_each(|v| {
            let node_handle = v.get_node_handle().expect("node should be running");
            node_handle.with(|node| {
                node.clear_override_protocol_upgrade_buffer_stake(0)
                    .unwrap()
            });
        });

        // default buffer stake is in effect, we do not advance to version 2.
        monitor_version_change(&test_cluster, 1 /* expected proto version */).await;
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_no_quorum() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            config
        });

        ProtocolConfig::poison_get_for_min_version();

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_version_callback(Arc::new(|idx, name| {
                if name.is_some() && idx <= 1 {
                    // two validators don't support version FINISH, so we never advance to FINISH.
                    SupportedProtocolVersions::new_for_testing(START, START)
                } else {
                    SupportedProtocolVersions::new_for_testing(START, FINISH)
                }
            }))
            .build()
            .await
            .unwrap();

        expect_upgrade_failed(&test_cluster).await;
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

        let (modified_at, mutated_to) = get_framework_upgrade_effects(&cluster).await;
        assert_eq!(Some(SequenceNumber::from(1)), modified_at);
        assert_eq!(Some(SequenceNumber::from(2)), mutated_to);
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
            .with_epoch_duration_ms(20000)
            .with_objects([sui_framework_object(from)])
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .build()
            .await
            .unwrap()
    }

    async fn call_canary(cluster: &TestCluster) -> u64 {
        let client = cluster.rpc_client();
        let sender = cluster.accounts.first().cloned().unwrap();

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .move_call(
                    SUI_FRAMEWORK_OBJECT_ID,
                    ident_str!("msim_extra_1").to_owned(),
                    ident_str!("canary").to_owned(),
                    vec![],
                    vec![],
                )
                .unwrap();
            builder.finish()
        };
        let txn = TransactionKind::programmable(pt);

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
        let return_ = &results.first().unwrap().return_values.first().unwrap().0;

        bcs::from_bytes(&return_).unwrap()
    }

    async fn expect_upgrade_failed(cluster: &TestCluster) {
        monitor_version_change(&cluster, START /* expected proto version */).await;
    }

    async fn expect_upgrade_succeeded(cluster: &TestCluster) {
        monitor_version_change(&cluster, FINISH /* expected proto version */).await;
    }

    async fn get_framework_upgrade_effects(
        cluster: &TestCluster,
    ) -> (Option<SequenceNumber>, Option<SequenceNumber>) {
        let node_handle = cluster
            .swarm
            .validators()
            .next()
            .unwrap()
            .get_node_handle()
            .unwrap();

        let effects = node_handle
            .with_async(|node| async {
                let db = node.state().db();
                let framework = db.get_object(&SUI_FRAMEWORK_OBJECT_ID);
                let digest = framework.unwrap().unwrap().previous_transaction;
                let effects = db.get_executed_effects(&digest);
                effects.unwrap().unwrap()
            })
            .await;

        let modified_at = effects
            .modified_at_versions()
            .iter()
            .find_map(|(id, v)| (id == &SUI_FRAMEWORK_OBJECT_ID).then_some(*v));

        let mutated_to = effects
            .mutated()
            .iter()
            .find_map(|((id, v, _), _)| (id == &SUI_FRAMEWORK_OBJECT_ID).then_some(*v));

        (modified_at, mutated_to)
    }

    #[sim_test]
    async fn test_framework_compatible_upgrade_no_protocol_version() {
        ProtocolConfig::poison_get_for_min_version();

        // Even though a new framework is available, the required new protocol version is not.
        sui_framework_injection::set_override(sui_framework("compatible"));
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_objects([sui_framework_object("base")])
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, START,
            ))
            .build()
            .await
            .unwrap();

        expect_upgrade_failed(&test_cluster).await;
    }

    #[sim_test]
    async fn test_framework_upgrade_conflicting_versions() {
        ProtocolConfig::poison_get_for_min_version();
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            config
        });

        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
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

        expect_upgrade_succeeded(&test_cluster).await;

        // expect_upgrade_succeeded only waits for fullnode to reconfigure - validator can actually be
        // slower than fullnode if it wasn't one of the signers of the final checkpoint.
        sleep(Duration::from_secs(3)).await;

        let node_handle = first.get_node_handle().expect("node should be running");
        // The dissenting node receives the correct framework via state sync and completes the upgrade
        node_handle.with(|node| {
            let committee = node.state().epoch_store_for_testing().committee().clone();
            assert_eq!(
                node.state().epoch_store_for_testing().protocol_version(),
                ProtocolVersion::new(FINISH)
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
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
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

        expect_upgrade_failed(&test_cluster).await;
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
                    0 => assert_eq!(protocol_version, ProtocolVersion::new(START)),
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
