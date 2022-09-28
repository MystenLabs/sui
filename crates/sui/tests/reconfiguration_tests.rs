// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use multiaddr::Multiaddr;
use prometheus::Registry;
use std::time::Duration;
use sui_config::ValidatorInfo;
use sui_core::authority_active::checkpoint_driver::{
    checkpoint_process_step, CheckpointProcessControl,
};
use sui_node::SuiNode;
use sui_types::base_types::{ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair, AuthorityKeyPair, AuthoritySignature,
    KeypairTraits, NetworkKeyPair,
};
use sui_types::error::SuiResult;
use sui_types::messages::{CallArg, ObjectArg, TransactionEffects};
use sui_types::object::Object;
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;
use test_utils::authority::{get_object, test_authority_configs};
use test_utils::messages::{make_transfer_sui_transaction, move_transaction};
use test_utils::objects::{generate_gas_object_with_balance, test_gas_objects};
use test_utils::test_account_keys;
use test_utils::transaction::{submit_shared_object_transaction, submit_single_owner_transaction};

#[tokio::test(flavor = "current_thread")]
async fn reconfig_end_to_end_tests() {
    telemetry_subscribers::init_for_testing();

    let mut configs = test_authority_configs();
    for c in configs.validator_configs.iter_mut() {
        // Turn off checkpoint process so that we can have fine control over it in the test.
        c.enable_checkpoint = false;
    }
    let validator_info = configs.validator_set();
    let mut gas_objects = test_gas_objects();
    let validator_stake = generate_gas_object_with_balance(100000000000000);
    let mut states = Vec::new();
    let mut nodes = Vec::new();
    for validator in configs.validator_configs() {
        let node = SuiNode::start(validator, Registry::new()).await.unwrap();
        let state = node.state();

        for gas in gas_objects.clone() {
            state.insert_genesis_object(gas).await;
        }
        state.insert_genesis_object(validator_stake.clone()).await;
        states.push(state);
        nodes.push(node);
    }

    // get sui system state and confirm it matches network info
    let sui_system_state = states[0].get_sui_system_state_object().await.unwrap();
    let mut net_addrs_from_chain: Vec<Multiaddr> = Vec::new();
    let old_committee_size = sui_system_state.validators.next_epoch_validators.len();
    for validator in sui_system_state.validators.active_validators {
        let address = Multiaddr::try_from(validator.metadata.net_address);
        net_addrs_from_chain.push(address.unwrap());
    }
    let mut net_addrs_from_config = Vec::new();
    for validator in configs.validator_configs() {
        net_addrs_from_config.push(validator.network_address.clone());
    }
    assert_eq!(net_addrs_from_config.len(), net_addrs_from_chain.len());
    net_addrs_from_config.sort();
    net_addrs_from_chain.sort();
    for (conf, chain) in net_addrs_from_config.iter().zip(&net_addrs_from_chain) {
        assert_eq!(conf, chain);
    }

    let effects = create_and_register_new_validator(
        states[0].get_framework_object_ref().await.unwrap(),
        &mut gas_objects,
        validator_stake.compute_object_reference(),
        validator_info,
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());

    let sui_system_state = states[0].get_sui_system_state_object().await.unwrap();
    let new_committee_size = sui_system_state.validators.next_epoch_validators.len();
    assert_eq!(old_committee_size + 1, new_committee_size);

    fast_forward_to_ready_for_reconfig_start(&nodes).await;

    // Start epoch change and halt all validators.
    for node in &nodes {
        node.active().start_epoch_change().await.unwrap();
    }

    fast_forward_to_ready_for_reconfig_finish(&nodes).await;

    let results: Vec<_> = nodes
        .iter()
        .map(|node| async {
            node.active().finish_epoch_change().await.unwrap();
        })
        .collect();

    futures::future::join_all(results).await;

    // refresh the system state and network addresses
    let sui_system_state = states[0].get_sui_system_state_object().await.unwrap();
    assert_eq!(sui_system_state.epoch, 1);
    // We should now have one more active validator.
    assert_eq!(sui_system_state.validators.active_validators.len(), 5);
}

