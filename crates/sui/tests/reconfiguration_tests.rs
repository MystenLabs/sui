// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::Ed25519KeyPair;
use futures::future::join_all;
use move_core_types::ident_str;
use mysten_metrics::RegistryService;
use prometheus::Registry;
use rand::{rngs::StdRng, SeedableRng};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_config::builder::ConfigBuilder;
use sui_config::NodeConfig;
use sui_core::authority_aggregator::{AuthAggMetrics, AuthorityAggregator};
use sui_core::consensus_adapter::position_submit_certificate;
use sui_core::safe_client::SafeClientMetricsBase;
use sui_core::test_utils::make_transfer_sui_transaction;
use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_types::base_types::{AuthorityName, ObjectRef, SuiAddress};
use sui_types::crypto::{
    generate_proof_of_possession, get_account_key_pair, get_key_pair_from_rng, AccountKeyPair,
    KeypairTraits, ToFromBytes,
};
use sui_types::effects::{CertifiedTransactionEffects, TransactionEffectsAPI};
use sui_types::error::SuiError;
use sui_types::gas::GasCostSummary;
use sui_types::message_envelope::Message;
use sui_types::messages::{
    CallArg, ObjectArg, TransactionData, TransactionDataAPI, TransactionExpiration,
    VerifiedTransaction, TEST_ONLY_GAS_UNIT_FOR_GENERIC, TEST_ONLY_GAS_UNIT_FOR_STAKING,
    TEST_ONLY_GAS_UNIT_FOR_TRANSFER, TEST_ONLY_GAS_UNIT_FOR_VALIDATOR,
};
use sui_types::object::{
    generate_test_gas_objects_with_owner, generate_test_gas_objects_with_owner_and_value, Object,
};
use sui_types::sui_system_state::sui_system_state_inner_v1::VerifiedValidatorMetadataV1;
use sui_types::sui_system_state::{
    get_validator_from_table, sui_system_state_summary::get_validator_by_pool_id,
    SuiSystemStateTrait,
};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    SUI_SYSTEM_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use test_utils::authority::start_node;
use test_utils::{
    authority::{
        spawn_test_authorities, test_authority_configs, test_authority_configs_with_objects,
    },
    network::TestClusterBuilder,
};
use tokio::time::{sleep, timeout};
use tracing::{info, warn};

#[sim_test]
async fn advance_epoch_tx_test() {
    let authorities = spawn_test_authorities(&test_authority_configs()).await;
    let states: Vec<_> = authorities
        .iter()
        .map(|authority| authority.with(|node| node.state()))
        .collect();
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
    let authorities = spawn_test_authorities(&test_authority_configs()).await;
    trigger_reconfiguration(&authorities).await;
}

