// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use rand::rngs::OsRng;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use std::time::Duration;
use sui_core::consensus_adapter::position_submit_certificate;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_protocol_config::ProtocolConfig;
use sui_swarm_config::genesis_config::{ValidatorGenesisConfig, ValidatorGenesisConfigBuilder};
use sui_test_transaction_builder::{make_transfer_sui_transaction, TestTransactionBuilder};
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiError;
use sui_types::gas::GasCostSummary;
use sui_types::governance::MIN_VALIDATOR_JOINING_STAKE_MIST;
use sui_types::message_envelope::Message;
use sui_types::sui_system_state::{
    get_validator_from_table, sui_system_state_summary::get_validator_by_pool_id,
    SuiSystemStateTrait,
};
use sui_types::transaction::{TransactionDataAPI, TransactionExpiration, VerifiedTransaction};
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::time::sleep;

#[sim_test]
async fn advance_epoch_tx_test() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let states = test_cluster
        .swarm
        .validator_node_handles()
        .into_iter()
        .map(|handle| handle.with(|node| node.state()))
        .collect::<Vec<_>>();
    let tasks: Vec<_> = states
        .iter()
        .map(|state| async {
            let (_system_state, effects) = state
                .create_and_execute_advance_epoch_tx(
                    &state.epoch_store_for_testing(),
                    &GasCostSummary::new(0, 0, 0, 0),
                    0, // checkpoint
                    0, // epoch_start_timestamp_ms
                )
                .await
                .unwrap();
            // Check that the validator didn't commit the transaction yet.
            assert!(state
                .get_signed_effects_and_maybe_resign(
                    effects.transaction_digest(),
                    &state.epoch_store_for_testing()
                )
                .unwrap()
                .is_none());
            effects
        })
        .collect();
    let results: HashSet<_> = join_all(tasks)
        .await
        .into_iter()
        .map(|result| result.digest())
        .collect();
    // Check that all validators have the same result.
    assert_eq!(results.len(), 1);
}

#[sim_test]
async fn basic_reconfig_end_to_end_test() {
    // TODO remove this sleep when this test passes consistently
    sleep(Duration::from_secs(1)).await;
    let test_cluster = TestClusterBuilder::new().build().await;
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_transaction_expiration() {
    let test_cluster = TestClusterBuilder::new().build().await;
    test_cluster.trigger_reconfiguration().await;

    let (sender, gas) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;
    let mut data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(1), sender)
        .build();
    // Expired transaction returns an error
    let mut expired_data = data.clone();
    *expired_data.expiration_mut_for_testing() = TransactionExpiration::Epoch(0);
    let expired_transaction = test_cluster.wallet.sign_transaction(&expired_data);
    let result = test_cluster
        .wallet
        .execute_transaction_may_fail(expired_transaction)
        .await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(&SuiError::TransactionExpired.to_string()));

    // Non expired transaction signed without issue
    *data.expiration_mut_for_testing() = TransactionExpiration::Epoch(10);
    let transaction = test_cluster.wallet.sign_transaction(&data);
    test_cluster
        .wallet
        .execute_transaction_may_fail(transaction)
        .await
        .unwrap();
}

