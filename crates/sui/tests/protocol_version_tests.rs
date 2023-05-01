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
    let latest_version = ProtocolVersion::MAX;
    let network_config = sui_config::builder::ConfigBuilder::new(&dir)
        .with_protocol_version(ProtocolVersion::new(latest_version.as_u64() + 1))
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
            latest_version.as_u64(),
            latest_version.as_u64(),
        ))
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
    use sui_core::authority::framework_injection;
    use sui_framework::BuiltInFramework;
    use sui_json_rpc::api::WriteApiClient;
    use sui_macros::*;
    use sui_move_build::{BuildConfig, CompiledPackage};
    use sui_protocol_config::SupportedProtocolVersions;
    use sui_types::base_types::ObjectID;
    use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
    use sui_types::id::ID;
    use sui_types::messages::{Command, ProgrammableMoveCall};
    use sui_types::object::Owner;
    use sui_types::sui_system_state::{
        epoch_start_sui_system_state::EpochStartSystemStateTrait, get_validator_from_table,
        SuiSystemState, SuiSystemStateTrait, SUI_SYSTEM_STATE_SIM_TEST_DEEP_V2,
        SUI_SYSTEM_STATE_SIM_TEST_SHALLOW_V2, SUI_SYSTEM_STATE_SIM_TEST_V1,
    };
    use sui_types::{
        base_types::SequenceNumber, digests::TransactionDigest, messages::TransactionKind,
        object::Object, programmable_transaction_builder::ProgrammableTransactionBuilder,
        storage::ObjectStore, MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_OBJECT_ID,
    };
    use test_utils::network::{TestCluster, TestClusterBuilder};
    use tokio::time::{sleep, Duration};
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

        assert_eq!(
            test_cluster
                .wait_for_epoch(Some(1))
                .await
                .protocol_version(),
            FINISH
        );
        test_cluster.start_validator(validator).await;

        test_cluster.wait_for_epoch(Some(2)).await;
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
                    SupportedProtocolVersions::new_for_testing(START, START)
                } else {
                    SupportedProtocolVersions::new_for_testing(START, FINISH)
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
        monitor_version_change(&test_cluster, FINISH /* expected proto version */).await;
    }

    #[sim_test]
    async fn test_protocol_version_upgrade_no_override_cleared() {
        ProtocolConfig::poison_get_for_min_version();
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(7500);
            config
        });

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

        // default buffer stake is in effect, we do not advance to version FINISH.
        monitor_version_change(&test_cluster, START /* expected proto version */).await;
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

        let (modified_at, mutated_to) = get_framework_upgrade_versions(&cluster).await;
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
    async fn test_framework_add_struct_ability() {
        // Upgrade attempts to add an ability to a struct
        let cluster = run_framework_upgrade("base", "add_struct_ability").await;
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

    #[sim_test]
    async fn test_new_framework_package() {
        ProtocolConfig::poison_get_for_min_version();

        let sui_extra = ObjectID::from_single_byte(0x42);
        framework_injection::set_override(sui_extra, fixture_modules("extra_package"));

        let cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .build()
            .await
            .unwrap();

        expect_upgrade_succeeded(&cluster).await;

        // Make sure the epoch change event includes the event from the new package's module
        // initializer
        let effects = get_framework_upgrade_effects(&cluster, &sui_extra).await;

        let shared_id = effects
            .created()
            .iter()
            .find_map(|(obj, owner)| {
                if let Owner::Shared { .. } = owner {
                    Some(obj.0)
                } else {
                    None
                }
            })
            .unwrap();

        let shared = get_object(&cluster, &shared_id).await;
        let type_ = shared.type_().unwrap();
        assert_eq!(type_.module().as_str(), "msim_extra_1");
        assert_eq!(type_.name().as_str(), "S");

        // Call a function from the newly published system package
        assert_eq!(
            dev_inspect_call(
                &cluster,
                ProgrammableMoveCall {
                    package: sui_extra,
                    module: ident_str!("msim_extra_1").to_owned(),
                    function: ident_str!("canary").to_owned(),
                    type_arguments: vec![],
                    arguments: vec![],
                }
            )
            .await,
            43,
        );
    }

    async fn run_framework_upgrade(from: &str, to: &str) -> TestCluster {
        ProtocolConfig::poison_get_for_min_version();

        override_sui_system_modules(to);
        TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_objects([sui_system_package_object(from)])
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .build()
            .await
            .unwrap()
    }

    async fn call_canary(cluster: &TestCluster) -> u64 {
        dev_inspect_call(
            cluster,
            ProgrammableMoveCall {
                package: SUI_SYSTEM_OBJECT_ID,
                module: ident_str!("msim_extra_1").to_owned(),
                function: ident_str!("canary").to_owned(),
                type_arguments: vec![],
                arguments: vec![],
            },
        )
        .await
    }

    async fn dev_inspect_call(cluster: &TestCluster, call: ProgrammableMoveCall) -> u64 {
        let client = cluster.rpc_client();
        let sender = cluster.accounts.first().cloned().unwrap();

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.command(Command::MoveCall(Box::new(call)));
            builder.finish()
        };
        let txn = TransactionKind::programmable(pt);

        let response = client
            .dev_inspect_transaction_block(
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

    async fn get_framework_upgrade_versions(
        cluster: &TestCluster,
    ) -> (Option<SequenceNumber>, Option<SequenceNumber>) {
        let effects = get_framework_upgrade_effects(cluster, &SUI_SYSTEM_OBJECT_ID).await;

        let modified_at = effects
            .modified_at_versions()
            .iter()
            .find_map(|(id, v)| (id == &SUI_SYSTEM_OBJECT_ID).then_some(*v));

        let mutated_to = effects
            .mutated()
            .iter()
            .find_map(|((id, v, _), _)| (id == &SUI_SYSTEM_OBJECT_ID).then_some(*v));

        (modified_at, mutated_to)
    }

    async fn get_framework_upgrade_effects(
        cluster: &TestCluster,
        package: &ObjectID,
    ) -> TransactionEffects {
        let node_handle = cluster
            .swarm
            .validators()
            .next()
            .unwrap()
            .get_node_handle()
            .unwrap();

        node_handle
            .with_async(|node| async {
                let db = node.state().db();
                let framework = db.get_object(package);
                let digest = framework.unwrap().unwrap().previous_transaction;
                let effects = db.get_executed_effects(&digest);
                effects.unwrap().unwrap()
            })
            .await
    }

    async fn get_object(cluster: &TestCluster, package: &ObjectID) -> Object {
        let node_handle = cluster
            .swarm
            .validators()
            .next()
            .unwrap()
            .get_node_handle()
            .unwrap();

        node_handle
            .with_async(|node| async { node.state().db().get_object(package).unwrap().unwrap() })
            .await
    }

    #[sim_test]
    async fn test_framework_compatible_upgrade_no_protocol_version() {
        ProtocolConfig::poison_get_for_min_version();

        // Even though a new framework is available, the required new protocol version is not.
        override_sui_system_modules("compatible");
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_objects([sui_system_package_object("base")])
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
        override_sui_system_modules_cb(Box::new(move |name| {
            if name == first_name {
                info!("node {:?} using compatible packages", name.concise());
                Some(sui_system_modules("base"))
            } else {
                Some(sui_system_modules("compatible"))
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
        override_sui_system_modules_cb(Box::new(move |name| {
            if name == first || name == second {
                Some(sui_system_modules("compatible"))
            } else {
                None
            }
        }));

        expect_upgrade_failed(&test_cluster).await;
    }

    #[sim_test]
    async fn test_safe_mode_recovery() {
        override_sui_system_modules("mock_sui_systems/base");
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            // Overrides with a sui system package that would abort during epoch change txn
            .with_objects([sui_system_package_object("mock_sui_systems/safe_mode")])
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .build()
            .await
            .unwrap();
        let genesis_epoch_start_time = test_cluster
            .swarm
            .validators()
            .next()
            .unwrap()
            .get_node_handle()
            .unwrap()
            .with(|node| {
                node.state()
                    .epoch_store_for_testing()
                    .epoch_start_state()
                    .epoch_start_timestamp_ms()
            });

        // We are going to enter safe mode so set the expectation right.
        test_cluster.set_safe_mode_expected(true);

        // Wait for epoch change to happen. This epoch we should also experience a framework
        // upgrade that upgrades the framework to the base one (which doesn't abort), and thus
        // a protocol version increment.
        let system_state = test_cluster.wait_for_epoch(Some(1)).await;
        assert_eq!(system_state.epoch(), 1);
        assert_eq!(system_state.protocol_version(), FINISH); // protocol version increments
        assert!(system_state.safe_mode()); // enters safe mode
        assert!(system_state.epoch_start_timestamp_ms() >= genesis_epoch_start_time + 20000);

        // We are getting out of safe mode soon.
        test_cluster.set_safe_mode_expected(false);

        // This epoch change should execute successfully without any upgrade and get us out of safe mode.
        let system_state = test_cluster.wait_for_epoch(Some(2)).await;
        assert_eq!(system_state.epoch(), 2);
        assert_eq!(system_state.protocol_version(), FINISH); // protocol version stays the same
        assert!(!system_state.safe_mode()); // out of safe mode
    }

    #[sim_test]
    async fn sui_system_mock_smoke_test() {
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, START,
            ))
            .with_objects([sui_system_package_object("mock_sui_systems/base")])
            .build()
            .await
            .unwrap();
        // Make sure we can survive at least one epoch.
        test_cluster.wait_for_epoch(None).await;
    }

    #[sim_test]
    async fn sui_system_state_shallow_upgrade_test() {
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .with_objects([sui_system_package_object("mock_sui_systems/base")])
            .build()
            .await
            .unwrap();
        override_sui_system_modules("mock_sui_systems/shallow_upgrade");
        // Wait for the upgrade to finish. After the upgrade, the new framework will be installed,
        // but the system state object hasn't been upgraded yet.
        let system_state = test_cluster.wait_for_epoch(Some(1)).await;
        assert_eq!(system_state.protocol_version(), FINISH);
        assert_eq!(
            system_state.system_state_version(),
            SUI_SYSTEM_STATE_SIM_TEST_V1
        );
        assert!(matches!(system_state, SuiSystemState::SimTestV1(_)));

        // The system state object will be upgraded next time we execute advance_epoch transaction
        // at epoch boundary.
        let system_state = test_cluster.wait_for_epoch(Some(2)).await;
        assert_eq!(
            system_state.system_state_version(),
            SUI_SYSTEM_STATE_SIM_TEST_SHALLOW_V2
        );
        assert!(matches!(system_state, SuiSystemState::SimTestShallowV2(_)));
    }

    #[sim_test]
    async fn sui_system_state_deep_upgrade_test() {
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .with_objects([sui_system_package_object("mock_sui_systems/base")])
            .build()
            .await
            .unwrap();
        override_sui_system_modules("mock_sui_systems/deep_upgrade");
        // Wait for the upgrade to finish. After the upgrade, the new framework will be installed,
        // but the system state object hasn't been upgraded yet.
        let system_state = test_cluster.wait_for_epoch(Some(1)).await;
        assert_eq!(system_state.protocol_version(), FINISH);
        assert_eq!(
            system_state.system_state_version(),
            SUI_SYSTEM_STATE_SIM_TEST_V1
        );
        if let SuiSystemState::SimTestV1(inner) = system_state {
            // Make sure we have 1 inactive validator for latter testing.
            assert_eq!(inner.validators.inactive_validators.size, 1);
            get_validator_from_table(
                test_cluster.fullnode_handle.sui_node.state().db().as_ref(),
                inner.validators.inactive_validators.id,
                &ID::new(ObjectID::ZERO),
            )
            .unwrap();
        } else {
            panic!("Expecting SimTestV1 type");
        }

        // The system state object will be upgraded next time we execute advance_epoch transaction
        // at epoch boundary.
        let system_state = test_cluster.wait_for_epoch(Some(2)).await;
        assert_eq!(
            system_state.system_state_version(),
            SUI_SYSTEM_STATE_SIM_TEST_DEEP_V2
        );
        if let SuiSystemState::SimTestDeepV2(inner) = system_state {
            // Make sure we have 1 inactive validator for latter testing.
            assert_eq!(inner.validators.inactive_validators.size, 1);
            get_validator_from_table(
                test_cluster.fullnode_handle.sui_node.state().db().as_ref(),
                inner.validators.inactive_validators.id,
                &ID::new(ObjectID::ZERO),
            )
            .unwrap();
        } else {
            panic!("Expecting SimTestDeepV2 type");
        }
    }

    #[sim_test]
    async fn sui_system_state_production_upgrade_test() {
        // Use this test to test a real sui system state upgrade. To make this test work,
        // put the new sui system in a new path and point to it in the override.
        // It's important to also handle the new protocol version in protocol-config/lib.rs.
        // The MAX_PROTOCOL_VERSION must not be changed yet when testing this.
        let test_cluster = TestClusterBuilder::new()
            .with_epoch_duration_ms(20000)
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                START, FINISH,
            ))
            .build()
            .await
            .unwrap();
        // TODO: Replace the path with the new framework path when we test it for real.
        override_sui_system_modules("../../../sui-framework/packages/sui-system");
        // Wait for the upgrade to finish. After the upgrade, the new framework will be installed,
        // but the system state object hasn't been upgraded yet.
        let system_state = test_cluster.wait_for_epoch(Some(1)).await;
        assert_eq!(system_state.protocol_version(), FINISH);

        // The system state object will be upgraded next time we execute advance_epoch transaction
        // at epoch boundary.
        let system_state = test_cluster.wait_for_epoch(Some(2)).await;
        if let SuiSystemState::V2(inner) = system_state {
            assert_eq!(inner.parameters.min_validator_count, 4);
        } else {
            unreachable!("Unexpected sui system state version");
        }
    }

    async fn monitor_version_change(test_cluster: &TestCluster, final_version: u64) {
        let system_state = test_cluster.wait_for_epoch(Some(1)).await;
        assert_eq!(system_state.protocol_version(), final_version);
        // End this at the end of epoch 2 since tests expect so.
        test_cluster.wait_for_epoch(Some(2)).await;
    }

    fn override_sui_system_modules(path: &str) {
        framework_injection::set_override(SUI_SYSTEM_OBJECT_ID, sui_system_modules(path));
    }

    fn override_sui_system_modules_cb(f: framework_injection::PackageUpgradeCallback) {
        framework_injection::set_override_cb(SUI_SYSTEM_OBJECT_ID, f)
    }

    /// Get compiled modules for Sui System, built from fixture `fixture` in the
    /// `framework_upgrades` directory.
    fn sui_system_modules(fixture: &str) -> Vec<CompiledModule> {
        fixture_package(fixture)
            .get_sui_system_modules()
            .cloned()
            .collect()
    }

    /// Like `sui_system_modules`, but package the modules in an `Object`.
    fn sui_system_package_object(fixture: &str) -> Object {
        Object::new_package(
            &sui_system_modules(fixture),
            TransactionDigest::genesis(),
            u64::MAX,
            &[
                BuiltInFramework::get_package_by_id(&MOVE_STDLIB_OBJECT_ID).genesis_move_package(),
                BuiltInFramework::get_package_by_id(&SUI_FRAMEWORK_OBJECT_ID)
                    .genesis_move_package(),
            ],
        )
        .unwrap()
    }

    /// Get root compiled modules, built from fixture `fixture` in the `framework_upgrades`
    /// directory.
    fn fixture_modules(fixture: &str) -> Vec<CompiledModule> {
        fixture_package(fixture).into_modules()
    }

    fn fixture_package(fixture: &str) -> CompiledPackage {
        let mut package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        package.extend(["tests", "framework_upgrades", fixture]);

        let mut config = BuildConfig::new_for_testing();
        config.run_bytecode_verifier = true;
        config.build(package).unwrap()
    }
}