#[sim_test]
async fn test_transaction_expiration() {
    let (sender, keypair) = get_account_key_pair();
    let gas = Object::with_owner_for_testing(sender);
    let (configs, objects) = test_authority_configs_with_objects([gas]);
    let rgp = configs.genesis.reference_gas_price();
    let authorities = spawn_test_authorities(&configs).await;
    trigger_reconfiguration(&authorities).await;

    let mut data = TransactionData::new_transfer_sui(
        sender,
        sender,
        Some(1),
        objects[0].compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    // Expired transaction returns an error
    let mut expired_data = data.clone();
    *expired_data.expiration_mut_for_testing() = TransactionExpiration::Epoch(0);
    let expired_transaction = to_sender_signed_transaction(expired_data, &keypair);
    let result = authorities[0]
        .with_async(|node| async {
            let epoch_store = node.state().epoch_store_for_testing();
            node.state()
                .handle_transaction(&epoch_store, expired_transaction)
                .await
        })
        .await;
    assert!(matches!(result.unwrap_err(), SuiError::TransactionExpired));

    // Non expired transaction signed without issue
    *data.expiration_mut_for_testing() = TransactionExpiration::Epoch(10);
    let transaction = to_sender_signed_transaction(data, &keypair);
    authorities[0]
        .with_async(|node| async {
            let epoch_store = node.state().epoch_store_for_testing();
            node.state()
                .handle_transaction(&epoch_store, transaction)
                .await
        })
        .await
        .unwrap();
}

#[sim_test]
async fn reconfig_with_revert_end_to_end_test() {
    let (sender, keypair) = get_account_key_pair();
    let gas1 = Object::with_owner_for_testing(sender); // committed
    let gas2 = Object::with_owner_for_testing(sender); // (most likely) reverted
    let (configs, objects) = test_authority_configs_with_objects([gas1, gas2]);
    let gas1 = &objects[0];
    let gas2 = &objects[1];
    let authorities = spawn_test_authorities(&configs).await;
    let registry = Registry::new();
    let rgp = authorities
        .get(0)
        .unwrap()
        .with(|sui_node| sui_node.state().reference_gas_price_for_testing())
        .unwrap();

    // gas1 transaction is committed
    let tx = make_transfer_sui_transaction(
        gas1.compute_object_reference(),
        sender,
        None,
        sender,
        &keypair,
        rgp,
    );
    let net = AuthorityAggregator::new_from_local_system_state(
        &authorities[0].with(|node| node.state().db()),
        &authorities[0].with(|node| node.state().committee_store().clone()),
        SafeClientMetricsBase::new(&registry),
        AuthAggMetrics::new(&registry),
    )
    .unwrap();
    let cert = net
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();
    let (effects1, _, _) = net
        .process_certificate(cert.clone().into_inner())
        .await
        .unwrap();
    assert_eq!(0, effects1.epoch());

    // gas2 transaction is (most likely) reverted
    let tx = make_transfer_sui_transaction(
        gas2.compute_object_reference(),
        sender,
        None,
        sender,
        &keypair,
        rgp,
    );
    let cert = net
        .process_transaction(tx.clone())
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
        .handle_certificate(cert.clone().into_inner())
        .await
        .unwrap();

    authorities[reverting_authority_idx]
        .with_async(|node| async {
            let object = node
                .state()
                .get_objects(&[gas2.id()])
                .await
                .unwrap()
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
                    .get_objects(&[gas1.id()])
                    .await
                    .unwrap()
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
                    .get_objects(&[gas2.id()])
                    .await
                    .unwrap()
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
    telemetry_subscribers::init_for_testing();
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(1000)
        .build()
        .await
        .unwrap();

    let target_epoch: u64 = std::env::var("RECONFIG_TARGET_EPOCH")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(4);

    test_cluster.wait_for_epoch(Some(target_epoch)).await;

    test_cluster
        .swarm
        .validators()
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
            assert_eq!(commitments.len(), 0);
        });
}

// This test just starts up a cluster that reconfigures itself under 0 load.
#[cfg(msim)]
#[sim_test]
async fn test_create_advance_epoch_tx_race() {
    use std::sync::Arc;
    use sui_macros::{register_fail_point, register_fail_point_async};
    use tokio::sync::broadcast;

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
        .await
        .unwrap();

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
            .await
            .unwrap(),
    );

    let _restarter_handle = test_cluster
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
    let (sender, keypair) = get_account_key_pair();
    let gas = Object::with_owner_for_testing(sender);
    let (configs, mut objects) = test_authority_configs_with_objects([gas]);
    let gas = objects.pop().unwrap();
    let authorities = spawn_test_authorities(&configs).await;
    let rgp = configs.genesis.reference_gas_price();
    let tx = make_transfer_sui_transaction(
        gas.compute_object_reference(),
        sender,
        None,
        sender,
        &keypair,
        rgp,
    );
    let registry = Registry::new();
    let mut net = AuthorityAggregator::new_from_local_system_state(
        &authorities[0].with(|node| node.state().db()),
        &authorities[0].with(|node| node.state().committee_store().clone()),
        SafeClientMetricsBase::new(&registry),
        AuthAggMetrics::new(&registry),
    )
    .unwrap();
    let cert = net
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();
    let (effects0, _, _) = net
        .process_certificate(cert.clone().into_inner())
        .await
        .unwrap();
    assert_eq!(effects0.epoch(), 0);
    // Give it enough time for the transaction to be checkpointed and hence finalized.
    sleep(Duration::from_secs(10)).await;
    trigger_reconfiguration(&authorities).await;
    // Manually reconfigure the aggregator.
    net.committee.epoch = 1;
    let (effects1, _, _) = net.process_certificate(cert.into_inner()).await.unwrap();
    // Ensure that we are able to form a new effects cert in the new epoch.
    assert_eq!(effects1.epoch(), 1);
    assert_eq!(effects0.into_message(), effects1.into_message());
}