// TODO: This test does not guarantee that tx would be reverted, and hence the code path
// may not always be tested.
#[sim_test]
async fn reconfig_with_revert_end_to_end_test() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let authorities = test_cluster.swarm.validator_node_handles();
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, mut gas_objects) = test_cluster.wallet.get_one_account().await.unwrap();

    // gas1 transaction is committed
    let gas1 = gas_objects.pop().unwrap();
    let tx = test_cluster.wallet.sign_transaction(
        &TestTransactionBuilder::new(sender, gas1, rgp)
            .transfer_sui(None, sender)
            .build(),
    );
    let effects1 = test_cluster.execute_transaction(tx).await;
    assert_eq!(0, effects1.effects.unwrap().executed_epoch());

    // gas2 transaction is (most likely) reverted
    let gas2 = gas_objects.pop().unwrap();
    let tx = test_cluster.wallet.sign_transaction(
        &TestTransactionBuilder::new(sender, gas2, rgp)
            .transfer_sui(None, sender)
            .build(),
    );
    let net = test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.clone_authority_aggregator().unwrap());
    let cert = net
        .process_transaction(tx.clone(), None)
        .await
        .unwrap()
        .into_cert_for_testing();

    // Close epoch on 3 (2f+1) validators.
    let mut reverting_authority_idx = None;
    for (i, handle) in authorities.iter().enumerate() {
        handle
            .with_async(|node| async {
                if position_submit_certificate(&net.committee, &node.state().name, tx.digest())
                    < (authorities.len() - 1)
                {
                    node.close_epoch_for_testing().await.unwrap();
                } else {
                    // remember the authority that wouild submit it to consensus last.
                    reverting_authority_idx = Some(i);
                }
            })
            .await;
    }

    let reverting_authority_idx = reverting_authority_idx.unwrap();
    let client = net
        .get_client(&authorities[reverting_authority_idx].with(|node| node.state().name))
        .unwrap();
    client
        .handle_certificate_v2(cert.clone(), None)
        .await
        .unwrap();

    authorities[reverting_authority_idx]
        .with_async(|node| async {
            let object = node
                .state()
                .get_objects(&[gas2.0])
                .await
                .into_iter()
                .next()
                .unwrap()
                .unwrap();
            // verify that authority 0 advanced object version
            assert_eq!(2, object.version().value());
        })
        .await;

    // Wait for all nodes to reach the next epoch.
    let handles: Vec<_> = authorities
        .iter()
        .map(|handle| {
            handle.with_async(|node| async {
                loop {
                    if node.state().current_epoch_for_testing() == 1 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            })
        })
        .collect();
    join_all(handles).await;

    let mut epoch = None;
    for handle in authorities.iter() {
        handle
            .with_async(|node| async {
                let object = node
                    .state()
                    .get_objects(&[gas1.0])
                    .await
                    .into_iter()
                    .next()
                    .unwrap()
                    .unwrap();
                assert_eq!(2, object.version().value());
                // Due to race conditions, it's possible that tx2 went in
                // before 2f+1 validators sent EndOfPublish messages and close
                // the curtain of epoch 0. So, we are asserting that
                // the object version is either 1 or 2, but needs to be
                // consistent in all validators.
                // Note that previously test checked that object version == 2 on authority 0
                let object = node
                    .state()
                    .get_objects(&[gas2.0])
                    .await
                    .into_iter()
                    .next()
                    .unwrap()
                    .unwrap();
                let object_version = object.version().value();
                if epoch.is_none() {
                    assert!(object_version == 1 || object_version == 2);
                    epoch.replace(object_version);
                } else {
                    assert_eq!(epoch, Some(object_version));
                }
            })
            .await;
    }
}

// This test just starts up a cluster that reconfigures itself under 0 load.
#[sim_test]
async fn test_passive_reconfig() {
    do_test_passive_reconfig().await;
}

#[sim_test(check_determinism)]
async fn test_passive_reconfig_determinism() {
    do_test_passive_reconfig().await;
}

async fn do_test_passive_reconfig() {
    telemetry_subscribers::init_for_testing();
    let _commit_root_state_digest = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_commit_root_state_digest_supported_for_testing(true);
        config
    });
    ProtocolConfig::poison_get_for_min_version();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(1000)
        .build()
        .await;

    let target_epoch: u64 = std::env::var("RECONFIG_TARGET_EPOCH")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(4);

    test_cluster.wait_for_epoch(Some(target_epoch)).await;

    test_cluster
        .swarm
        .validator_nodes()
        .next()
        .unwrap()
        .get_node_handle()
        .unwrap()
        .with(|node| {
            let commitments = node
                .state()
                .get_epoch_state_commitments(0)
                .unwrap()
                .unwrap();
            assert_eq!(commitments.len(), 1);
        });
}