#[tokio::test(flavor = "current_thread")]
async fn reconfig_last_checkpoint_sync_missing_tx() {
    telemetry_subscribers::init_for_testing();

    let mut configs = test_authority_configs();
    for c in configs.validator_configs.iter_mut() {
        // Turn off checkpoint process so that we can have fine control over it in the test.
        c.enable_checkpoint = false;
    }
    let validator_info = configs.validator_set();
    let mut gas_objects = test_gas_objects();
    let mut states = Vec::new();
    let mut nodes = Vec::new();
    for validator in configs.validator_configs() {
        let node = SuiNode::start(validator, Registry::new()).await.unwrap();
        let state = node.state();

        for gas in gas_objects.clone() {
            state.insert_genesis_object(gas).await;
        }
        states.push(state);
        nodes.push(node);
    }

    fast_forward_to_ready_for_reconfig_start(&nodes).await;

    let (sender, key_pair) = test_account_keys().pop().unwrap();
    let object_ref = gas_objects.pop().unwrap().compute_object_reference();
    let transaction = make_transfer_sui_transaction(
        object_ref,
        SuiAddress::random_for_testing_only(),
        None,
        sender,
        &key_pair,
    );
    // Only send the transaction to validator 0, but not other validators.
    // Since gossip is disabled by default, validator 1-3 will not see it.
    submit_single_owner_transaction(transaction, &validator_info[0..1]).await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    for (idx, validator) in validator_info.iter().enumerate() {
        // Check that the object is mutated on validator 0 only.
        assert_eq!(
            get_object(validator, object_ref.0).await.version(),
            SequenceNumber::from(if idx == 0 { 1 } else { 0 })
        );
    }

    // Start epoch change and halt all validators.
    for node in &nodes {
        node.active().start_epoch_change().await.unwrap();
    }

    // Create a proposal on validator 0, which ensures that the transaction above will be included
    // in the checkpoint.
    nodes[0]
        .state()
        .checkpoints
        .as_ref()
        .unwrap()
        .lock()
        .set_proposal(0)
        .unwrap();
    let mut checkpoint_processes = vec![];
    // Only validator 1 and 2 will participate the checkpoint progress, which will use fragments
    // involving validator 0, 1, 2. Since validator 1 and 2 don't have the above transaction
    // executed, they will actively sync and execute it. This exercises the code path where we can
    // execute a transaction from a pending checkpoint even when validator is halted.
    for node in &nodes[1..3] {
        let active = node.active().clone();
        let handle = tokio::spawn(async move {
            while !active
                .state
                .checkpoints
                .as_ref()
                .unwrap()
                .lock()
                .is_ready_to_finish_epoch_change()
            {
                let _ =
                    checkpoint_process_step(active.clone(), &CheckpointProcessControl::default())
                        .await;
            }
        });
        checkpoint_processes.push(handle);
    }
    // Wait for all validators to be ready for epoch change.
    join_all(checkpoint_processes).await;

    // Now that we have a new checkpoint cert formed for the last checkpoint, check that
    // validator 3 is able to also sync and execute the above transaction and finish epoch change.
    // This exercises the code path where a validator can execute transactions from a checkpoint
    // cert even when the validator is halted.
    while !nodes[3]
        .state()
        .checkpoints
        .as_ref()
        .unwrap()
        .lock()
        .is_ready_to_finish_epoch_change()
    {
        let _ = checkpoint_process_step(
            nodes[3].active().clone(),
            &CheckpointProcessControl::default(),
        )
        .await;
    }
}

async fn create_and_register_new_validator(
    framework_pkg: ObjectRef,
    gas_objects: &mut Vec<Object>,
    validator_stake: ObjectRef,
    validator_info: &[ValidatorInfo],
) -> SuiResult<TransactionEffects> {
    let (new_validator, new_validator_pop) = get_new_validator();

    let validator_tx = move_transaction(
        gas_objects.pop().unwrap(),
        "sui_system",
        "request_add_validator",
        framework_pkg,
        vec![
            CallArg::Object(ObjectArg::SharedObject(SUI_SYSTEM_STATE_OBJECT_ID)),
            CallArg::Pure(bcs::to_bytes(&new_validator.protocol_key()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&new_validator.network_key()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&new_validator_pop.as_ref()).unwrap()),
            CallArg::Pure(
                bcs::to_bytes(format!("Validator{}", new_validator.sui_address()).as_bytes())
                    .unwrap(),
            ),
            CallArg::Pure(bcs::to_bytes(&new_validator.network_address).unwrap()),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(validator_stake)),
            CallArg::Pure(bcs::to_bytes(&new_validator.gas_price()).unwrap()),
        ],
    );
    submit_shared_object_transaction(validator_tx, validator_info).await
}

pub fn get_new_validator() -> (ValidatorInfo, AuthoritySignature) {
    let keypair: AuthorityKeyPair = get_key_pair().1;
    let worker_keypair: NetworkKeyPair = get_key_pair().1;
    let network_keypair: NetworkKeyPair = get_key_pair().1;
    let account_keypair = test_account_keys().pop().unwrap().1;
    let pop = generate_proof_of_possession(&keypair, account_keypair.public().into());
    (
        ValidatorInfo {
            name: "".to_string(),
            protocol_key: keypair.public().into(),
            worker_key: worker_keypair.public().clone(),
            account_key: account_keypair.public().clone().into(),
            network_key: network_keypair.public().clone(),
            stake: 1,
            delegation: 0,
            gas_price: 1,
            network_address: sui_config::utils::new_network_address(),
            narwhal_primary_address: sui_config::utils::new_network_address(),
            narwhal_worker_address: sui_config::utils::new_network_address(),
            narwhal_consensus_address: sui_config::utils::new_network_address(),
        },
        pop,
    )
}

async fn fast_forward_to_ready_for_reconfig_start(nodes: &[SuiNode]) {
    let mut checkpoint_processes = vec![];
    for node in nodes {
        let active = node.active().clone();
        let handle = tokio::spawn(async move {
            while !active
                .state
                .checkpoints
                .as_ref()
                .unwrap()
                .lock()
                .is_ready_to_start_epoch_change()
            {
                let _ =
                    checkpoint_process_step(active.clone(), &CheckpointProcessControl::default())
                        .await;
            }
        });
        checkpoint_processes.push(handle);
    }
    // Wait for all validators to be ready for epoch change.
    join_all(checkpoint_processes).await;
}

async fn fast_forward_to_ready_for_reconfig_finish(nodes: &[SuiNode]) {
    let mut checkpoint_processes = vec![];
    for node in nodes {
        let active = node.active().clone();
        let handle = tokio::spawn(async move {
            while !active
                .state
                .checkpoints
                .as_ref()
                .unwrap()
                .lock()
                .is_ready_to_finish_epoch_change()
            {
                let _ =
                    checkpoint_process_step(active.clone(), &CheckpointProcessControl::default())
                        .await;
            }
        });
        checkpoint_processes.push(handle);
    }
    // Wait for all validators to be ready for epoch change.
    join_all(checkpoint_processes).await;
}