#[sim_test]
async fn test_validator_candidate_pool_read() {
    let new_validator_key = gen_keys(5).pop().unwrap();
    let new_validator_address: SuiAddress = new_validator_key.public().into();

    let gas_objects =
        generate_test_gas_objects_with_owner_and_value(4, new_validator_address, 100_000_000_000);

    let init_configs = ConfigBuilder::new_with_temp_dir()
        .rng(StdRng::from_seed([0; 32]))
        .with_validator_account_keys(gen_keys(4))
        .with_objects(gas_objects.clone())
        .build();

    let new_configs = ConfigBuilder::new_with_temp_dir()
        .rng(StdRng::from_seed([0; 32]))
        .with_validator_account_keys(gen_keys(5))
        .with_objects(gas_objects.clone())
        .build();

    let gas_objects: Vec<_> = gas_objects
        .into_iter()
        .map(|o| init_configs.genesis.object(o.id()).unwrap())
        .collect();

    // Generate a new validator config.
    let public_keys: HashSet<_> = init_configs
        .validator_configs
        .iter()
        .map(|config| config.protocol_public_key())
        .collect();
    // Node configs contain things such as private keys, which we need to send transactions.
    let new_node_config = new_configs
        .validator_configs
        .iter()
        .find(|v| !public_keys.contains(&v.protocol_public_key()))
        .unwrap();
    // Validator information from genesis contains public network addresses that we need to commit on-chain.
    let new_validator = new_configs
        .genesis
        .validator_set_for_tooling()
        .into_iter()
        .find(|v| {
            let name: AuthorityName = v.verified_metadata().sui_pubkey_bytes();
            !public_keys.contains(&name)
        })
        .unwrap();

    let authorities = spawn_test_authorities(&init_configs).await;
    let _effects = execute_add_validator_candidate_tx(
        &authorities,
        gas_objects[3].compute_object_reference(),
        new_node_config,
        new_validator.verified_metadata(),
        &new_validator_key,
    )
    .await;

    // Trigger reconfiguration so that the candidate adding txn is executed on all authorities.
    trigger_reconfiguration(&authorities).await;

    // Check that the candidate can be found in the candidate table now.
    authorities[0].with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let system_state_summary = system_state.clone().into_sui_system_state_summary();
        assert_eq!(system_state_summary.validator_candidates_size, 1);
        let staking_pool_id = get_validator_from_table(
            node.state().db().as_ref(),
            system_state_summary.validator_candidates_id,
            &new_validator_address,
        )
        .unwrap()
        .staking_pool_id;
        let validator = get_validator_by_pool_id(
            node.state().db().as_ref(),
            &system_state,
            &system_state_summary,
            staking_pool_id,
        )
        .unwrap();
        assert_eq!(validator.sui_address, new_validator_address);
    });
}

#[sim_test]
async fn test_inactive_validator_pool_read() {
    let leaving_validator_account_key = gen_keys(5).pop().unwrap();
    let address: SuiAddress = leaving_validator_account_key.public().into();

    let gas_objects = generate_test_gas_objects_with_owner(1, address);
    let stake = Object::new_gas_with_balance_and_owner_for_testing(25_000_000_000_000_000, address);
    let mut genesis_objects = vec![stake];
    genesis_objects.extend(gas_objects.clone());

    let init_configs = ConfigBuilder::new_with_temp_dir()
        .rng(StdRng::from_seed([0; 32]))
        .with_validator_account_keys(gen_keys(5))
        .with_objects(genesis_objects.clone())
        .build();

    let gas_objects: Vec<_> = gas_objects
        .into_iter()
        .map(|o| init_configs.genesis.object(o.id()).unwrap())
        .collect();

    let authorities = spawn_test_authorities(&init_configs).await;
    let rgp = init_configs.genesis.reference_gas_price();

    let staking_pool_id = authorities[0].with(|node| {
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
    authorities[0].with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let system_state_summary = system_state.clone().into_sui_system_state_summary();
        // Validator is active. Check that we can find its summary by staking pool id.
        let validator = get_validator_by_pool_id(
            node.state().db().as_ref(),
            &system_state,
            &system_state_summary,
            staking_pool_id,
        )
        .unwrap();
        assert_eq!(validator.sui_address, address);
    });

    let tx_data = TransactionData::new_move_call(
        address,
        SUI_SYSTEM_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!("request_remove_validator").to_owned(),
        vec![],
        gas_objects[0].compute_object_reference(),
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: SUI_SYSTEM_STATE_OBJECT_ID,
            initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
            mutable: true,
        })],
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(tx_data, &leaving_validator_account_key);
    let effects = execute_transaction_block(&authorities, transaction)
        .await
        .unwrap();
    assert!(effects.status().is_ok());

    trigger_reconfiguration(&authorities).await;

    // Check that the validator that just left now shows up in the inactive_validators,
    // and we can still deserialize it and get the inactive staking pool.
    authorities[0].with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let system_state_summary = system_state.clone().into_sui_system_state_summary();
        let validator = get_validator_by_pool_id(
            node.state().db().as_ref(),
            &system_state,
            &system_state_summary,
            staking_pool_id,
        )
        .unwrap();
        assert_eq!(validator.sui_address, address);
        assert!(validator.staking_pool_deactivation_epoch.is_some());
    })
}