// Test that transaction locks from previously epochs could be overridden.
#[sim_test]
async fn test_expired_locks() {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .build()
        .await;

    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let sender = accounts_and_objs[0].0;
    let receiver = accounts_and_objs[1].0;
    let gas_object = accounts_and_objs[0].1[0];

    let transfer_sui = |amount| {
        test_cluster.wallet.sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .transfer_sui(Some(amount), receiver)
                .build(),
        )
    };

    let t1 = transfer_sui(1);
    // attempt to equivocate
    let t2 = transfer_sui(2);

    for (idx, validator) in test_cluster.all_validator_handles().into_iter().enumerate() {
        let state = validator.state();
        let epoch_store = state.epoch_store_for_testing();
        let t = if idx % 2 == 0 { t1.clone() } else { t2.clone() };
        validator
            .state()
            .handle_transaction(&epoch_store, VerifiedTransaction::new_unchecked(t))
            .await
            .unwrap();
    }
    test_cluster
        .create_certificate(t1.clone(), None)
        .await
        .unwrap_err();

    test_cluster
        .create_certificate(t2.clone(), None)
        .await
        .unwrap_err();

    test_cluster.wait_for_epoch_all_nodes(1).await;

    // old locks can be overridden in new epoch
    test_cluster
        .create_certificate(t2.clone(), None)
        .await
        .unwrap();

    // attempt to equivocate
    test_cluster
        .create_certificate(t1.clone(), None)
        .await
        .unwrap_err();
}

// This test just starts up a cluster that reconfigures itself under 0 load.
#[cfg(msim)]
#[sim_test]
async fn test_create_advance_epoch_tx_race() {
    use std::sync::Arc;
    use sui_macros::{register_fail_point, register_fail_point_async};
    use tokio::sync::broadcast;
    use tracing::info;

    telemetry_subscribers::init_for_testing();
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    // panic if we enter safe mode. If you remove the check for `is_tx_already_executed` in
    // AuthorityState::create_and_execute_advance_epoch_tx, this test should fail.
    register_fail_point("record_checkpoint_builder_is_safe_mode_metric", || {
        panic!("safe mode recorded");
    });

    // Intercept the specified async wait point on a given node, and wait there until a message
    // is sent from the given tx.
    let register_wait = |failpoint, node_id, tx: Arc<broadcast::Sender<()>>| {
        let node = sui_simulator::task::NodeId(node_id);
        register_fail_point_async(failpoint, move || {
            let cur_node = sui_simulator::current_simnode_id();
            let tx = tx.clone();
            async move {
                if cur_node == node {
                    let mut rx = tx.subscribe();

                    info!(
                        "waiting for test to send continuation signal for {}",
                        failpoint
                    );
                    rx.recv().await.unwrap();
                    info!("continuing {}", failpoint);
                }
            }
        });
    };

    // Set up wait points.
    let (change_epoch_delay_tx, _change_epoch_delay_rx) = broadcast::channel(1);
    let change_epoch_delay_tx = Arc::new(change_epoch_delay_tx);
    let (reconfig_delay_tx, _reconfig_delay_rx) = broadcast::channel(1);
    let reconfig_delay_tx = Arc::new(reconfig_delay_tx);

    // Test code runs in node 1 - node 2 is always a validator.
    let target_node = 2;
    register_wait(
        "change_epoch_tx_delay",
        target_node,
        change_epoch_delay_tx.clone(),
    );
    register_wait("reconfig_delay", target_node, reconfig_delay_tx.clone());

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(1000)
        .build()
        .await;

    test_cluster.wait_for_epoch(None).await;

    // Allow time for paused node to execute change epoch tx via state sync.
    sleep(Duration::from_secs(5)).await;

    // now release the pause, node will find that change epoch tx has already been executed.
    info!("releasing change epoch delay tx");
    change_epoch_delay_tx.send(()).unwrap();

    // proceeded with reconfiguration.
    sleep(Duration::from_secs(1)).await;
    reconfig_delay_tx.send(()).unwrap();
}