// generate N keys - use a fixed RNG so we can regenerate the same set again later (keypairs
// are not Clone)
fn gen_keys(count: usize) -> Vec<AccountKeyPair> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..count)
        .map(|_| get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng).1)
        .collect()
}

#[sim_test]
async fn test_reconfig_with_committee_change_basic() {
    // This test exercise the full flow of a validator joining the network, catch up and then leave.

    let new_validator_key = gen_keys(5).pop().unwrap();
    let new_validator_address: SuiAddress = new_validator_key.public().into();

    // TODO: In order to better "test" this flow we probably want to set the validators to ignore
    // all p2p peer connections so that we can verify that new nodes joining can really "talk" with the
    // other validators in the set.
    let gas_objects =
        generate_test_gas_objects_with_owner_and_value(4, new_validator_address, 100_000_000_000);
    let stake = Object::new_gas_with_balance_and_owner_for_testing(
        30_000_000_000_000_000,
        new_validator_address,
    );
    let mut objects = vec![stake.clone()];
    objects.extend(gas_objects.clone());

    let init_configs = ConfigBuilder::new_with_temp_dir()
        .rng(StdRng::from_seed([0; 32]))
        .with_validator_account_keys(gen_keys(4))
        .with_objects(objects.clone())
        .build();

    let new_configs = ConfigBuilder::new_with_temp_dir()
        .rng(StdRng::from_seed([0; 32]))
        .with_validator_account_keys(gen_keys(5))
        .with_objects(objects.clone())
        .build();

    let gas_objects: Vec<_> = gas_objects
        .into_iter()
        .map(|o| init_configs.genesis.object(o.id()).unwrap())
        .collect();

    let stake = init_configs.genesis.object(stake.id()).unwrap();

    // Generate a new validator config.
    // Our committee generation uses a fixed seed, so we need to generate a new committee
    // with one extra validator.
    // Furthermore, since the order is not fixed, we need to find the new validator
    // that doesn't exist in the previous committee manually.
    // The order of validator_set() and validator_configs() is also different.
    // TODO: We should really fix the above inconveniences.
    let public_keys: HashSet<_> = init_configs
        .validator_configs
        .iter()
        .map(|config| config.protocol_public_key())
        .collect();
    // Node configs contain things such as private keys, which we need to send transactions.
    let new_node_config = new_configs
        .validator_configs
        .iter()
        .find(|v| !public_keys.contains(&v.protocol_public_key()))
        .unwrap();
    // Validator information from genesis contains public network addresses that we need to commit on-chain.
    let new_validator = new_configs
        .genesis
        .validator_set_for_tooling()
        .into_iter()
        .find(|v| {
            let name: AuthorityName = v.verified_metadata().sui_pubkey_bytes();
            !public_keys.contains(&name)
        })
        .unwrap();
    info!(
        "New validator is: {:?}",
        new_validator
            .verified_metadata()
            .sui_pubkey_bytes()
            .concise()
    );

    let mut authorities = spawn_test_authorities(&init_configs).await;

    let _effects = execute_join_committee_txes(
        &authorities,
        gas_objects
            .clone()
            .into_iter()
            .map(|obj| obj.compute_object_reference())
            .collect::<Vec<_>>(),
        stake.compute_object_reference(),
        new_node_config,
        new_validator.verified_metadata(),
        &new_validator_key,
    )
    .await;

    // Give the nodes enough time to execute the joining txns.
    sleep(Duration::from_secs(5)).await;

    // Check that we can get the pending validator from 0x5.
    authorities[0].with(|node| {
        let system_state = node
            .state()
            .get_sui_system_state_object_for_testing()
            .unwrap();
        let pending_active_validators = system_state
            .get_pending_active_validators(node.state().db().as_ref())
            .unwrap();
        assert_eq!(pending_active_validators.len(), 1);
        assert_eq!(
            pending_active_validators[0].sui_address,
            new_validator_address
        );
    });

    trigger_reconfiguration(&authorities).await;
    // Check that a new validator has joined the committee.
    authorities[0].with(|node| {
        assert_eq!(
            node.state()
                .epoch_store_for_testing()
                .committee()
                .num_members(),
            5
        );
    });

    let mut new_node_config_clone = new_node_config.clone();
    // Make sure that the new validator config shares the same genesis as the initial one.
    new_node_config_clone.genesis = init_configs.validator_configs[0].genesis.clone();
    let handle = start_node(
        &new_node_config_clone,
        RegistryService::new(Registry::new()),
    )
    .await;
    // Give the new validator enough time to catch up and sync.
    // TODO: 30s is still flaky.
    tokio::time::sleep(Duration::from_secs(30)).await;
    handle.with(|node| {
        // Eventually the validator will catch up to the new epoch and become part of the committee.
        assert!(node
            .state()
            .is_validator(&node.state().epoch_store_for_testing()));
    });

    let _effects = execute_leave_committee_tx(
        &authorities,
        gas_objects[3].compute_object_reference(),
        &new_validator_key,
    )
    .await;

    authorities.push(handle);
    trigger_reconfiguration(&authorities).await;

    // Check that this validator has left the committee, and is no longer a validator.
    authorities[4].with(|node| {
        assert_eq!(
            node.state()
                .epoch_store_for_testing()
                .committee()
                .num_members(),
            4
        );
        assert!(node
            .state()
            .is_fullnode(&node.state().epoch_store_for_testing()));
    })
}