#[sim_test]
async fn test_reconfig_with_failing_validator() {
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    let test_cluster = Arc::new(
        TestClusterBuilder::new()
            .with_epoch_duration_ms(5000)
            .build()
            .await,
    );

    test_cluster
        .random_node_restarter()
        .with_kill_interval_secs(2, 4)
        .with_restart_delay_secs(2, 4)
        .run();

    let target_epoch: u64 = std::env::var("RECONFIG_TARGET_EPOCH")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(4);

    // A longer timeout is required, as restarts can cause reconfiguration to take longer.
    test_cluster
        .wait_for_epoch_with_timeout(Some(target_epoch), Duration::from_secs(90))
        .await;
}

#[sim_test]
async fn test_validator_resign_effects() {
    // This test checks that validators are able to re-sign transaction effects that were finalized
    // in previous epochs. This allows authority aggregator to form a new effects certificate
    // in the new epoch.
    let test_cluster = TestClusterBuilder::new().build().await;
    let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
    let effects0 = test_cluster
        .execute_transaction(tx.clone())
        .await
        .effects
        .unwrap();
    assert_eq!(effects0.executed_epoch(), 0);
    test_cluster.trigger_reconfiguration().await;

    let net = test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.clone_authority_aggregator().unwrap());
    let effects1 = net
        .process_transaction(tx, None)
        .await
        .unwrap()
        .into_effects_for_testing();
    // Ensure that we are able to form a new effects cert in the new epoch.
    assert_eq!(effects1.epoch(), 1);
    assert_eq!(effects1.executed_epoch(), 0);
}

#[sim_test]
async fn test_validator_candidate_pool_read() {
    let new_validator = ValidatorGenesisConfigBuilder::new().build(&mut OsRng);
    let address: SuiAddress = (&new_validator.account_key_pair.public()).into();
    let test_cluster = TestClusterBuilder::new()
        .with_validator_candidates([address])
        .build()
        .await;
    add_validator_candidate(&test_cluster, &new_validator).await;
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let system_state_summary = system_state.clone().into_sui_system_state_summary();
        let staking_pool_id = get_validator_from_table(
            node.state().get_object_store().as_ref(),
            system_state_summary.validator_candidates_id,
            &address,
        )
        .unwrap()
        .staking_pool_id;
        let validator = get_validator_by_pool_id(
            node.state().get_object_store().as_ref(),
            &system_state,
            &system_state_summary,
            staking_pool_id,
        )
        .unwrap();
        assert_eq!(validator.sui_address, address);
    });
}

#[sim_test]
async fn test_inactive_validator_pool_read() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(5)
        .build()
        .await;
    // Pick the first validator.
    let validator = test_cluster.swarm.validator_node_handles().pop().unwrap();
    let address = validator.with(|node| node.get_config().sui_address());
    let staking_pool_id = test_cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_sui_system_state_object_for_testing()
            .unwrap()
            .into_sui_system_state_summary()
            .active_validators
            .iter()
            .find(|v| v.sui_address == address)
            .unwrap()
            .staking_pool_id
    });
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let system_state_summary = system_state.clone().into_sui_system_state_summary();
        // Validator is active. Check that we can find its summary by staking pool id.
        let validator = get_validator_by_pool_id(
            node.state().get_object_store().as_ref(),
            &system_state,
            &system_state_summary,
            staking_pool_id,
        )
        .unwrap();
        assert_eq!(validator.sui_address, address);
    });
    execute_remove_validator_tx(&test_cluster, &validator).await;

    test_cluster.trigger_reconfiguration().await;

    // Check that this node is no longer a validator.
    validator.with(|node| {
        assert!(node
            .state()
            .is_fullnode(&node.state().epoch_store_for_testing()));
    });

    // Check that the validator that just left now shows up in the inactive_validators,
    // and we can still deserialize it and get the inactive staking pool.
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        assert_eq!(
            system_state
                .get_current_epoch_committee()
                .committee()
                .num_members(),
            4
        );
        let system_state_summary = system_state.clone().into_sui_system_state_summary();
        let validator = get_validator_by_pool_id(
            node.state().get_object_store().as_ref(),
            &system_state,
            &system_state_summary,
            staking_pool_id,
        )
        .unwrap();
        assert_eq!(validator.sui_address, address);
        assert!(validator.staking_pool_deactivation_epoch.is_some());
    })
}

#[sim_test]
async fn test_reconfig_with_committee_change_basic() {
    // This test exercise the full flow of a validator joining the network, catch up and then leave.

    let new_validator = ValidatorGenesisConfigBuilder::new().build(&mut OsRng);
    let address = (&new_validator.account_key_pair.public()).into();
    let mut test_cluster = TestClusterBuilder::new()
        .with_validator_candidates([address])
        .build()
        .await;

    execute_add_validator_transactions(&test_cluster, &new_validator).await;

    test_cluster.trigger_reconfiguration().await;

    // Check that a new validator has joined the committee.
    test_cluster.fullnode_handle.sui_node.with(|node| {
        assert_eq!(
            node.state()
                .epoch_store_for_testing()
                .committee()
                .num_members(),
            5
        );
    });
    let new_validator_handle = test_cluster.spawn_new_validator(new_validator).await;
    test_cluster.wait_for_epoch_all_nodes(1).await;

    new_validator_handle.with(|node| {
        assert!(node
            .state()
            .is_validator(&node.state().epoch_store_for_testing()));
    });

    execute_remove_validator_tx(&test_cluster, &new_validator_handle).await;
    test_cluster.trigger_reconfiguration().await;
    test_cluster.fullnode_handle.sui_node.with(|node| {
        assert_eq!(
            node.state()
                .epoch_store_for_testing()
                .committee()
                .num_members(),
            4
        );
    });
}

#[sim_test]
async fn test_reconfig_with_committee_change_stress() {
    do_test_reconfig_with_committee_change_stress().await;
}

#[sim_test(check_determinism)]
async fn test_reconfig_with_committee_change_stress_determinism() {
    do_test_reconfig_with_committee_change_stress().await;
}

async fn do_test_reconfig_with_committee_change_stress() {
    let mut candidates = (0..6)
        .map(|_| ValidatorGenesisConfigBuilder::new().build(&mut OsRng))
        .collect::<Vec<_>>();
    let addresses = candidates
        .iter()
        .map(|c| (&c.account_key_pair.public()).into())
        .collect::<Vec<SuiAddress>>();
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(7)
        .with_validator_candidates(addresses)
        .with_num_unpruned_validators(2)
        .build()
        .await;

    let mut cur_epoch = 0;

    while let Some(v1) = candidates.pop() {
        let v2 = candidates.pop().unwrap();
        execute_add_validator_transactions(&test_cluster, &v1).await;
        execute_add_validator_transactions(&test_cluster, &v2).await;
        let mut removed_validators = vec![];
        for v in test_cluster
            .swarm
            .active_validators()
            // Skip removal of any non-pruning validators from the committee.
            // Until we have archival solution, we need to have some validators that do not prune,
            // otherwise new validators to the committee will not be able to catch up to the network
            // TODO: remove and replace with usage of archival solution
            .filter(|node| {
                node.config()
                    .authority_store_pruning_config
                    .num_epochs_to_retain_for_checkpoints()
                    .is_some()
            })
            .take(2)
        {
            let h = v.get_node_handle().unwrap();
            removed_validators.push(h.state().name);
            execute_remove_validator_tx(&test_cluster, &h).await;
        }
        let handle1 = test_cluster.spawn_new_validator(v1).await;
        let handle2 = test_cluster.spawn_new_validator(v2).await;

        tokio::join!(
            test_cluster.wait_for_epoch_on_node(&handle1, Some(cur_epoch), Duration::from_secs(60)),
            test_cluster.wait_for_epoch_on_node(&handle2, Some(cur_epoch), Duration::from_secs(60))
        );

        test_cluster.trigger_reconfiguration().await;
        let committee = test_cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().epoch_store_for_testing().committee().clone());
        cur_epoch = committee.epoch();
        assert_eq!(committee.num_members(), 7);
        assert!(committee.authority_exists(&handle1.state().name));
        assert!(committee.authority_exists(&handle2.state().name));
        removed_validators
            .iter()
            .all(|v| !committee.authority_exists(v));
    }
}