#[sim_test]
async fn test_reconfig_with_committee_change_stress() {
    // This needs to be written to genesis for all validators, present and future
    // (either that or we create these objects via Transaction later, but that's more work)
    let all_validator_keys = gen_keys(11);
    let address_key_mapping: BTreeMap<SuiAddress, Ed25519KeyPair> = all_validator_keys
        .iter()
        .map(|key| (key.public().into(), key.copy()))
        .collect();

    let object_set: HashMap<SuiAddress, (Vec<Object>, Object)> = all_validator_keys
        .iter()
        .map(|key| {
            let sender: SuiAddress = key.public().into();
            let gas_objects =
                generate_test_gas_objects_with_owner_and_value(4, sender, 100_000_000_000);
            let stake =
                Object::new_gas_with_balance_and_owner_for_testing(30_000_000_000_000_000, sender);
            (sender, (gas_objects, stake))
        })
        .collect();

    let genesis_objects: Vec<_> = object_set
        .values()
        .flat_map(|(g, s)| {
            let mut objs = g.clone();
            objs.push(s.clone());
            objs
        })
        .collect();

    let initial_network = ConfigBuilder::new_with_temp_dir()
        .rng(StdRng::from_seed([0; 32]))
        .with_validator_account_keys(gen_keys(5))
        .with_objects(genesis_objects.clone())
        .build();

    let mut object_map: HashMap<SuiAddress, (Vec<ObjectRef>, ObjectRef)> = object_set
        .into_iter()
        .map(|(sender, (gas, stake))| {
            (
                sender,
                (
                    gas.into_iter()
                        .map(|obj| {
                            initial_network
                                .genesis
                                .object(obj.id())
                                .unwrap()
                                .compute_object_reference()
                        })
                        .collect::<Vec<_>>(),
                    initial_network
                        .genesis
                        .object(stake.id())
                        .unwrap()
                        .compute_object_reference(),
                ),
            )
        })
        .collect();

    // Network config composed of the join of all committees that will
    // exist at any point during this test. Each NetworkConfig epoch committee
    // will be a subset of this
    let validator_superset = ConfigBuilder::new_with_temp_dir()
        .rng(StdRng::from_seed([0; 32]))
        .with_validator_account_keys(all_validator_keys)
        .with_objects(genesis_objects.clone())
        .build();
    let validator_superset_mapping = validator_superset
        .genesis
        .validator_set_for_tooling()
        .into_iter()
        .map(|v| {
            let name: AuthorityName = v.verified_metadata().sui_pubkey_bytes();
            (name, v.verified_metadata().sui_address)
        })
        .collect::<HashMap<_, _>>();

    let mut validator_handles = spawn_test_authorities(&initial_network).await;
    assert_eq!(validator_handles.len(), 5);

    let initial_keys: HashSet<_> = initial_network
        .validator_configs()
        .iter()
        .map(|config| config.protocol_public_key())
        .collect();

    // start all the other nodes (they should start as fullnodes as they are not
    // yet in the committee)
    let fullnode_futures: Vec<_> = validator_superset
        .validator_configs()
        .iter()
        .filter(|config| !initial_keys.contains(&config.protocol_public_key()))
        .map(|config| async {
            // Make sure that the new validator config shares the same genesis as the initial one.
            let mut new_config = config.clone();
            new_config.genesis = initial_network.validator_configs()[0].genesis.clone();
            start_node(&new_config, RegistryService::new(Registry::new())).await
        })
        .collect();
    let mut fullnode_handles = futures::future::join_all(fullnode_futures).await;
    for handle in &fullnode_handles {
        handle.with(|node| {
            assert!(node
                .state()
                .is_fullnode(&node.state().epoch_store_for_testing()));
        });
    }

    assert_eq!(validator_handles.len(), 5);
    assert_eq!(fullnode_handles.len(), 6);

    // give time for authorities to startup and genesis
    tokio::time::sleep(Duration::from_secs(5)).await;

    let initial_pubkeys: Vec<_> = initial_network
        .genesis
        .validator_set_for_tooling()
        .iter()
        .map(|v| v.verified_metadata().sui_pubkey_bytes())
        .collect();
    let mut standby_nodes: Vec<_> = validator_superset
        .genesis
        .validator_set_for_tooling()
        .into_iter()
        .map(|val| {
            let node_config = validator_superset
                .validator_configs()
                .iter()
                .find(|config| {
                    config.protocol_public_key() == val.verified_metadata().sui_pubkey_bytes()
                })
                .unwrap();
            (val, node_config)
        })
        .filter(|(val, _node)| {
            !initial_pubkeys.contains(&val.verified_metadata().sui_pubkey_bytes())
        })
        .collect();
    assert_eq!(standby_nodes.len(), 6);

    let mut epoch = 0;

    while !standby_nodes.is_empty() {
        // Add 2 validators and remove 2 validators to/from the committee
        // per iteration (epoch)

        let joining_validators = standby_nodes.split_off(standby_nodes.len() - 2);
        assert_eq!(joining_validators.len(), 2);

        // request to add new validators
        for (validator, node_config) in joining_validators.clone() {
            let sender = validator.verified_metadata().sui_address;
            let (gas_objects, stake) = object_map.get(&sender).unwrap();
            let effects = execute_join_committee_txes(
                &validator_handles,
                gas_objects.clone(),
                *stake,
                node_config,
                validator.verified_metadata(),
                address_key_mapping.get(&sender).unwrap(),
            )
            .await;

            let gas_objects = vec![
                effects[0].gas_object().0,
                effects[1].gas_object().0,
                effects[2].gas_object().0,
                gas_objects[3],
            ];
            object_map.insert(sender, (gas_objects, *stake));
        }

        // last 2 validators request to leave
        let auth_len = validator_handles.len();
        for auth in &validator_handles[auth_len - 2..] {
            let name = auth.with(|node| node.state().name);
            let sender = validator_superset_mapping.get(&name).unwrap();
            let (gas_objects, stake) = object_map.get(sender).unwrap();
            let effects = execute_leave_committee_tx(
                &validator_handles,
                gas_objects[3],
                address_key_mapping.get(sender).unwrap(),
            )
            .await;

            let gas_objects = vec![
                gas_objects[0],
                gas_objects[1],
                gas_objects[2],
                effects.gas_object().0,
            ];
            object_map.insert(*sender, (gas_objects, *stake));
        }

        trigger_reconfiguration(&validator_handles).await;
        epoch += 1;

        // allow time for reconfig
        tokio::time::sleep(Duration::from_secs(30)).await;

        // bookkeeping and verification for joined validators
        let joined_auths: Vec<_> = joining_validators
            .iter()
            .map(|(val, _node)| val.verified_metadata().sui_pubkey_bytes())
            .collect();

        assert_eq!(joined_auths.len(), 2);

        // find handles for nodes that joined, check that they are now
        // fullnodes, and if so, remove them from the fullnodes list
        // and add to the validator list
        for name in joined_auths.into_iter() {
            let pos = fullnode_handles
                .iter()
                .position(|handle| handle.with(|node| node.state().name == name))
                .unwrap();
            let handle = fullnode_handles.remove(pos);
            handle
                .with_async(|node| async {
                    assert_eq!(node.state().epoch_store_for_testing().epoch(), epoch);
                    assert!(node
                        .state()
                        .is_validator(&node.state().epoch_store_for_testing()));
                })
                .await;

            // insert to the beginning, as the end currently contains the validators
            // that requested to leave
            validator_handles.insert(0, handle);
        }

        // Bookkeeping and verification for left validators

        // The last two validator_handles were the ones that left the committee
        let mut left_auth_handles = validator_handles.split_off(validator_handles.len() - 2);
        assert_eq!(left_auth_handles.len(), 2);

        for handle in &left_auth_handles {
            handle.with(|node| {
                assert!(node
                    .state()
                    .is_fullnode(&node.state().epoch_store_for_testing()));
            });
        }
        fullnode_handles.push(left_auth_handles.pop().unwrap());
        fullnode_handles.push(left_auth_handles.pop().unwrap());

        // Check that new validators have joined the committee.
        let valdator_pubkeys: Vec<_> = validator_handles
            .iter()
            .map(|auth| auth.with(|node| node.state().name))
            .collect();
        validator_handles.last().unwrap().with(|node| {
            assert_eq!(
                node.state()
                    .epoch_store_for_testing()
                    .committee()
                    .num_members(),
                5
            );
            for key in valdator_pubkeys.iter() {
                assert!(node
                    .state()
                    .epoch_store_for_testing()
                    .committee()
                    .authority_exists(key));
            }
        });
    }
    assert_eq!(epoch, 3);
}

#[cfg(msim)]
#[sim_test]
async fn safe_mode_reconfig_test() {
    use sui_types::sui_system_state::advance_epoch_result_injection;
    use test_utils::messages::make_staking_transaction_with_wallet_context;

    // Inject failure at epoch change 1 -> 2.
    advance_epoch_result_injection::set_override(Some((2, 3)));

    let mut test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(3000)
        .build()
        .await
        .unwrap();

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
    assert!(system_state.epoch_start_timestamp_ms() >= prev_epoch_start_timestamp + 5000);

    // Try a staking transaction.
    let validator_address = system_state
        .into_sui_system_state_summary()
        .active_validators[0]
        .sui_address;
    let txn =
        make_staking_transaction_with_wallet_context(test_cluster.wallet_mut(), validator_address)
            .await;
    let response = test_cluster
        .execute_transaction(txn)
        .await
        .expect("Staking txn failed");
    assert!(response.status_ok().unwrap());

    // Now remove the override and check that in the next epoch we are no longer in safe mode.
    test_cluster.set_safe_mode_expected(false);

    let system_state = test_cluster.wait_for_epoch(Some(3)).await;
    assert!(!system_state.safe_mode());
    assert_eq!(system_state.epoch(), 3);
    assert_eq!(system_state.system_state_version(), 2);
}