#[cfg(msim)]
#[sim_test]
async fn test_epoch_flag_upgrade() {
    use std::sync::Mutex;
    use sui_core::authority::epoch_start_configuration::EpochFlag;
    use sui_core::authority::epoch_start_configuration::EpochStartConfigTrait;
    use sui_macros::register_fail_point_arg;

    let initial_flags_nodes = Arc::new(Mutex::new(HashSet::new()));
    register_fail_point_arg("initial_epoch_flags", move || {
        // only alter flags on each node once
        let current_node = sui_simulator::current_simnode_id();

        // override flags on up to 2 nodes.
        let mut initial_flags_nodes = initial_flags_nodes.lock().unwrap();
        if initial_flags_nodes.len() >= 2 || !initial_flags_nodes.insert(current_node) {
            return None;
        }

        // start with only UseVersionAssignmentTablesV3
        let flags: Vec<EpochFlag> = vec![EpochFlag::UseVersionAssignmentTablesV3];
        Some(flags)
    });

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(30000)
        .build()
        .await;

    let mut all_flags = vec![];
    for node in test_cluster.all_node_handles() {
        all_flags.push(node.with(|node| {
            node.state()
                .epoch_store_for_testing()
                .epoch_start_config()
                .flags()
                .to_vec()
        }));
    }
    all_flags.iter_mut().for_each(|flags| flags.sort());
    all_flags.sort();
    all_flags.dedup();
    assert_eq!(
        all_flags.len(),
        2,
        "expected 2 different sets of flags: {:?}",
        all_flags
    );

    test_cluster.wait_for_epoch_all_nodes(1).await;

    let mut any_empty = false;
    for node in test_cluster.all_node_handles() {
        any_empty = any_empty
            || node.with(|node| {
                node.state()
                    .epoch_store_for_testing()
                    .epoch_start_config()
                    .flags()
                    .is_empty()
            });
    }
    assert!(!any_empty);

    sleep(Duration::from_secs(15)).await;

    test_cluster.stop_all_validators().await;
    test_cluster.start_all_validators().await;

    test_cluster.wait_for_epoch_all_nodes(2).await;
}

#[cfg(msim)]
#[sim_test]
async fn safe_mode_reconfig_test() {
    use sui_test_transaction_builder::make_staking_transaction;
    use sui_types::sui_system_state::advance_epoch_result_injection;

    const EPOCH_DURATION: u64 = 10000;

    // Inject failure at epoch change 1 -> 2.
    advance_epoch_result_injection::set_override(Some((2, 3)));

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(EPOCH_DURATION)
        .build()
        .await;

    let system_state = test_cluster
        .sui_client()
        .governance_api()
        .get_latest_sui_system_state()
        .await
        .unwrap();

    // On startup, we should be at V1.
    assert_eq!(system_state.system_state_version, 1);
    assert_eq!(system_state.epoch, 0);

    // Wait for regular epoch change to happen once. Migration from V1 to V2 should happen here.
    let system_state = test_cluster.wait_for_epoch(Some(1)).await;
    assert!(!system_state.safe_mode());
    assert_eq!(system_state.epoch(), 1);
    assert_eq!(system_state.system_state_version(), 2);

    let prev_epoch_start_timestamp = system_state.epoch_start_timestamp_ms();

    // We are going to enter safe mode so set the expectation right.
    test_cluster.set_safe_mode_expected(true);

    // Reconfig again and check that we are in safe mode now.
    let system_state = test_cluster.wait_for_epoch(Some(2)).await;
    assert!(system_state.safe_mode());
    assert_eq!(system_state.epoch(), 2);
    // Check that time is properly set even in safe mode.
    assert!(system_state.epoch_start_timestamp_ms() >= prev_epoch_start_timestamp + EPOCH_DURATION);

    // Try a staking transaction.
    let validator_address = system_state
        .into_sui_system_state_summary()
        .active_validators[0]
        .sui_address;
    let txn = make_staking_transaction(&test_cluster.wallet, validator_address).await;
    test_cluster.execute_transaction(txn).await;

    // Now remove the override and check that in the next epoch we are no longer in safe mode.
    test_cluster.set_safe_mode_expected(false);

    let system_state = test_cluster.wait_for_epoch(Some(3)).await;
    assert!(!system_state.safe_mode());
    assert_eq!(system_state.epoch(), 3);
    assert_eq!(system_state.system_state_version(), 2);
}

async fn add_validator_candidate(
    test_cluster: &TestCluster,
    new_validator: &ValidatorGenesisConfig,
) {
    let cur_validator_candidate_count = test_cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_sui_system_state_object_for_testing()
            .unwrap()
            .into_sui_system_state_summary()
            .validator_candidates_size
    });
    let address = (&new_validator.account_key_pair.public()).into();
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let tx =
        TestTransactionBuilder::new(address, gas, test_cluster.get_reference_gas_price().await)
            .call_request_add_validator_candidate(
                &new_validator.to_validator_info_with_random_name().into(),
            )
            .build_and_sign(&new_validator.account_key_pair);
    test_cluster.execute_transaction(tx).await;

    // Check that the candidate can be found in the candidate table now.
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let system_state_summary = system_state.into_sui_system_state_summary();
        assert_eq!(
            system_state_summary.validator_candidates_size,
            cur_validator_candidate_count + 1
        );
    });
}

async fn execute_remove_validator_tx(test_cluster: &TestCluster, handle: &SuiNodeHandle) {
    let address = handle.with(|node| node.get_config().sui_address());
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let rgp = test_cluster.get_reference_gas_price().await;
    let tx = handle.with(|node| {
        TestTransactionBuilder::new(address, gas, rgp)
            .call_request_remove_validator()
            .build_and_sign(node.get_config().account_key_pair.keypair())
    });
    test_cluster.execute_transaction(tx).await;
}

/// Execute a sequence of transactions to add a validator, including adding candidate, adding stake
/// and activate the validator.
/// It does not however trigger reconfiguration yet.
async fn execute_add_validator_transactions(
    test_cluster: &TestCluster,
    new_validator: &ValidatorGenesisConfig,
) {
    let pending_active_count = test_cluster.fullnode_handle.sui_node.with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        system_state
            .get_pending_active_validators(node.state().get_object_store().as_ref())
            .unwrap()
            .len()
    });
    add_validator_candidate(test_cluster, new_validator).await;

    let address = (&new_validator.account_key_pair.public()).into();
    let stake_coin = test_cluster
        .wallet
        .gas_for_owner_budget(
            address,
            MIN_VALIDATOR_JOINING_STAKE_MIST,
            Default::default(),
        )
        .await
        .unwrap()
        .1
        .object_ref();
    let gas = test_cluster
        .wallet
        .gas_for_owner_budget(address, 0, BTreeSet::from([stake_coin.0]))
        .await
        .unwrap()
        .1
        .object_ref();

    let rgp = test_cluster.get_reference_gas_price().await;
    let stake_tx = TestTransactionBuilder::new(address, gas, rgp)
        .call_staking(stake_coin, address)
        .build_and_sign(&new_validator.account_key_pair);
    test_cluster.execute_transaction(stake_tx).await;

    let gas = test_cluster.wallet.get_object_ref(gas.0).await.unwrap();
    let tx = TestTransactionBuilder::new(address, gas, rgp)
        .call_request_add_validator()
        .build_and_sign(&new_validator.account_key_pair);
    test_cluster.execute_transaction(tx).await;

    // Check that we can get the pending validator from 0x5.
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let pending_active_validators = system_state
            .get_pending_active_validators(node.state().get_object_store().as_ref())
            .unwrap();
        assert_eq!(pending_active_validators.len(), pending_active_count + 1);
        assert_eq!(
            pending_active_validators[pending_active_validators.len() - 1].sui_address,
            address
        );
    });
}