async fn execute_add_validator_candidate_tx(
    authorities: &[SuiNodeHandle],
    gas_object: ObjectRef,
    node_config: &NodeConfig,
    val: &VerifiedValidatorMetadataV1,
    account_kp: &Ed25519KeyPair,
) -> CertifiedTransactionEffects {
    let sender = val.sui_address;
    let proof_of_possession = generate_proof_of_possession(node_config.protocol_key_pair(), sender);
    let rgp = authorities[0]
        .with(|node| node.state().reference_gas_price_for_testing())
        .unwrap();
    let candidate_tx_data = TransactionData::new_move_call(
        sender,
        SUI_SYSTEM_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!("request_add_validator_candidate").to_owned(),
        vec![],
        gas_object,
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: SUI_SYSTEM_STATE_OBJECT_ID,
                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                mutable: true,
            }),
            CallArg::Pure(bcs::to_bytes(val.protocol_pubkey.as_bytes()).unwrap()),
            CallArg::Pure(bcs::to_bytes(val.network_pubkey.as_bytes()).unwrap()),
            CallArg::Pure(bcs::to_bytes(val.worker_pubkey.as_bytes()).unwrap()),
            CallArg::Pure(bcs::to_bytes(proof_of_possession.as_ref()).unwrap()),
            CallArg::Pure(bcs::to_bytes(val.name.as_bytes()).unwrap()),
            CallArg::Pure(bcs::to_bytes(val.description.as_bytes()).unwrap()),
            CallArg::Pure(bcs::to_bytes(val.image_url.as_bytes()).unwrap()),
            CallArg::Pure(bcs::to_bytes(val.project_url.as_bytes()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&val.net_address).unwrap()),
            CallArg::Pure(bcs::to_bytes(&val.p2p_address).unwrap()),
            CallArg::Pure(bcs::to_bytes(&val.primary_address).unwrap()),
            CallArg::Pure(bcs::to_bytes(&val.worker_address).unwrap()),
            CallArg::Pure(bcs::to_bytes(&1u64).unwrap()), // gas_price
            CallArg::Pure(bcs::to_bytes(&0u64).unwrap()), // commission_rate
        ],
        rgp * TEST_ONLY_GAS_UNIT_FOR_VALIDATOR,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(candidate_tx_data, account_kp);
    let effects = execute_transaction_block(authorities, transaction)
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    effects
}

async fn execute_join_committee_txes(
    authorities: &[SuiNodeHandle],
    gas_objects: Vec<ObjectRef>,
    stake: ObjectRef,
    node_config: &NodeConfig,
    val: &VerifiedValidatorMetadataV1,
    account_kp: &Ed25519KeyPair,
) -> Vec<CertifiedTransactionEffects> {
    assert_eq!(node_config.protocol_public_key(), val.sui_pubkey_bytes());
    let mut effects_ret = vec![];
    let sender = val.sui_address;
    let rgp = authorities[0]
        .with(|node| node.state().reference_gas_price_for_testing())
        .unwrap();
    // Step 1: Add the new node as a validator candidate.
    let effects = execute_add_validator_candidate_tx(
        authorities,
        gas_objects[0],
        node_config,
        val,
        account_kp,
    )
    .await;

    effects_ret.push(effects);

    // Step 2: Give the candidate enough stake.
    let stake_tx_data = TransactionData::new_move_call(
        sender,
        SUI_SYSTEM_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!("request_add_stake").to_owned(),
        vec![],
        gas_objects[1],
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: SUI_SYSTEM_STATE_OBJECT_ID,
                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                mutable: true,
            }),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(stake)),
            CallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
        rgp * TEST_ONLY_GAS_UNIT_FOR_STAKING,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(stake_tx_data, account_kp);
    let effects = execute_transaction_block(authorities, transaction)
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects);

    effects_ret.push(effects);

    // Step 3: Convert the candidate to an active valdiator.
    let activation_tx_data = TransactionData::new_move_call(
        sender,
        SUI_SYSTEM_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!("request_add_validator").to_owned(),
        vec![],
        gas_objects[2],
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: SUI_SYSTEM_STATE_OBJECT_ID,
            initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
            mutable: true,
        })],
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(activation_tx_data, account_kp);
    let effects = execute_transaction_block(authorities, transaction)
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    effects_ret.push(effects);

    effects_ret
}

async fn execute_leave_committee_tx(
    authorities: &[SuiNodeHandle],
    gas: ObjectRef,
    account_kp: &Ed25519KeyPair,
) -> CertifiedTransactionEffects {
    let sui_address: SuiAddress = account_kp.public().into();
    let rgp = authorities[0]
        .with(|node| node.state().reference_gas_price_for_testing())
        .unwrap();
    let tx_data = TransactionData::new_move_call(
        sui_address,
        SUI_SYSTEM_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!("request_remove_validator").to_owned(),
        vec![],
        gas,
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: SUI_SYSTEM_STATE_OBJECT_ID,
            initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
            mutable: true,
        })],
        rgp * TEST_ONLY_GAS_UNIT_FOR_VALIDATOR,
        rgp,
    )
    .unwrap();

    let transaction = to_sender_signed_transaction(tx_data, account_kp);
    let effects = execute_transaction_block(authorities, transaction)
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    effects
}

async fn trigger_reconfiguration(authorities: &[SuiNodeHandle]) {
    info!("Starting reconfiguration");
    let start = Instant::now();

    // Close epoch on 2f+1 validators.
    let cur_committee =
        authorities[0].with(|node| node.state().epoch_store_for_testing().committee().clone());
    let mut cur_stake = 0;
    for handle in authorities {
        handle
            .with_async(|node| async {
                node.close_epoch_for_testing().await.unwrap();
                cur_stake += cur_committee.weight(&node.state().name);
            })
            .await;
        if cur_stake >= cur_committee.quorum_threshold() {
            break;
        }
    }
    info!("close_epoch complete after {:?}", start.elapsed());

    // Wait for all nodes to reach the next epoch.
    let handles: Vec<_> = authorities
        .iter()
        .map(|handle| {
            handle.with_async(|node| async {
                let mut retries = 0;
                loop {
                    if node.state().epoch_store_for_testing().epoch() == cur_committee.epoch + 1 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    retries += 1;
                    if retries % 5 == 0 {
                        warn!(validator=?node.state().name.concise(), "Waiting for {:?} seconds for epoch change", retries);
                    }
                }
            })
        })
        .collect();

    timeout(Duration::from_secs(40), join_all(handles))
        .await
        .expect("timed out waiting for reconfiguration to complete");

    info!("reconfiguration complete after {:?}", start.elapsed());
}

async fn execute_transaction_block(
    authorities: &[SuiNodeHandle],
    transaction: VerifiedTransaction,
) -> anyhow::Result<CertifiedTransactionEffects> {
    let registry = Registry::new();
    let net = AuthorityAggregator::new_from_local_system_state(
        &authorities[0].with(|node| node.state().db()),
        &authorities[0].with(|node| node.state().committee_store().clone()),
        SafeClientMetricsBase::new(&registry),
        AuthAggMetrics::new(&registry),
    )
    .unwrap();
    net.execute_transaction_block(&transaction)
        .await
        .map(|e| e.into_inner())
}
